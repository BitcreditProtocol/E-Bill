use super::behaviour::{Command, Event, FileResponse};
use crate::blockchain::{Chain, GossipsubEvent, GossipsubEventId};
use crate::constants::{
    BILLS_FOLDER_PATH, BILLS_KEYS_FOLDER_PATH, BILLS_PREFIX, IDENTITY_FILE_PATH,
};
use crate::{
    bill::{
        contacts::IdentityPublicData,
        get_bills,
        identity::{get_whole_identity, read_peer_id_from_file, IdentityWithAll},
    },
    util::{
        is_not_hidden,
        rsa::{decrypt_bytes_with_private_key, encrypt_bytes_with_public_key},
    },
};
use futures::channel::mpsc::Receiver;
use futures::channel::{mpsc, oneshot};
use futures::prelude::*;
use libp2p::kad::record::Record;
use libp2p::request_response::ResponseChannel;
use libp2p::PeerId;
use std::collections::HashSet;
use std::error::Error;
use std::fs;
use std::path::Path;
use tokio_stream::wrappers::LinesStream;

#[derive(Clone)]
pub struct Client {
    pub(super) sender: mpsc::Sender<Command>,
}

impl Client {
    pub async fn run(
        mut self,
        mut stdin: stream::Fuse<LinesStream<tokio::io::BufReader<tokio::io::Stdin>>>,
        mut network_events: Receiver<Event>,
    ) {
        loop {
            futures::select! {
                line = stdin.select_next_some() => self.handle_input_line(line.expect("Stdin not to close.")).await,
                event = network_events.next() => self.handle_event(event.expect("Swarm stream to be infinite.")).await,
            }
        }
    }

    pub async fn check_new_bills(&mut self, node_id: String) {
        let node_request = BILLS_PREFIX.to_string() + &node_id;
        let list_bills_for_node = self.get_record(node_request.clone()).await;
        let value = list_bills_for_node.value;

        if !value.is_empty() {
            let record_for_saving_in_dht = std::str::from_utf8(&value)
                .expect("Cant get value.")
                .to_string();
            let bills = record_for_saving_in_dht.split(',');
            for bill_id in bills {
                if !Path::new((BILLS_FOLDER_PATH.to_string() + "/" + bill_id + ".json").as_str())
                    .exists()
                {
                    let bill_bytes = self.get_bill(bill_id.to_string().clone()).await;
                    if !bill_bytes.is_empty() {
                        let path = BILLS_FOLDER_PATH.to_string() + "/" + bill_id + ".json";
                        fs::write(path, bill_bytes.clone()).expect("Can't write file.");
                    }

                    let key_bytes = self.get_key(bill_id.to_string().clone()).await;
                    if !key_bytes.is_empty() {
                        let pr_key = get_whole_identity().identity.private_key_pem;

                        let key_bytes_decrypted =
                            decrypt_bytes_with_private_key(&key_bytes, pr_key);

                        let path = BILLS_KEYS_FOLDER_PATH.to_string() + "/" + bill_id + ".json";
                        fs::write(path, key_bytes_decrypted).expect("Can't write file.");
                    }

                    if !bill_bytes.is_empty() {
                        self.sender
                            .send(Command::SubscribeToTopic {
                                topic: bill_id.to_string().clone(),
                            })
                            .await
                            .expect("Command receiver not to be dropped.");
                    }
                }
            }
        }
    }

    //TODO: change
    //
    // pub async fn upgrade_table_for_other_node(&mut self, node_id: String, bill: String) {
    //     let node_request = BILLS_PREFIX.to_string() + &node_id;
    //     let list_bills_for_node = self.get_record(node_request.clone()).await;
    //     let value = list_bills_for_node.value;
    //
    //     if !value.is_empty() {
    //         let record_in_dht = std::str::from_utf8(&value)
    //             .expect("Cant get value.")
    //             .to_string();
    //         let mut new_record: String = record_in_dht.clone();
    //
    //         if !record_in_dht.contains(&bill) {
    //             new_record += (",".to_string() + &bill).as_str();
    //         }
    //
    //         if !record_in_dht.eq(&new_record) {
    //             self.put_record(node_request.clone(), new_record).await;
    //         }
    //     } else {
    //         let mut new_record: String = bill.clone();
    //
    //         if !new_record.is_empty() {
    //             self.put_record(node_request.clone(), new_record).await;
    //         }
    //     }
    // }

