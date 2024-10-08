use std::{collections::HashSet, fs};
use base64::{engine::general_purpose, Engine};
use openssl::{error::ErrorStack, symm::{decrypt, Cipher}};

// Always operate on raw bytes, never on encoded strings. Only use hex and base64 for pretty-printing.

const LETTER_FREQ: [f64; 27] = [
    0.08167, 0.01492, 0.02782, 0.04253, 0.12702, 0.02228, 0.02015, // A-G
    0.06094, 0.06966, 0.00153, 0.00772, 0.04025, 0.02406, 0.06749, // H-N
    0.07507, 0.01929, 0.00095, 0.05987, 0.06327, 0.09056, 0.02758, // O-U
    0.00978, 0.02360, 0.00150, 0.01974, 0.00074, 0.19181, // V-Z & space char
];

pub fn hex_to_bytes(hex: &str) -> Vec<u8>{
    hex::decode(hex).unwrap()
}

pub fn bytes_to_hex(bytes: &[u8]) -> String{
    hex::encode(bytes)
}

pub fn bytes_to_b64(bytes: &[u8]) -> String{
    general_purpose::STANDARD.encode(bytes)
}

pub fn b64_to_bytes(b64: &str) -> Vec<u8>{
    general_purpose::STANDARD.decode(b64).unwrap()
}

pub fn bytes_to_plaintext(bytes: &Vec<u8>) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

pub fn open_file_to_string(path: &str) -> String {
    fs::read_to_string(path).unwrap().split('\n').collect::<Vec<_>>().join("")
}

pub fn fixed_xor(bytes_1: &Vec<u8>, bytes_2: &Vec<u8>) -> Vec<u8> {
    if bytes_1.len() != bytes_2.len() {
        panic!("Lengths are not equal");
    }

    bytes_1
        .iter()
        .zip(bytes_2.iter())
        .map(|(&b1, &b2)| b1 ^ b2)
        .collect::<Vec<u8>>()
}

pub fn count_freq_score(plaintext: &str) -> f64 {
    // better score => closer to english
    let mut counts = vec![0_u32; 27];
    let mut score = 0_f64;

    plaintext.chars().for_each(|c| match c {
        'a'..='z' => {
            counts[c as usize - 'a' as usize] += 1;
        }
        'A'..='Z' => {
            counts[c as usize - 'A' as usize] += 1;
        }
        ' ' => counts[26] += 1,
        _ => {}
    });

    for i in 0..27 {
        score += (counts[i] as f64) * LETTER_FREQ[i];
    }
    score
}

pub fn break_single_char_xor(bytes: &Vec<u8>) -> (f64, u8, String) {
    //(score, key, plaintext)
    let mut best_candidate: (f64, u8, String) = (0_f64, 0_u8, "".to_owned());

    for temp_key in 0..=255 {
        let plaintext_bytes: Vec<u8> = bytes.iter().map(|&b| b ^ temp_key).collect();
        let plaintext = String::from_utf8_lossy(&plaintext_bytes);
        let temp_score = count_freq_score(&plaintext);

        if temp_score > best_candidate.0 {
            best_candidate.0 = temp_score;
            best_candidate.1 = temp_key;
            best_candidate.2 = plaintext.to_string();
        }
    }
    best_candidate
}

pub fn repeat_key(size: usize, key: &str) -> Vec<u8> {    
    key.chars()
        .cycle()
        .take(size)
        .collect::<String>()
        .into_bytes()
}

pub fn edit_distance(bytes_1: &Vec<u8>, bytes_2: &Vec<u8>) -> u32 {
    if bytes_1.len() != bytes_2.len() {
        panic!("Lengths are not equal");
    }

    fixed_xor(bytes_1, bytes_2)
        .iter()
        .map(|b| b.count_ones())
        .sum()
}

pub fn guess_keysize(data: &[u8]) -> usize {
    //returns the keysize with the smallest normalized distance
    let mut distances = Vec::new();
    
    for keysize in 2..=40 {
        let blocks: Vec<&[u8]> = data.chunks(keysize).take(4).collect();
        let mut total_distance = 0;
        let mut pairs = 0;
        for i in 0..blocks.len() {
            for j in i+1..blocks.len() {
                total_distance += edit_distance(&blocks[i].to_vec(), &blocks[j].to_vec());
                pairs +=1;
            }
        }
        let normalized_distance = total_distance as f64 / (pairs * keysize) as f64;
        distances.push((keysize, normalized_distance));
    }
    distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    distances[0].0
}

