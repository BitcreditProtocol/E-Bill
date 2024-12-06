#[cfg(test)]
pub mod test {
    use libp2p::identity::Keypair;
    use libp2p::PeerId;

    use crate::service::bill_service::BillKeys;
    use std::fs;
    use std::path::Path;

    pub fn get_bill_keys() -> BillKeys {
        BillKeys {
            private_key_pem: TEST_PRIVATE_KEY.to_owned(),
            public_key_pem: TEST_PUB_KEY.to_owned(),
        }
    }

    pub const TEST_PUB_KEY: &str = r#"
-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAubhgUJO9PWBZK2CfqSJr
v3RlDeF3TWiXBocWmBJXzQe4F8qfbj8nTHYJ0Eh22uPVg/Meul/3WNitFMU93jTL
hnYsx5qxOTHpQ8PVh1+2WvkpIfvJYBVuvmFMtFliyPuJKrOSGJp3SP5EgXbhSI+0
BB9y/pF5E0fZbh7Nwlci1R4L+dmuW0raPxgSgQw+g3KeBc+DiFEvJJ/ZuoaukS0h
UwDwY/QdSYRDNHNNO1W4hFJJj1dqnwfs/OmK8yWOG1GjJpI4TYnn/UO6ZJkTkTbA
xWiIC5Q+ZwzlYEJMNIBTBz+KKTUr4BeJEdneznUb0yeBzcdCg5EHQlvv7plXsQju
DQIDAQAB
-----END PUBLIC KEY-----
"#;

    pub const TEST_PRIVATE_KEY: &str = r#"
-----BEGIN RSA PRIVATE KEY-----
MIIEpAIBAAKCAQEAubhgUJO9PWBZK2CfqSJrv3RlDeF3TWiXBocWmBJXzQe4F8qf
bj8nTHYJ0Eh22uPVg/Meul/3WNitFMU93jTLhnYsx5qxOTHpQ8PVh1+2WvkpIfvJ
YBVuvmFMtFliyPuJKrOSGJp3SP5EgXbhSI+0BB9y/pF5E0fZbh7Nwlci1R4L+dmu
W0raPxgSgQw+g3KeBc+DiFEvJJ/ZuoaukS0hUwDwY/QdSYRDNHNNO1W4hFJJj1dq
nwfs/OmK8yWOG1GjJpI4TYnn/UO6ZJkTkTbAxWiIC5Q+ZwzlYEJMNIBTBz+KKTUr
4BeJEdneznUb0yeBzcdCg5EHQlvv7plXsQjuDQIDAQABAoIBABgwJr8n1rxBKavo
HDNDi+P2DVlG9apLxmuvuWYZ8Xx/Fl9m4OfTatNfBj0tyukMRlk2l1hvuj/EjJpJ
bBreJmm/R2rBv3YzBW3xegR1F0N28v/9kockk3VRJ9PPVnnVpNI+a/cvWvzTPOnd
qU6xhKEK1YfJO4sizvM0KNk4Tw2RcE08o7tcDxQY6VO94dbaZ8ZJ2V+saiAa5BqN
cVZ+uZBmriJg+MVeB2PECqAGJWJ98r8I1Tq+2aRBc1+94E8Ilfi54qp4jpTghw3y
LH/uyf3BsbY4j08gk0Y7ljoXmaVyR7BZhcSOhc3XvMAzoqRzQpu/Fexk3Db6fuXz
wxvUW6ECgYEA/YTnhDbUS7r5ze7ntNQpuZZ3F9vU/FG2c+iC/MJ4Z2wb0gUJ8dXG
8Zbx7fQE3Hs44bW50tUaTvg7UsvyLMun6/OhdDgS+HGbMhJDNOBHQ3I1QKYUulbt
ZxRqt8dJRqOi5ctp0+zsFTko0lgA9BlIqG07oXNWzUS8Cf9DsBSaAn0CgYEAu4mg
oZok/ohv+//sb0T0UzDlxRSUf5a7Q2A0+a+hyMJm5QYHc+slLLsUdUapsx8tu71B
Y29J7+yfttH4R1NTP1cOPJj5edt+qknuQ0hMZKt+CS4ItxMM/bHV2z+Oi0U/LoW8
4jo2hh2oaHdXiDlXT9Eds7RK0vTrpcw5Q95fXtECgYAow7gecFqOmtAUJvgnAX58
Ew+vTG/g6pq15Is7bWHC74VBrgG9WyyUKDtakcQ+V6n70SbCGfYTAKM5WwXj4hNs
Q06Qy3txa4MS+BDKbc3HsJOTg6ENnXCrBINsbaUAsMs+vAiWRSBpATnpKLFujqo6
OuY9vbgVZZn+2Ybex1FEWQKBgQCAOqN9u9MtwwanDR+SGVjiBR4memLrNppGgGLY
kvGRPvNyB4RTC2Z4xlY/thhUpK31n3s1TSQGDApMzBbyVhQmzBSs9IAohR9/ultS
3/10HBpqlnJZE4qfcNhkOHnz2l5QJhu3p8weOesruuY7+9EqfzbK6Cz9P4Bc9l31
fPhC8QKBgQCO5FYksuRclILpzSVIJRj68NXZaLknDwAiNqb1a2diqGMCASXC5Z/U
jS4/cHdsAfssbxRGpoM5WNU7QPa/vhCVygcmAPPBD0DLT16JGpcnuAy3Ae4ss3Ih
HnZAVCxlGQ7ooHRIxJnp09ogDo7cDIevyMn1VmIZDm9JR1TUL6pbsg==
-----END RSA PRIVATE KEY-----
    "#;

