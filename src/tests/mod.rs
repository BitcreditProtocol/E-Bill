#[cfg(test)]
mod test {
    use crate::bill::identity::{
        byte_array_to_size_array_keypair, byte_array_to_size_array_peer_id, create_new_identity,
    };
    use crate::external::bitcoin::AddressInfo;
    use crate::util::numbers_to_words::encode;
    use crate::util::rsa::generation_rsa_key;
    use crate::util::structure_as_u8_slice;
    use bitcoin::secp256k1::Scalar;
    use libp2p::identity::Keypair;
    use libp2p::PeerId;
    use openssl::rsa::{Padding, Rsa};
    use std::path::Path;
    use std::{fs, mem};

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

    //TODO: change. Because we read from file

    // #[test]
    // fn signature_test() {
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
    //     let identity = read_identity_from_file();
    //
    //     // Create signer
    //     let private_key = private_key_from_pem_u8(&identity.private_key_pem.as_bytes().to_vec());
    //     let signer_key = PKey::from_rsa(private_key).unwrap();
    //     let mut signer: Signer = Signer::new(MessageDigest::sha256(), signer_key.as_ref()).unwrap();
    //     signer.update(bill_to_byte_array(&data).as_slice()).unwrap();
    //     let signature: Vec<u8> = signer.sign_to_vec().unwrap();
    //
    //     // Create verifier
    //     let public_key = public_key_from_pem_u8(&identity.public_key_pem.as_bytes().to_vec());
    //     let verifier_key = PKey::from_rsa(public_key).unwrap();
    //     let mut verifier = Verifier::new(MessageDigest::sha256(), verifier_key.as_ref()).unwrap();
    //
    //     verifier
    //         .update(bill_to_byte_array(&data).as_slice())
    //         .unwrap();
    //
    //     // Verify
    //     assert!(verifier.verify(signature.as_slice()).unwrap());
    // }

    // #[test]
    // fn test_new_bill_enc() {
    //     let public_data_drawee = IdentityPublicData {
    //         peer_id: "".to_string(),
    //         name: "bill.drawee_name".to_string(),
    //         bitcoin_public_key: "".to_string(),
    //         postal_address: "".to_string(),
    //         email: "".to_string(),
    //     };
    //
    //     let peer_id = read_peer_id_from_file().to_string();
    //
    //     let public_data_payee = IdentityPublicData {
    //         peer_id: peer_id,
    //         name: "bill.payee_name".to_string(),
    //         bitcoin_public_key: "".to_string(),
    //         postal_address: "".to_string(),
    //         email: "".to_string(),
    //     };
    //
    //     let drawer = get_whole_identity();
    //
    //     let bill = issue_new_bill(
    //         "bill.bill_jurisdiction".to_string(),
    //         "bill.place_of_drawing".to_string(),
    //         12,
    //         "bill.place_of_payment".to_string(),
    //         "bill.maturity_date".to_string(),
    //         drawer.clone(),
    //         "bill.language".to_string(),
    //         public_data_drawee,
    //         public_data_payee,
    //     );
    //
    //     let bill2 = read_bill_from_file(&bill.name);
    //
    //     assert_eq!(bill.bill_jurisdiction, bill2.bill_jurisdiction);
    // }

    // #[test]
    // fn test_new_bill_end() {
    //     let peer_id = read_peer_id_from_file().to_string();
    //
    //     let public_data_drawee = IdentityPublicData {
    //         peer_id,
    //         name: "bill.drawee_name".to_string(),
    //         bitcoin_public_key: "".to_string(),
    //         postal_address: "".to_string(),
    //         email: "".to_string(),
    //     };
    //
    //     endorse_bitcredit_bill(
    //         &"5f58c116fa86af48dc4442e7daa4cf062564415fad31a889b3ed7e02f76bcf8b".to_string(),
    //         public_data_drawee,
    //     );
    //
    //     let bill = read_bill_from_file(
    //         &"5f58c116fa86af48dc4442e7daa4cf062564415fad31a889b3ed7e02f76bcf8b".to_string(),
    //     );
    //
    //     assert_eq!(bill.bill_jurisdiction, "bill.bill_jurisdiction".to_string());
    // }