    pub async fn upgrade_table(&mut self, node_id: String) {
        let node_request = BILLS_PREFIX.to_string() + &node_id;
        let list_bills_for_node = self.get_record(node_request.clone()).await;
        let value = list_bills_for_node.value;

        if !value.is_empty() {
            let record_in_dht = std::str::from_utf8(&value)
                .expect("Cant get value.")
                .to_string();
            let mut new_record: String = record_in_dht.clone();

            for file in fs::read_dir(BILLS_FOLDER_PATH).unwrap() {
                let dir = file.unwrap();
                if is_not_hidden(&dir) {
                    let mut bill_name = dir.file_name().into_string().unwrap();

                    bill_name = Path::file_stem(Path::new(&bill_name))
                        .expect("File name error")
                        .to_str()
                        .expect("File name error")
                        .to_string();

                    if !record_in_dht.contains(&bill_name) {
                        new_record += (",".to_string() + &bill_name.clone()).as_str();
                        self.put(&bill_name).await;
                    }
                }
            }
            if !record_in_dht.eq(&new_record) {
                self.put_record(node_request.clone(), new_record).await;
            }
        } else {
            let mut new_record = String::new();
            for file in fs::read_dir(BILLS_FOLDER_PATH).unwrap() {
                let dir = file.unwrap();
                if is_not_hidden(&dir) {
                    let mut bill_name = dir.file_name().into_string().unwrap();
                    bill_name = Path::file_stem(Path::new(&bill_name))
                        .expect("File name error")
                        .to_str()
                        .expect("File name error")
                        .to_string();

                    if new_record.is_empty() {
                        new_record = bill_name.clone();
                        self.put(&bill_name).await;
                    } else {
                        new_record += (",".to_string() + &bill_name.clone()).as_str();
                        self.put(&bill_name).await;
                    }
                }
            }
            if !new_record.is_empty() {
                self.put_record(node_request.clone(), new_record).await;
            }
        }
    }

    pub async fn start_provide(&mut self) {
        for file in fs::read_dir(BILLS_FOLDER_PATH).unwrap() {
            let dir = file.unwrap();
            if is_not_hidden(&dir) {
                let mut bill_name = dir.file_name().into_string().unwrap();
                bill_name = Path::file_stem(Path::new(&bill_name))
                    .expect("File name error")
                    .to_str()
                    .expect("File name error")
                    .to_string();
                self.put(&bill_name).await;
            }
        }
    }

    pub async fn put_identity_public_data_in_dht(&mut self) {
        if Path::new(IDENTITY_FILE_PATH).exists() {
            let identity: IdentityWithAll = get_whole_identity();
            let identity_data = IdentityPublicData::new(
                identity.identity.clone(),
                identity.peer_id.to_string().clone(),
            );

            let key = "INFO".to_string() + &identity_data.peer_id;
            let current_info = self.get_record(key.clone()).await.value;
            let mut current_info_string = String::new();
            if !current_info.is_empty() {
                current_info_string = std::str::from_utf8(&current_info)
                    .expect("Cant get value.")
                    .to_string();
            }
            let value = serde_json::to_string(&identity_data).unwrap();
            if !current_info_string.eq(&value) {
                self.put_record(key, value).await;
            }
        }
    }

    pub async fn get_identity_public_data_from_dht(
        &mut self,
        peer_id: String,
    ) -> IdentityPublicData {
        let key = "INFO".to_string() + &peer_id;
        let current_info = self.get_record(key.clone()).await.value;
        let mut identity_public_data: IdentityPublicData = IdentityPublicData::new_empty();
        if !current_info.is_empty() {
            let current_info_string = std::str::from_utf8(&current_info)
                .expect("Cant get value.")
                .to_string();
            identity_public_data = serde_json::from_str(&current_info_string).unwrap();
        }

        identity_public_data
    }

    pub async fn add_bill_to_dht_for_node(&mut self, bill_name: &String, node_id: &String) {
        let node_request = BILLS_PREFIX.to_string() + node_id;
        let mut record_for_saving_in_dht;
        let list_bills_for_node = self.get_record(node_request.clone()).await;
        let value = list_bills_for_node.value;
        if !value.is_empty() {
            record_for_saving_in_dht = std::str::from_utf8(&value)
                .expect("Cant get value.")
                .to_string();
            if !record_for_saving_in_dht.contains(bill_name) {
                record_for_saving_in_dht = record_for_saving_in_dht.to_string() + "," + bill_name;
            }
        } else {
            record_for_saving_in_dht = bill_name.clone();
        }

        if !std::str::from_utf8(&value)
            .expect("Cant get value.")
            .to_string()
            .eq(&record_for_saving_in_dht)
        {
            self.put_record(node_request.clone(), record_for_saving_in_dht.to_string())
                .await;
        }
    }

    pub async fn add_message_to_topic(&mut self, msg: Vec<u8>, topic: String) {
        self.send_message(msg, topic).await;
    }