    // fn bill_to_byte_array(bill: &BitcreditBill) -> Vec<u8> {
    //     to_vec(bill).unwrap()
    // }

    //TODO: Change. Because we create new bill every time we run tests

    // #[test]
    // fn blockchain() {
    //     //Identity
    //     let drawer = read_identity_from_file();
    //
    //     // New bill
    //     let bill = issue_new_bill(
    //         "bill.bill_jurisdiction".to_string(),
    //         "bill.place_of_drawing".to_string(),
    //         10,
    //         drawer.clone(),
    //         "bill.language".to_string(),
    //         "bill.drawee_name".to_string(),
    //     );
    //
    //     // Read blockchain from file
    //     let mut blockchain_from_file = Chain::read_chain_from_file(&bill.name);
    //
    //     //Take last block
    //     let last_block = blockchain_from_file.get_latest_block();
    //
    //     // Data for second block
    //     let data2 = "Ivan Tymko".to_string();
    //
    //     // Create second block
    //     let private_key = private_key_from_pem_u8(&drawer.private_key_pem.as_bytes().to_vec());
    //     let signer_key = PKey::from_rsa(private_key).unwrap();
    //     let signature: String = signature(&bill, &signer_key);
    //     let block_two = Block::new(
    //         last_block.id + 1,
    //         last_block.hash.clone(),
    //         hex::encode(data2.as_bytes()),
    //         bill.name.clone(),
    //         signature,
    //         "".to_string(),
    //         "".to_string(),
    //     );
    //
    //     // Validate and write chain
    //     blockchain_from_file.try_add_block(block_two);
    //     if blockchain_from_file.is_chain_valid() {
    //         blockchain_from_file.write_chain_to_file(&bill.name);
    //     }
    //
    //     // Try take last version of bill
    //     let chain_two = Chain::read_chain_from_file(&bill.name);
    //     let bill2 = chain_two.get_last_version_bill();
    //
    //     //Tests
    //     assert_eq!(bill.holder_name, "Mykyta Tymko".to_string());
    //     assert_eq!(bill2.holder_name, "Ivan Tymko".to_string());
    // }

    // #[test]
    // fn test_bitcoin() {
    //     let _ = env_logger::try_init();
    //     let s1 = bitcoin::secp256k1::Secp256k1::new();
    //     let private_key1 = bitcoin::PrivateKey::new(
    //         s1.generate_keypair(&mut bitcoin::secp256k1::rand::thread_rng())
    //             .0,
    //         bitcoin::Network::Testnet,
    //     );
    //     let public_key1 = private_key1.public_key(&s1);
    //     let _address1 = bitcoin::Address::p2pkh(public_key1, bitcoin::Network::Testnet);