    #[test]
    fn test_bitcoin() {
        let s1 = bitcoin::secp256k1::Secp256k1::new();
        let private_key1 = bitcoin::PrivateKey::new(
            s1.generate_keypair(&mut bitcoin::secp256k1::rand::thread_rng())
                .0,
            bitcoin::Network::Testnet,
        );
        let public_key1 = private_key1.public_key(&s1);
        let _address1 = bitcoin::Address::p2pkh(public_key1, bitcoin::Network::Testnet);

        let s2 = bitcoin::secp256k1::Secp256k1::new();
        let private_key2 = bitcoin::PrivateKey::new(
            s2.generate_keypair(&mut bitcoin::secp256k1::rand::thread_rng())
                .0,
            bitcoin::Network::Testnet,
        );
        let public_key2 = private_key1.public_key(&s2);
        let _address2 = bitcoin::Address::p2pkh(public_key2, bitcoin::Network::Testnet);

        let private_key3 = private_key1
            .inner
            .add_tweak(&Scalar::from(private_key2.inner))
            .unwrap();
        let pr_key3 = bitcoin::PrivateKey::new(private_key3, bitcoin::Network::Testnet);
        let public_key3 = public_key1.inner.combine(&public_key2.inner).unwrap();
        let pub_key3 = bitcoin::PublicKey::new(public_key3);
        let address3 = bitcoin::Address::p2pkh(pub_key3, bitcoin::Network::Testnet);

        println!("private key: {}", pr_key3);
        println!("public key: {}", pub_key3);
        println!("address: {}", address3);
        println!("{}", address3.is_spend_standard());
    }

    // #[tokio::test]
    // async fn test_mint() {
    //     let dir = PathBuf::from("./data/wallet".to_string());
    //     fs::create_dir_all(dir.clone()).unwrap();
    //     let db_path = dir.join("wallet.db").to_str().unwrap().to_string();

    //     let localstore = SqliteLocalStore::with_path(db_path.clone())
    //         .await
    //         .expect("Cannot parse local store");

    //     let mint_url = Url::parse("http://127.0.0.1:3338").expect("Invalid url");

    //     let wallet: Wallet<_, CrossPlatformHttpClient> = Wallet::builder()
    //         .with_localstore(localstore)
    //         .build()
    //         .await
    //         .expect("Could not create wallet");

    //     let wallet_keysets = wallet
    //         .add_mint_keysets_by_id(
    //             &Url::parse("http://127.0.0.1:3338").unwrap(),
    //             "cr-sat".to_string(),
    //             "5ee3478d7e11534d332dffe67dfad8c6def74d2130d8af3e9035cd180d0f70f6".to_string(),
    //         )
    //         .await
    //         .unwrap();
    //     let wallet_keyset = wallet_keysets.first().unwrap();

    //     let balance = wallet.get_balance().await.unwrap();
    //     println!("Balance: {balance:?} sats");

    //     let result = wallet
    //         .mint_tokens(
    //             wallet_keyset,
    //             &PaymentMethod::Bitcredit,
    //             45.into(),
    //             "f4f35d70-813f-4310-9c10-fdd4953c0f9a".to_string(),
    //             CurrencyUnit::CrSat,
    //         )
    //         .await;

    //     let token = result
    //         .unwrap()
    //         .serialize(Option::from(CurrencyUnit::CrSat))
    //         .unwrap();
    //     println!("Token: {token:?}");

    //     let balance2 = wallet.get_balance().await.unwrap();
    //     println!("Balance2: {balance2:?} sats");

    //     assert_eq!(1, 2);
    // }

    //#[tokio::test]
    //async fn test_check_quote() {
    //    let dir = PathBuf::from("./data/wallet".to_string());
    //    fs::create_dir_all(dir.clone()).unwrap();
    //    let db_path = dir.join("wallet.db").to_str().unwrap().to_string();

