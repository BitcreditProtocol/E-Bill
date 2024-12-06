#![allow(clippy::needless_range_loop)]
use anyhow::{anyhow, Result};
use openssl::rsa::{Padding, Rsa};

pub fn create_rsa_key_pair() -> Result<(String, String)> {
    let rsa = Rsa::generate(2048).map_err(|e| anyhow!("Could not create rsa key: {e}"))?;
    let private_key: Vec<u8> = rsa.private_key_to_pem()?;
    let public_key: Vec<u8> = rsa.public_key_to_pem()?;
    Ok((
        String::from_utf8(private_key)
            .map_err(|e| anyhow!("Could not create string from private key: {e}"))?,
        String::from_utf8(public_key)
            .map_err(|e| anyhow!("Could not create string from private key: {e}"))?,
    ))
}

//-------------------------Bytes common-------------------------
pub fn encrypt_bytes_with_public_key(bytes: &[u8], public_key: &str) -> Vec<u8> {
    let public_key = Rsa::public_key_from_pem(public_key.as_bytes()).unwrap();

    let key_size: usize = (public_key.size() / 2) as usize; //128

    let mut whole_encrypted_buff: Vec<u8> = Vec::new();
    let mut temp_buff: Vec<u8> = vec![0; key_size];
    let mut temp_buff_encrypted: Vec<u8> = vec![0; public_key.size() as usize];

    let number_of_key_size_in_whole_bill: usize = bytes.len() / key_size;
    let remainder: usize = bytes.len() - key_size * number_of_key_size_in_whole_bill;

    for i in 0..number_of_key_size_in_whole_bill {
        for j in 0..key_size {
            let byte_number: usize = key_size * i + j;
            temp_buff[j] = bytes[byte_number];
        }

        let _encrypted_len: usize = public_key
            .public_encrypt(&temp_buff, &mut temp_buff_encrypted, Padding::PKCS1)
            .unwrap();

        whole_encrypted_buff.append(&mut temp_buff_encrypted);
        temp_buff = vec![0; key_size];
        temp_buff_encrypted = vec![0; public_key.size() as usize];
    }

    if remainder != 0 {
        temp_buff = vec![0; remainder];

        let position: usize = key_size * number_of_key_size_in_whole_bill;
        temp_buff[..(bytes.len() - position)].copy_from_slice(&bytes[position..]);

        let _encrypted_len: usize = public_key
            .public_encrypt(&temp_buff, &mut temp_buff_encrypted, Padding::PKCS1)
            .unwrap();

        whole_encrypted_buff.append(&mut temp_buff_encrypted);
        temp_buff.clear();
        temp_buff_encrypted.clear();
    }

    whole_encrypted_buff
}

pub fn decrypt_bytes_with_private_key(bytes: &[u8], private_key: &str) -> Vec<u8> {
    let private_key = Rsa::private_key_from_pem(private_key.as_bytes()).unwrap();

    let key_size: usize = private_key.size() as usize; //256

    let mut whole_decrypted_buff: Vec<u8> = Vec::new();
    let mut temp_buff: Vec<u8> = vec![0; private_key.size() as usize];
    let mut temp_buff_decrypted: Vec<u8> = vec![0; private_key.size() as usize];

    let number_of_key_size_in_whole_bill: usize = bytes.len() / key_size;

    for i in 0..number_of_key_size_in_whole_bill {
        for j in 0..key_size {
            let byte_number = key_size * i + j;
            temp_buff[j] = bytes[byte_number];
        }

        let decrypted_len: usize = private_key
            .private_decrypt(&temp_buff, &mut temp_buff_decrypted, Padding::PKCS1)
            .unwrap();

        whole_decrypted_buff.append(&mut temp_buff_decrypted[0..decrypted_len].to_vec());
        temp_buff = vec![0; private_key.size() as usize];
        temp_buff_decrypted = vec![0; private_key.size() as usize];
    }

    whole_decrypted_buff
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn end_to_end_encryption_example() {
        let (private_key, public_key) = create_rsa_key_pair().unwrap();
        let input = "Hello World";
        let encrypted = encrypt_bytes_with_public_key(input.as_bytes(), &public_key);
        let decrypted = decrypt_bytes_with_private_key(&encrypted, &private_key);
        assert_eq!(input, std::str::from_utf8(&decrypted).unwrap());
    }
}