pub fn transpose_blocks(blocks: Vec<&[u8]>) -> Vec<Vec<u8>> {
    let mut transposed: Vec<Vec<u8>> = vec![Vec::new(); blocks[0].len()];

    for block in blocks {
        for (i, &byte) in block.iter().enumerate() {
            transposed[i].push(byte);
        }
    }
    transposed
}

pub fn break_repeating_key_xor(keysize: usize, ciphertext_bytes: Vec<u8>) -> (Vec<u8>, String){

    let cipher_blocks: Vec<&[u8]> = ciphertext_bytes.chunks(keysize).collect();
    let transposed_blocks = transpose_blocks(cipher_blocks);
    let mut key_bytes = Vec::new();

    for block in transposed_blocks {
        let (_score, key_byte, _plaintext) = break_single_char_xor(&block);
        key_bytes.push(key_byte);
    }

    let repeated_key = repeat_key(ciphertext_bytes.len(), &bytes_to_plaintext(&key_bytes).as_str());
    let plaintext = bytes_to_plaintext(&fixed_xor(&ciphertext_bytes, &repeated_key));

    (key_bytes, plaintext)
}

pub fn decrypt_aes_ecb_128(key_bytes: &[u8],ciphertext_bytes: &[u8]) -> Result<Vec<u8>, ErrorStack> {
    decrypt(Cipher::aes_128_ecb(), key_bytes, None, &ciphertext_bytes)
}

pub fn detect_aes_ecb(ciphertext_bytes: &[u8]) -> usize {
    let block_size = 16;
    let blocks = ciphertext_bytes.chunks(block_size);
    let unique_blocks: HashSet<&[u8]> = HashSet::from_iter(blocks.clone());
    blocks.len() - unique_blocks.len()
}

pub fn pkcs7_padding(block_size: u8, input_text: &[u8]) -> Vec<u8> {
    let padding_size = block_size - (input_text.len() % block_size as usize) as u8;
    let pad = vec![padding_size; padding_size as usize];
    [input_text, &pad].concat()
}


#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_edit_distance(){
        let test_s1 = "this is a test";
        let test_s2 = "wokka wokka!!!";
        let result = edit_distance(&test_s1.as_bytes().to_vec(), &test_s2.as_bytes().to_vec());
        // println!(" Test String 1 = {:#?}\n Test String 2 = {}\n Expected Hamming Distance = 37\n Actual Hamming Distance = {:#?}\n", test_s1, test_s2, result);
        assert_eq!(37, result);
    }

    #[test]
    fn test_detect_aes_ecb(){
        let ciphertext = "d880619740a8a19b7840a8a31c810a3d08649af70dc06f4fd5d2d69c744cd283e2dd052f6b641dbf9d11b0348542bb5708649af70dc06f4fd5d2d69c744cd2839475c9dfdbc1d46597949d9c7e82bf5a08649af70dc06f4fd5d2d69c744cd28397a93eab8d6aecd566489154789a6b0308649af70dc06f4fd5d2d69c744cd283d403180c98c8f6db1f2a3f9c4040deb0ab51b29933f2c123c58386b06fba186a";
        let expected_blocks = 3;
        let repeated_blocks = detect_aes_ecb(&hex_to_bytes(&ciphertext));
        assert_eq!(repeated_blocks, expected_blocks);
    }

    #[test]
    fn test_pkcs7_padding() {
        let test_size_1 = 16;
        let test_size_2 =32;
        let test_1 = "YELLOW SUB";
        let test_2 = "YELLOW SUBMARINE";

        let expected_output_1 = "YELLOW SUB\x06\x06\x06\x06\x06\x06";
        let expected_output_2 = "YELLOW SUBMARINE\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10\x10";
        assert_eq!(expected_output_1, bytes_to_plaintext(&pkcs7_padding(test_size_1, test_1.as_bytes())));
        assert_eq!(expected_output_2, bytes_to_plaintext(&pkcs7_padding(test_size_2, test_2.as_bytes())));
    }
}