    //     let s2 = bitcoin::secp256k1::Secp256k1::new();
    //     let private_key2 = bitcoin::PrivateKey::new(
    //         s2.generate_keypair(&mut bitcoin::secp256k1::rand::thread_rng())
    //             .0,
    //         bitcoin::Network::Testnet,
    //     );
    //     let public_key2 = private_key1.public_key(&s2);
    //     let _address2 = bitcoin::Address::p2pkh(public_key2, bitcoin::Network::Testnet);

    //     let private_key3 = private_key1
    //         .inner
    //         .add_tweak(&Scalar::from(private_key2.inner))
    //         .unwrap();
    //     let pr_key3 = bitcoin::PrivateKey::new(private_key3, bitcoin::Network::Testnet);
    //     let public_key3 = public_key1.inner.combine(&public_key2.inner).unwrap();
    //     let pub_key3 = bitcoin::PublicKey::new(public_key3);
    //     let address3 = bitcoin::Address::p2pkh(pub_key3, bitcoin::Network::Testnet);

    //     info!("private key: {}", pr_key3);
    //     info!("public key: {}", pub_key3);
    //     info!("address: {}", address3);
    //     info!("{}", address3.is_spend_standard());
    // }

    #[test]
    fn test_schnorr() {
        let secp1 = bitcoin::secp256k1::Secp256k1::new();
        let key_pair1 =
            bitcoin::secp256k1::Keypair::new(&secp1, &mut bitcoin::secp256k1::rand::thread_rng());
        let xonly1 = bitcoin::secp256k1::XOnlyPublicKey::from_keypair(&key_pair1);

        let secp2 = bitcoin::secp256k1::Secp256k1::new();
        let key_pair2 =
            bitcoin::secp256k1::Keypair::new(&secp2, &mut bitcoin::secp256k1::rand::thread_rng());
        let _xonly2 = bitcoin::secp256k1::XOnlyPublicKey::from_keypair(&key_pair2);

        let msg = bitcoin::secp256k1::Message::from_digest_slice(&[0xab; 32]).unwrap();
        let a = secp1.sign_schnorr(&msg, &key_pair1);
        secp2
            .verify_schnorr(&a, &msg, &xonly1.0)
            .expect("verify failed");
    }

    #[test]
    fn peer_id_and_keypair_serialization_and_deserialization() {
        let ed25519_keys = Keypair::generate_ed25519();
        let peer_id = PeerId::from(ed25519_keys.public());

        let bytes_ed25519_keys = ed25519_keys.to_protobuf_encoding().unwrap();
        let bytes_peer_id = peer_id.to_bytes();

        if !Path::new("test").exists() {
            fs::create_dir("test").expect("Can't create folder.");
        }

        fs::write("test/keys", bytes_ed25519_keys).expect("Unable to write keys in file");
        fs::write("test/peer_id", bytes_peer_id).expect("Unable to write peer id in file");

        let data_key = fs::read("test/keys").expect("Unable to read file with keypair");
        let key_pair_deserialized = Keypair::from_protobuf_encoding(&data_key).unwrap();
        assert_eq!(ed25519_keys.public(), key_pair_deserialized.public());

        let data_peer_id = fs::read("test/peer_id").expect("Unable to read file with peer_id");
        let peer_id_deserialized = PeerId::from_bytes(&data_peer_id).unwrap();
        assert_eq!(peer_id, peer_id_deserialized);
    }