    pub async fn put(&mut self, name: &str) {
        self.start_providing(name.to_owned()).await;
    }

    pub async fn get_bill(&mut self, name: String) -> Vec<u8> {
        let providers = self.get_providers(name.clone()).await;
        if providers.is_empty() {
            eprintln!("No providers was found.");
            Vec::new()
        } else {
            //TODO: If it's me - don't continue.
            let requests = providers.into_iter().map(|peer| {
                let mut network_client = self.clone();
                let local_peer_id = read_peer_id_from_file().to_string();
                let mut name = name.clone();
                name = "BILL_".to_string() + name.as_str();
                name = local_peer_id + "_" + name.as_str();
                async move { network_client.request_file(peer, name).await }.boxed()
            });

            let file_content = futures::future::select_ok(requests);

            let file_content_await = file_content.await;

            if file_content_await.is_err() {
                println!("None of the providers returned file.");
                Vec::new()
            } else {
                file_content_await
                    .map_err(|_| "None of the providers returned file.")
                    .expect("Can not get file content.")
                    .0
            }
        }
    }

    pub async fn get_key(&mut self, name: String) -> Vec<u8> {
        let providers = self.get_providers(name.clone()).await;
        if providers.is_empty() {
            eprintln!("No providers was found.");
            Vec::new()
        } else {
            //TODO: If it's me - don't continue.
            let requests = providers.into_iter().map(|peer| {
                let mut network_client = self.clone();
                let local_peer_id = read_peer_id_from_file().to_string();
                let mut name = name.clone();
                name = "KEY_".to_string() + name.as_str();
                name = local_peer_id + "_" + name.as_str();
                async move { network_client.request_file(peer, name).await }.boxed()
            });

            let file_content = futures::future::select_ok(requests);

            let file_content_await = file_content.await;

            if file_content_await.is_err() {
                println!("None of the providers returned file.");
                Vec::new()
            } else {
                file_content_await
                    .map_err(|_| "None of the providers returned file.")
                    .expect("Can not get file content.")
                    .0
            }
        }
    }

    pub async fn put_bills_for_parties(&mut self) {
        let bills = get_bills();

        for bill in bills {
            let chain = Chain::read_chain_from_file(&bill.name);
            let nodes = chain.get_all_nodes_from_bill();
            for node in nodes {
                self.add_bill_to_dht_for_node(&bill.name, &node).await;
            }
        }
    }

    pub async fn subscribe_to_all_bills_topics(&mut self) {
        let bills = get_bills();

        for bill in bills {
            self.subscribe_to_topic(bill.name).await;
        }
    }

    pub async fn receive_updates_for_all_bills_topics(&mut self) {
        let bills = get_bills();

        for bill in bills {
            let event = GossipsubEvent::new(GossipsubEventId::CommandGetChain, vec![0; 24]);
            let message = event.to_byte_array();

            self.add_message_to_topic(message, bill.name).await;
        }
    }

    pub async fn subscribe_to_topic(&mut self, topic: String) {
        self.sender
            .send(Command::SubscribeToTopic { topic })
            .await
            .expect("Command receiver not to be dropped.");
    }

    async fn send_message(&mut self, msg: Vec<u8>, topic: String) {
        self.sender
            .send(Command::SendMessage { msg, topic })
            .await
            .expect("Command receiver not to be dropped.");
    }

    async fn put_record(&mut self, key: String, value: String) {
        self.sender
            .send(Command::PutRecord { key, value })
            .await
            .expect("Command receiver not to be dropped.");
    }