    //    let localstore = SqliteLocalStore::with_path(db_path.clone())
    //        .await
    //        .expect("Cannot parse local store");

    //    let mint_url = Url::parse("http://127.0.0.1:3338").expect("Invalid url");

    //    let wallet: Wallet<_, CrossPlatformHttpClient> = Wallet::builder()
    //        .with_localstore(localstore)
    //        .build()
    //        .await
    //        .expect("Could not create wallet");

    //    let result = wallet
    //        .check_bitcredit_quote(
    //            &mint_url,
    //            "9d676f0425295dacb5724fb3f0488934f97aff8d044c7a2eb051275671f1a5de".to_string(),
    //            "12D3KooWRzpBaZnydS4eMA74yaKEoGZFP7WFRvC8yQR7HyGoWfAk".to_string(),
    //        )
    //        .await;

    //    //bad
    //    // let result = wallet
    //    //     .check_bitcredit_quote(&mint_url, "19d676f0425295dacb5724fb3f0488934f97aff8d044c7a2eb051275671f1a5de".to_string(), "112D3KooWRzpBaZnydS4eMA74yaKEoGZFP7WFRvC8yQR7HyGoWfAk".to_string())
    //    //     .await;

    //    println!("Quote: {result:?}");

    //    assert_eq!(1, 2);
    //}

    // #[tokio::test]
    // async fn test_send() {
    //     let dir = PathBuf::from("./data/wallet".to_string());
    //     fs::create_dir_all(dir.clone()).unwrap();
    //     let db_path = dir.join("wallet.db").to_str().unwrap().to_string();
    //
    //     let localstore = SqliteLocalStore::with_path(db_path.clone())
    //         .await
    //         .expect("Cannot parse local store");
    //
    //     let mint_url = Url::parse("http://127.0.0.1:3338").expect("Invalid url");
    //
    //     let wallet: Wallet<_, CrossPlatformHttpClient> = Wallet::builder()
    //         .with_localstore(localstore)
    //         .build()
    //         .await
    //         .expect("Could not create wallet");
    //
    //     let result = wallet.send_tokens(10).await.expect("Cannot send tokens");
    //     let payment_invoice: String = result.try_into().unwrap();
    //
    //     println!("Result:\n{payment_invoice}");
    //     println!(
    //         "\nNew balance: {:?} sats",
    //         wallet.get_balance().await.unwrap()
    //     );
    //
    //     assert_eq!("test".to_string(), payment_invoice);
    //     assert_ne!("test".to_string(), payment_invoice);
    // }

    // #[tokio::test]
    // async fn test_balance() {
    //     let dir = PathBuf::from("./data/wallet".to_string());
    //     fs::create_dir_all(dir.clone()).unwrap();
    //     let db_path = dir.join("wallet.db").to_str().unwrap().to_string();

    //     let localstore = SqliteLocalStore::with_path(db_path.clone())
    //         .await
    //         .expect("Cannot parse local store");

    //     let mint_url = Url::parse("http://127.0.0.1:3338").expect("Invalid url");

    //     let identity: Identity = read_identity_from_file();
    //     let bitcoin_key = identity.bitcoin_public_key.clone();

    //     let wallet: Wallet<_, CrossPlatformHttpClient> = Wallet::builder()
    //         .with_localstore(localstore)
    //         .build()
    //         .await
    //         .expect("Could not create wallet");

    //     let balance = wallet.get_balance().await.unwrap();
    //     println!("Balance: {balance:?} sats");

    //     assert_eq!(1, balance);
    //     assert_ne!(1, balance);
    // }