    // #[test]
    // fn encrypt_bill_with_rsa_keypair() {
    //     let bill = BitcreditBill {
    //         name: "".to_string(),
    //         to_payee: false,
    //         bill_jurisdiction: "".to_string(),
    //         timestamp_at_drawing: 0,
    //         drawee_name: "".to_string(),
    //         drawer_name: "".to_string(),
    //         holder_name: "".to_string(),
    //         place_of_drawing: "".to_string(),
    //         currency_code: "".to_string(),
    //         amount_numbers: 0,
    //         amounts_letters: "".to_string(),
    //         maturity_date: "".to_string(),
    //         date_of_issue: "".to_string(),
    //         compounding_interest_rate: 0,
    //         type_of_interest_calculation: false,
    //         place_of_payment: "".to_string(),
    //         public_key_pem: "".to_string(),
    //         private_key_pem: "".to_string(),
    //         language: "".to_string(),
    //     };
    //
    //     let rsa_key = generation_rsa_key();
    //     let bill_bytes = bill_to_byte_array(&bill);
    //
    //     let enc = encrypt_bytes(&bill_bytes, &rsa_key);
    //
    //     let mut final_number_256_byte_arrays: u32;
    //     let bill_bytes_len = bill_bytes.len();
    //     let exact_number_256_byte_arrays = (bill_bytes_len as f32 / 128 as f32) as f32;
    //     if exact_number_256_byte_arrays % 1.0 == 0 as f32 {
    //         final_number_256_byte_arrays = exact_number_256_byte_arrays as u32
    //     } else {
    //         final_number_256_byte_arrays = exact_number_256_byte_arrays as u32 + 1
    //     }
    //
    //     assert_eq!(final_number_256_byte_arrays * 256, enc.len() as u32);
    // }

    // #[test]
    // fn decrypt_bill_with_rsa_keypair() {
    //     let bill = BitcreditBill {
    //         name: "".to_string(),
    //         to_payee: false,
    //         bill_jurisdiction: "".to_string(),
    //         timestamp_at_drawing: 0,
    //         drawee_name: "".to_string(),
    //         drawer_name: "".to_string(),
    //         holder_name: "".to_string(),
    //         place_of_drawing: "".to_string(),
    //         currency_code: "".to_string(),
    //         amount_numbers: 0,
    //         amounts_letters: "".to_string(),
    //         maturity_date: "".to_string(),
    //         date_of_issue: "".to_string(),
    //         compounding_interest_rate: 0,
    //         type_of_interest_calculation: false,
    //         place_of_payment: "".to_string(),
    //         public_key_pem: "".to_string(),
    //         private_key_pem: "".to_string(),
    //         language: "".to_string(),
    //     };
    //
    //     let rsa_key = generation_rsa_key();
    //     let bill_bytes = bill_to_byte_array(&bill);
    //
    //     let encrypted_bill = encrypt_bytes(&bill_bytes, &rsa_key);
    //
    //     let decrypted_bill = decrypt_bytes(&encrypted_bill, &rsa_key);
    //     assert_eq!(bill_bytes.len(), decrypted_bill.len());
    //
    //     let new_bill = bill_from_byte_array(&decrypted_bill);
    //
    //     assert_eq!(bill.bill_jurisdiction, new_bill.bill_jurisdiction);
    // }

    // #[test]
    // fn sign_and_verify_data_given_an_rsa_keypair() {
    //     let data = BitcreditBill {
    //         name: "".to_string(),
    //         to_payee: false,
    //         bill_jurisdiction: "".to_string(),
    //         timestamp_at_drawing: 0,
    //         drawee_name: "".to_string(),
    //         drawer_name: "".to_string(),
    //         holder_name: "".to_string(),
    //         place_of_drawing: "".to_string(),
    //         currency_code: "".to_string(),
    //         amount_numbers: 0,
    //         amounts_letters: "".to_string(),
    //         maturity_date: "".to_string(),
    //         date_of_issue: "".to_string(),
    //         compounding_interest_rate: 0,
    //         type_of_interest_calculation: false,
    //         place_of_payment: "".to_string(),
    //         public_key_pem: "".to_string(),
    //         private_key_pem: "".to_string(),
    //         language: "".to_string(),
    //     };
    //
    //     // Generate a keypair
    //     let rsa_key = generation_rsa_key();
    //     let p_key = PKey::from_rsa(rsa_key).unwrap();
    //
    //     // Create signer
    //     let mut signer = Signer::new(MessageDigest::sha256(), p_key.as_ref()).unwrap();
    //
    //     // Sign
    //     signer.update(&*data.try_to_vec().unwrap()).unwrap();
    //     let signature = signer.sign_to_vec().unwrap();
    //
    //     // Create verifier
    //     let mut verifier = Verifier::new(MessageDigest::sha256(), p_key.as_ref()).unwrap();
    //
    //     // Verify
    //     verifier.update(&*data.try_to_vec().unwrap()).unwrap();
    //     assert!(verifier.verify(&signature).unwrap());
    // }
}