    async fn get_record(&mut self, key: String) -> Record {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::GetRecord { key, sender })
            .await
            .expect("Command receiver not to be dropped.");
        receiver.await.expect("Sender not to be dropped.")
    }

    async fn start_providing(&mut self, file_name: String) {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::StartProviding { file_name, sender })
            .await
            .expect("Command receiver not to be dropped.");
        receiver.await.expect("Sender not to be dropped.");
    }

    async fn get_providers(&mut self, file_name: String) -> HashSet<PeerId> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::GetProviders { file_name, sender })
            .await
            .expect("Command receiver not to be dropped.");
        receiver.await.expect("Sender not to be dropped.")
    }

    async fn request_file(
        &mut self,
        peer: PeerId,
        file_name: String,
    ) -> Result<Vec<u8>, Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::RequestFile {
                file_name,
                peer,
                sender,
            })
            .await
            .expect("Command receiver not to be dropped.");
        receiver.await.expect("Sender not be dropped.")
    }

    async fn respond_file(&mut self, file: Vec<u8>, channel: ResponseChannel<FileResponse>) {
        self.sender
            .send(Command::RespondFile { file, channel })
            .await
            .expect("Command receiver not to be dropped.");
    }

    async fn handle_event(&mut self, event: Event) {
        let Event::InboundRequest { request, channel } = event;
        let size_request = request.split("_").collect::<Vec<&str>>();
        if size_request.len().eq(&3) {
            let request_node_id: String =
                request.splitn(2, "_").collect::<Vec<&str>>()[0].to_string();
            let request = request.splitn(2, "_").collect::<Vec<&str>>()[1].to_string();

            let mut bill_name = request.clone();
            if request.starts_with("KEY_") {
                bill_name = request.splitn(2, "KEY_").collect::<Vec<&str>>()[1].to_string();
            } else if request.starts_with("BILL_") {
                bill_name = request.split("BILL_").collect::<Vec<&str>>()[1].to_string();
            }
            let chain = Chain::read_chain_from_file(&bill_name);

            let bill_contain_node = chain.bill_contain_node(request_node_id.clone());

            if request.starts_with("KEY_") {
                if bill_contain_node {
                    let public_key = self
                        .get_identity_public_data_from_dht(request_node_id.clone())
                        .await
                        .rsa_public_key_pem;

                    let key_name = request.splitn(2, "KEY_").collect::<Vec<&str>>()[1].to_string();
                    let path_to_key =
                        BILLS_KEYS_FOLDER_PATH.to_string() + "/" + &key_name + ".json";
                    let file = fs::read(&path_to_key).unwrap();

                    let file_encrypted = encrypt_bytes_with_public_key(&file, public_key);

                    self.respond_file(file_encrypted, channel).await;
                }
            } else if request.starts_with("BILL_") {
                let bill_name = request.splitn(2, "BILL_").collect::<Vec<&str>>()[1].to_string();
                let path_to_bill = BILLS_FOLDER_PATH.to_string() + "/" + &bill_name + ".json";
                let file = fs::read(&path_to_bill).unwrap();

                self.respond_file(file, channel).await;
            }
        }
    }

    //Need for testing from console.
    async fn handle_input_line(&mut self, line: String) {
        let mut args = line.split(' ');

        match args.next() {
            Some("PUT") => {
                let name: String = {
                    match args.next() {
                        Some(name) => String::from(name),
                        None => {
                            eprintln!("Expected name.");
                            return;
                        }
                    }
                };
                self.put(&name).await;
            }

            Some("GET_BILL") => {
                let name: String = {
                    match args.next() {
                        Some(name) => String::from(name),
                        None => {
                            eprintln!("Expected name.");
                            return;
                        }
                    }
                };
                self.get_bill(name).await;
            }

            Some("GET_KEY") => {
                let name: String = {
                    match args.next() {
                        Some(name) => String::from(name),
                        None => {
                            eprintln!("Expected name.");
                            return;
                        }
                    }
                };
                self.get_key(name).await;
            }

            Some("PUT_RECORD") => {
                let key = {
                    match args.next() {
                        Some(key) => String::from(key),
                        None => {
                            eprintln!("Expected key");
                            return;
                        }
                    }
                };
                let value = {
                    match args.next() {
                        Some(value) => String::from(value),
                        None => {
                            eprintln!("Expected value");
                            return;
                        }
                    }
                };

                self.put_record(key, value).await;
            }

            Some("SEND_MESSAGE") => {
                let topic = {
                    match args.next() {
                        Some(key) => String::from(key),
                        None => {
                            eprintln!("Expected topic");
                            return;
                        }
                    }
                };
                let msg = {
                    match args.next() {
                        Some(value) => String::from(value),
                        None => {
                            eprintln!("Expected msg");
                            return;
                        }
                    }
                };

                self.send_message(msg.into_bytes(), topic).await;
            }

            Some("SUBSCRIBE") => {
                let topic = {
                    match args.next() {
                        Some(key) => String::from(key),
                        None => {
                            eprintln!("Expected topic");
                            return;
                        }
                    }
                };

                self.subscribe_to_topic(topic).await;
            }

            Some("GET_RECORD") => {
                let key = {
                    match args.next() {
                        Some(key) => String::from(key),
                        None => {
                            eprintln!("Expected key");
                            return;
                        }
                    }
                };
                self.get_record(key).await;
            }

            Some("GET_PROVIDERS") => {
                let key = {
                    match args.next() {
                        Some(key) => String::from(key),
                        None => {
                            eprintln!("Expected key");
                            return;
                        }
                    }
                };
                self.get_providers(key).await;
            }

            _ => {
                eprintln!(
                        "expected GET, PUT, SEND_MESSAGE, SUBSCRIBE, GET_RECORD, PUT_RECORD or GET_PROVIDERS."
                    );
            }
        }
    }
}