    #[tokio::test]
    async fn test_api() {
        let request_url = format!(
            "https://blockstream.info/testnet/api/address/{address}",
            address = "mzYHxNxTTGrrxnwSc1RvqTusK4EM88o6yj"
        );
        println!("{}", request_url);
        let response1 = reqwest::get(&request_url)
            .await
            .expect("Failed to send request")
            .text()
            .await
            .expect("Failed to read response");
        println!("{:?}", response1);
        let response: AddressInfo = reqwest::get(&request_url)
            .await
            .expect("Failed to send request")
            .json()
            .await
            .expect("Failed to read response");
        println!("{:?}", response);
    }

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
    fn structure_to_bytes() {
        let ed25519_keys = Keypair::generate_ed25519();
        let peer_id = PeerId::from(ed25519_keys.public());
        let _id = create_new_identity(
            "qwq".to_string(),
            "adsda".to_string(),
            "ewqe".to_string(),
            "qwewqe".to_string(),
            "qwewqe".to_string(),
            "qweeq".to_string(),
            "qwewqe".to_string(),
        );

        let bytes_ed25519_keys = unsafe { structure_as_u8_slice(&ed25519_keys) };
        let bytes_peer_id = unsafe { structure_as_u8_slice(&peer_id) };

        let bytes_ed25519_keys_sized = byte_array_to_size_array_keypair(bytes_ed25519_keys);
        let bytes_peer_id_sized = byte_array_to_size_array_peer_id(bytes_peer_id);

        if !Path::new("test").exists() {
            fs::create_dir("test").expect("Can't create folder.");
        }
        fs::write("test/keys", *bytes_ed25519_keys_sized).expect("Unable to write keys in file");
        fs::write("test/peer_id", *bytes_peer_id_sized).expect("Unable to write peer id in file");

        let data_key = fs::read("test/keys").expect("Unable to read file with keypair");
        let key_pair_bytes_sized = byte_array_to_size_array_keypair(data_key.as_slice());
        let _key_pair2: Keypair = unsafe { mem::transmute_copy(key_pair_bytes_sized) };

        let data_peer_id = fs::read("test/peer_id").expect("Unable to read file with peer_id");
        let peer_id_bytes_sized = byte_array_to_size_array_peer_id(data_peer_id.as_slice());
        let _peer_id2: PeerId = unsafe { mem::transmute_copy(peer_id_bytes_sized) };
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

    #[test]
    fn encrypt_and_decrypt_simple_data_with_keypair() {
        // Create data
        let data = "test";

        // Generate a keypair
        let rsa_key = generation_rsa_key();

        let public_key =
            Rsa::public_key_from_pem(rsa_key.public_key_to_pem().unwrap().as_slice()).unwrap();
        let _private_key =
            Rsa::private_key_from_pem(rsa_key.private_key_to_pem().unwrap().as_slice()).unwrap();

        // Encrypt with public key
        let mut buf: Vec<u8> = vec![0; rsa_key.size() as usize];
        let _ = public_key
            .public_encrypt(data.as_bytes(), &mut buf, Padding::PKCS1)
            .unwrap();

        let data_enc = buf;

        // Decrypt with private key
        let mut buf: Vec<u8> = vec![0; rsa_key.size() as usize];
        let _ = rsa_key
            .private_decrypt(&data_enc, &mut buf, Padding::PKCS1)
            .unwrap();
        assert!(String::from_utf8(buf).unwrap().starts_with(data));
    }

    #[test]
    fn encrypt_and_decrypt_simple_data_with_rsa_keypair() {
        // Create data
        let data = "test";

        // Generate a keypair
        let rsa_key = generation_rsa_key();

        let _p_key =
            Rsa::public_key_from_pem(rsa_key.public_key_to_pem().unwrap().as_slice()).unwrap();

        // Encrypt with public key
        let mut buf: Vec<u8> = vec![0; rsa_key.size() as usize];
        let _ = rsa_key
            .public_encrypt(data.as_bytes(), &mut buf, Padding::PKCS1)
            .unwrap();

        let data_enc = buf;

        // Decrypt with private key
        let mut buf: Vec<u8> = vec![0; rsa_key.size() as usize];
        let _ = rsa_key
            .private_decrypt(&data_enc, &mut buf, Padding::PKCS1)
            .unwrap();
        assert!(String::from_utf8(buf).unwrap().starts_with(data));
    }

    #[test]
    fn numbers_to_letters() {
        let result = encode(&123_324_324);
        assert_eq!("one hundred twenty-three million three hundred twenty-four thousand three hundred twenty-four".to_string(), result);
    }
}
