use std::error::Error;

use futures::prelude::*;
use libp2p::core::Multiaddr;
use libp2p::multihash::Multihash;
use serde_derive::{Deserialize, Serialize};
use tokio::spawn;

use crate::{config::Config, dht::network::Client};

pub async fn dht_main(conf: &Config) -> Result<Client, Box<dyn Error + Send + Sync>> {
    let (network_client, network_events, network_event_loop) = network::new(conf)
        .await
        .expect("Can not to create network module in dht.");

    //Need for testing from console.
    let stdin = async_std::io::BufReader::new(async_std::io::stdin())
        .lines()
        .fuse();

    spawn(network_event_loop.run());

    let network_client_to_return = network_client.clone();

    spawn(network_client.run(stdin, network_events));

    Ok(network_client_to_return)
}

pub mod network {
    use std::collections::{HashMap, HashSet};
    use std::net::Ipv4Addr;
    use std::path::Path;
    use std::{fs, iter, path};

    use async_trait::async_trait;
    use futures::channel::mpsc::Receiver;
    use futures::channel::{mpsc, oneshot};
    use futures::executor::block_on;
    use libp2p::core::transport::OrTransport;
    use libp2p::core::upgrade::{
        read_length_prefixed, write_length_prefixed, ProtocolName, Version,
    };
    use libp2p::dns::DnsConfig;
    use libp2p::identity::Keypair;
    use libp2p::kad::record::store::MemoryStore;
    use libp2p::kad::record::{Key, Record};
    use libp2p::kad::{
        GetProvidersOk, GetRecordError, GetRecordOk, Kademlia, KademliaEvent, PeerRecord, QueryId,
        QueryResult, Quorum,
    };
    use libp2p::multiaddr::Protocol;
    use libp2p::request_response::{self, ProtocolSupport, RequestId, ResponseChannel};
    use libp2p::swarm::{
        ConnectionHandlerUpgrErr, NetworkBehaviour, Swarm, SwarmBuilder, SwarmEvent,
    };
    use libp2p::{dcutr, gossipsub, identify, kad, noise, relay, tcp, yamux, PeerId, Transport};

    use crate::blockchain::{Block, Chain, GossipsubEvent, GossipsubEventId};
    use crate::constants::{
        BILLS_FOLDER_PATH, BILLS_KEYS_FOLDER_PATH, BILLS_PREFIX, BOOTSTRAP_NODES_FILE_PATH,
        IDENTITY_ED_25529_KEYS_FILE_PATH, IDENTITY_FILE_PATH, IDENTITY_PEER_ID_FILE_PATH,
        RELAY_BOOTSTRAP_NODE_ONE_IP, RELAY_BOOTSTRAP_NODE_ONE_PEER_ID,
        RELAY_BOOTSTRAP_NODE_ONE_TCP,
    };
    use crate::service::contact_service::IdentityPublicData;
    use crate::{
        config, decrypt_bytes_with_private_key, encrypt_bytes_with_public_key, generate_dht_logic,
        get_bills, get_whole_identity, is_not_hidden, read_ed25519_keypair_from_file,
        read_peer_id_from_file, IdentityWithAll,
    };

    use super::*;

    pub async fn new(
        conf: &Config,
    ) -> Result<(Client, Receiver<Event>, EventLoop), Box<dyn Error>> {
        if !Path::new(IDENTITY_PEER_ID_FILE_PATH).exists()
            && !Path::new(IDENTITY_ED_25529_KEYS_FILE_PATH).exists()
        {
            generate_dht_logic();
        }

        let local_public_key = read_ed25519_keypair_from_file();
        let local_peer_id = read_peer_id_from_file();
        println!("Local peer id: {local_peer_id:?}");

        let (relay_transport, client) = relay::client::new(local_peer_id);

        let transport = OrTransport::new(
            relay_transport,
            block_on(DnsConfig::system(tcp::tokio::Transport::new(
                tcp::Config::default().port_reuse(true),
            )))
            .unwrap(),
        )
        .upgrade(Version::V1Lazy)
        .authenticate(noise::Config::new(&local_public_key).unwrap())
        .multiplex(yamux::Config::default())
        .timeout(std::time::Duration::from_secs(20))
        .boxed();

        let behaviour = MyBehaviour::new(local_peer_id, local_public_key.clone(), client);

        let mut swarm =
            SwarmBuilder::with_tokio_executor(transport, behaviour, local_peer_id).build();

        swarm.listen_on(conf.p2p_listen_url().unwrap()).unwrap();

        // Wait to listen on all interfaces.
        block_on(async {
            let mut delay = futures_timer::Delay::new(std::time::Duration::from_secs(1)).fuse();
            loop {
                futures::select! {
                    event = swarm.next() => {
                        match event.unwrap() {
                            SwarmEvent::NewListenAddr { address, .. } => {
                                println!("Listening on {:?}", address);
                            }
                            SwarmEvent::Behaviour { .. } => {
                            }
                            event => panic!("{event:?}"),
                        }
                    }
                    _ = delay => {
                        // Likely listening on all interfaces now, thus continuing by breaking the loop.
                        break;
                    }
                }
            }
        });

        let relay_peer_id: PeerId = RELAY_BOOTSTRAP_NODE_ONE_PEER_ID
            .to_string()
            .parse()
            .expect("Can not to parse relay peer id.");
        let relay_address = Multiaddr::empty()
            .with(Protocol::Ip4(RELAY_BOOTSTRAP_NODE_ONE_IP))
            .with(Protocol::Tcp(RELAY_BOOTSTRAP_NODE_ONE_TCP))
            .with(Protocol::P2p(Multihash::from(relay_peer_id)));
        println!("Relay address: {:?}", relay_address);

        swarm.dial(relay_address.clone()).unwrap();
        block_on(async {
            let mut learned_observed_addr = false;
            let mut told_relay_observed_addr = false;

            loop {
                match swarm.next().await.unwrap() {
                    SwarmEvent::NewListenAddr { .. } => {}
                    SwarmEvent::Dialing { .. } => {}
                    SwarmEvent::ConnectionEstablished { .. } => {}
                    SwarmEvent::Behaviour(ComposedEvent::Identify(identify::Event::Sent {
                        ..
                    })) => {
                        println!("Told relay its public address.");
                        told_relay_observed_addr = true;
                    }
                    SwarmEvent::Behaviour(ComposedEvent::Identify(identify::Event::Received {
                        info: identify::Info { observed_addr, .. },
                        ..
                    })) => {
                        println!("Relay told us our public address: {:?}", observed_addr);
                        learned_observed_addr = true;
                    }
                    SwarmEvent::Behaviour { .. } => {}
                    event => panic!("{event:?}"),
                }

                if learned_observed_addr && told_relay_observed_addr {
                    break;
                }
            }
        });

        swarm.behaviour_mut().bootstrap_kademlia();

        swarm
            .listen_on(relay_address.clone().with(Protocol::P2pCircuit))
            .unwrap();

        block_on(async {
            loop {
                match swarm.next().await.unwrap() {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        println!("Listening on {:?}", address);
                        break;
                    }
                    SwarmEvent::Behaviour(ComposedEvent::Relay(
                        relay::client::Event::ReservationReqAccepted { .. },
                    )) => {
                        println!("Relay accepted our reservation request.");
                    }
                    SwarmEvent::Behaviour(ComposedEvent::Relay(event)) => {
                        println!("{:?}", event)
                    }
                    SwarmEvent::Behaviour(ComposedEvent::Dcutr(event)) => {
                        println!("{:?}", event)
                    }
                    SwarmEvent::Behaviour(ComposedEvent::Identify(event)) => {
                        println!("{:?}", event)
                    }
                    SwarmEvent::ConnectionEstablished {
                        peer_id, endpoint, ..
                    } => {
                        println!("Established connection to {:?} via {:?}", peer_id, endpoint);
                    }
                    SwarmEvent::OutgoingConnectionError { peer_id, error } => {
                        println!("Outgoing connection error to {:?}: {:?}", peer_id, error);
                    }
                    SwarmEvent::Behaviour(event) => {
                        println!("{event:?}")
                    }
                    _ => {}
                }
            }
        });

        let (command_sender, command_receiver) = mpsc::channel(0);
        let (event_sender, event_receiver) = mpsc::channel(0);
        let event_loop = EventLoop::new(swarm, command_receiver, event_sender);

        Ok((
            Client {
                sender: command_sender,
            },
            event_receiver,
            event_loop,
        ))
    }

    #[derive(Deserialize, Serialize, Debug)]
    struct Nodes {
        node: String,
        address: String,
    }

    #[derive(Deserialize, Serialize, Debug)]
    struct NodesJson {
        nodes: Vec<Nodes>,
    }

    #[derive(Clone)]
    pub struct Client {
        sender: mpsc::Sender<Command>,
    }

    impl Client {
        pub async fn run(
            mut self,
            mut stdin: stream::Fuse<
                futures::io::Lines<async_std::io::BufReader<async_std::io::Stdin>>,
            >,
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
                    if !Path::new(
                        (BILLS_FOLDER_PATH.to_string() + "/" + bill_id + ".json").as_str(),
                    )
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
            let mut record_for_saving_in_dht = String::new();
            let list_bills_for_node = self.get_record(node_request.clone()).await;
            let value = list_bills_for_node.value;
            if !value.is_empty() {
                record_for_saving_in_dht = std::str::from_utf8(&value)
                    .expect("Cant get value.")
                    .to_string();
                if !record_for_saving_in_dht.contains(bill_name) {
                    record_for_saving_in_dht =
                        record_for_saving_in_dht.to_string() + "," + bill_name;
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
            if let Event::InboundRequest { request, channel } = event {
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

                            let key_name =
                                request.splitn(2, "KEY_").collect::<Vec<&str>>()[1].to_string();
                            let path_to_key =
                                BILLS_KEYS_FOLDER_PATH.to_string() + "/" + &key_name + ".json";
                            let file = fs::read(&path_to_key).unwrap();

                            let file_encrypted = encrypt_bytes_with_public_key(&file, public_key);

                            self.respond_file(file_encrypted, channel).await;
                        }
                    } else if request.starts_with("BILL_") {
                        let bill_name =
                            request.splitn(2, "BILL_").collect::<Vec<&str>>()[1].to_string();
                        let path_to_bill =
                            BILLS_FOLDER_PATH.to_string() + "/" + &bill_name + ".json";
                        let file = fs::read(&path_to_bill).unwrap();

                        self.respond_file(file, channel).await;
                    }
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

    pub struct EventLoop {
        swarm: Swarm<MyBehaviour>,
        command_receiver: Receiver<Command>,
        event_sender: mpsc::Sender<Event>,
        pending_dial: HashMap<PeerId, oneshot::Sender<Result<(), Box<dyn Error + Send>>>>,
        pending_start_providing: HashMap<QueryId, oneshot::Sender<()>>,
        pending_get_providers: HashMap<QueryId, oneshot::Sender<HashSet<PeerId>>>,
        pending_get_records: HashMap<QueryId, oneshot::Sender<Record>>,
        pending_request_file:
            HashMap<RequestId, oneshot::Sender<Result<Vec<u8>, Box<dyn Error + Send>>>>,
    }

    impl EventLoop {
        fn new(
            swarm: Swarm<MyBehaviour>,
            command_receiver: Receiver<Command>,
            event_sender: mpsc::Sender<Event>,
        ) -> Self {
            Self {
                swarm,
                command_receiver,
                event_sender,
                pending_dial: Default::default(),
                pending_start_providing: Default::default(),
                pending_get_providers: Default::default(),
                pending_get_records: Default::default(),
                pending_request_file: Default::default(),
            }
        }

        pub async fn run(mut self) {
            loop {
                futures::select! {
                    event = self.swarm.next() => self.handle_event(event.expect("Swarm stream to be infinite.")).await,
                    command = self.command_receiver.next() => if let Some(c) = command { self.handle_command(c).await },
                }
            }
        }

        async fn handle_event(
            &mut self,
            event: SwarmEvent<
                ComposedEvent,
                //TODO change this (now it is bad)
                rocket::Either<
                    rocket::Either<
                        rocket::Either<
                            rocket::Either<
                                rocket::Either<
                                    ConnectionHandlerUpgrErr<std::io::Error>,
                                    std::io::Error,
                                >,
                                std::io::Error,
                            >,
                            void::Void,
                        >,
                        rocket::Either<
                            ConnectionHandlerUpgrErr<
                                rocket::Either<
                                    relay::inbound::stop::FatalUpgradeError,
                                    relay::outbound::hop::FatalUpgradeError,
                                >,
                            >,
                            void::Void,
                        >,
                    >,
                    rocket::Either<
                        ConnectionHandlerUpgrErr<
                            rocket::Either<
                                dcutr::inbound::UpgradeError,
                                dcutr::outbound::UpgradeError,
                            >,
                        >,
                        rocket::Either<ConnectionHandlerUpgrErr<std::io::Error>, void::Void>,
                    >,
                >,
            >,
        ) {
            match event {
                //--------------KADEMLIA EVENTS--------------
                SwarmEvent::Behaviour(ComposedEvent::Kademlia(
                    KademliaEvent::OutboundQueryProgressed { result, id, .. },
                )) => match result {
                    QueryResult::StartProviding(Ok(kad::AddProviderOk { key })) => {
                        let sender: oneshot::Sender<()> = self
                            .pending_start_providing
                            .remove(&id)
                            .expect("Completed query to be previously pending.");
                        let _ = sender.send(());
                    }

                    QueryResult::GetRecord(Ok(GetRecordOk::FoundRecord(PeerRecord {
                        record,
                        ..
                    }))) => {
                        if let Some(sender) = self.pending_get_records.remove(&id) {
                            println!(
                                "Got record {:?} {:?}",
                                std::str::from_utf8(record.key.as_ref()).unwrap(),
                                std::str::from_utf8(&record.value).unwrap(),
                            );

                            sender.send(record).expect("Receiver not to be dropped.");

                            // Finish the query. We are only interested in the first result.
                            //TODO: think how to do it better.
                            self.swarm
                                .behaviour_mut()
                                .kademlia
                                .query_mut(&id)
                                .unwrap()
                                .finish();
                        }
                    }

                    QueryResult::GetRecord(Ok(GetRecordOk::FinishedWithNoAdditionalRecord {
                        ..
                    })) => {
                        self.pending_get_records.remove(&id);
                        println!("No records.");
                    }

                    QueryResult::GetRecord(Err(GetRecordError::NotFound { key, .. })) => {
                        //TODO: its bad.
                        let record = Record {
                            key,
                            value: vec![],
                            publisher: None,
                            expires: None,
                        };
                        let _ = self
                            .pending_get_records
                            .remove(&id)
                            .expect("Request to still be pending.")
                            .send(record);
                    }

                    QueryResult::GetRecord(Err(GetRecordError::Timeout { key })) => {
                        //TODO: its bad.
                        let record = Record {
                            key,
                            value: vec![],
                            publisher: None,
                            expires: None,
                        };
                        let _ = self
                            .pending_get_records
                            .remove(&id)
                            .expect("Request to still be pending.")
                            .send(record);
                    }

                    QueryResult::GetRecord(Err(GetRecordError::QuorumFailed { key, .. })) => {
                        //TODO: its bad.
                        let record = Record {
                            key,
                            value: vec![],
                            publisher: None,
                            expires: None,
                        };
                        let _ = self
                            .pending_get_records
                            .remove(&id)
                            .expect("Request to still be pending.")
                            .send(record);
                    }

                    QueryResult::GetProviders(Ok(GetProvidersOk::FoundProviders {
                        providers,
                        ..
                    })) => {
                        if let Some(sender) = self.pending_get_providers.remove(&id) {
                            for peer in &providers {
                                println!("PEER {peer:?}");
                            }

                            sender.send(providers).expect("Receiver not to be dropped.");

                            // Finish the query. We are only interested in the first result.
                            //TODO: think how to do it better.
                            self.swarm
                                .behaviour_mut()
                                .kademlia
                                .query_mut(&id)
                                .unwrap()
                                .finish();
                        }
                    }

                    _ => {}
                },

                //--------------REQUEST RESPONSE EVENTS--------------
                SwarmEvent::Behaviour(ComposedEvent::RequestResponse(
                    request_response::Event::OutboundFailure {
                        request_id, error, ..
                    },
                )) => {
                    let _ = self
                        .pending_request_file
                        .remove(&request_id)
                        .expect("Request to still be pending.")
                        .send(Err(Box::new(error)));
                }

                SwarmEvent::Behaviour(ComposedEvent::RequestResponse(
                    request_response::Event::Message { message, .. },
                )) => match message {
                    request_response::Message::Request {
                        request, channel, ..
                    } => {
                        self.event_sender
                            .send(Event::InboundRequest {
                                request: request.0,
                                channel,
                            })
                            .await
                            .expect("Event receiver not to be dropped.");
                    }

                    request_response::Message::Response {
                        request_id,
                        response,
                    } => {
                        let _ = self
                            .pending_request_file
                            .remove(&request_id)
                            .expect("Request to still be pending.")
                            .send(Ok(response.0));
                    }
                },

                SwarmEvent::Behaviour(ComposedEvent::RequestResponse(
                    request_response::Event::ResponseSent { .. },
                )) => {
                    println!("{event:?}")
                }

                //--------------IDENTIFY EVENTS--------------
                SwarmEvent::Behaviour(ComposedEvent::Identify(event)) => {
                    println!("{:?}", event)
                }

                //--------------DCUTR EVENTS--------------
                SwarmEvent::Behaviour(ComposedEvent::Dcutr(event)) => {
                    println!("{:?}", event)
                }

                //--------------RELAY EVENTS--------------
                SwarmEvent::Behaviour(ComposedEvent::Relay(
                    relay::client::Event::ReservationReqAccepted { .. },
                )) => {
                    println!("{event:?}");
                    println!("Relay accepted our reservation request.");
                }

                SwarmEvent::Behaviour(ComposedEvent::Relay(event)) => {
                    println!("{:?}", event)
                }

                //--------------GOSSIPSUB EVENTS--------------
                SwarmEvent::Behaviour(ComposedEvent::Gossipsub(gossipsub::Event::Message {
                    propagation_source: peer_id,
                    message_id: id,
                    message,
                })) => {
                    let bill_name = message.topic.clone().into_string();
                    println!(
                        "Got message with id: {id} from peer: {peer_id} in topic: {bill_name}",
                    );
                    let event = GossipsubEvent::from_byte_array(&message.data);

                    if event.id.eq(&GossipsubEventId::Block) {
                        let block: Block =
                            serde_json::from_slice(&event.message).expect("Block are not valid.");
                        let mut chain: Chain = Chain::read_chain_from_file(&bill_name);
                        chain.try_add_block(block);
                        if chain.is_chain_valid() {
                            chain.write_chain_to_file(&bill_name);
                        }
                    } else if event.id.eq(&GossipsubEventId::Chain) {
                        let receive_chain: Chain =
                            serde_json::from_slice(&event.message).expect("Chain are not valid.");
                        let mut local_chain = Chain::read_chain_from_file(&bill_name);
                        local_chain.compare_chain(receive_chain, &bill_name);
                    } else if event.id.eq(&GossipsubEventId::CommandGetChain) {
                        let chain = Chain::read_chain_from_file(&bill_name);
                        let chain_bytes =
                            serde_json::to_vec(&chain).expect("Can not serialize chain.");
                        let event = GossipsubEvent::new(GossipsubEventId::Chain, chain_bytes);
                        let message = event.to_byte_array();
                        self.swarm
                            .behaviour_mut()
                            .gossipsub
                            .publish(gossipsub::IdentTopic::new(bill_name.clone()), message)
                            .expect("Can not publish message.");
                    } else {
                        println!(
                            "Unknown event id: {id} from peer: {peer_id} in topic: {bill_name}"
                        );
                    }
                }
                //--------------OTHERS BEHAVIOURS EVENTS--------------
                SwarmEvent::Behaviour(event) => {
                    println!("{event:?}")
                }

                //--------------COMMON EVENTS--------------
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Listening on {:?}", address);
                }

                SwarmEvent::IncomingConnection { .. } => {
                    println!("{event:?}")
                }

                SwarmEvent::ConnectionEstablished {
                    peer_id, endpoint, ..
                } => {
                    if endpoint.is_dialer() {
                        if let Some(sender) = self.pending_dial.remove(&peer_id) {
                            let _ = sender.send(Ok(()));
                        }
                    }
                }

                SwarmEvent::ConnectionClosed { .. } => {
                    println!("{event:?}")
                }

                SwarmEvent::OutgoingConnectionError { .. } => {
                    // println!("Outgoing connection error to {:?}: {:?}", peer_id, error);
                    // if let Some(peer_id) = peer_id {
                    //     if let Some(sender) = self.pending_dial.remove(&peer_id) {
                    //         let _ = sender.send(Err(Box::new(error)));
                    //     }
                    // }
                }

                SwarmEvent::IncomingConnectionError { .. } => {
                    println!("{event:?}")
                }

                _ => {}
            }
        }

        async fn handle_command(&mut self, command: Command) {
            match command {
                Command::StartProviding { file_name, sender } => {
                    println!("Start providing {file_name:?}");
                    let query_id = self
                        .swarm
                        .behaviour_mut()
                        .kademlia
                        .start_providing(file_name.into_bytes().into())
                        .expect("Can not provide.");
                    self.pending_start_providing.insert(query_id, sender);
                }

                Command::PutRecord { key, value } => {
                    println!("Put record {key:?}");
                    let key_record = Key::new(&key);
                    let value_bytes = value.as_bytes().to_vec();
                    let record = Record {
                        key: key_record,
                        value: value_bytes,
                        publisher: None,
                        expires: None,
                    };

                    let relay_peer_id: PeerId = RELAY_BOOTSTRAP_NODE_ONE_PEER_ID
                        .to_string()
                        .parse()
                        .expect("Can not to parse relay peer id.");

                    let _query_id = self
                        .swarm
                        .behaviour_mut()
                        .kademlia
                        //TODO: what quorum use?
                        .put_record_to(record, iter::once(relay_peer_id), Quorum::All);
                }

                Command::SendMessage { msg, topic } => {
                    println!("Send message to topic {topic:?}");
                    let swarm = self.swarm.behaviour_mut();
                    //TODO: check if topic not empty.
                    swarm
                        .gossipsub
                        .publish(gossipsub::IdentTopic::new(topic), msg)
                        .expect("Can not publish message.");
                }

                Command::SubscribeToTopic { topic } => {
                    println!("Subscribe to topic {topic:?}");
                    self.swarm
                        .behaviour_mut()
                        .gossipsub
                        .subscribe(&gossipsub::IdentTopic::new(topic))
                        .expect("TODO: panic message");
                }

                Command::GetRecord { key, sender } => {
                    println!("Get record {key:?}");
                    let key_record = Key::new(&key);
                    let query_id = self.swarm.behaviour_mut().kademlia.get_record(key_record);
                    self.pending_get_records.insert(query_id, sender);
                }

                Command::GetProviders { file_name, sender } => {
                    println!("Get providers {file_name:?}");
                    let query_id = self
                        .swarm
                        .behaviour_mut()
                        .kademlia
                        .get_providers(file_name.into_bytes().into());
                    self.pending_get_providers.insert(query_id, sender);
                }

                Command::RequestFile {
                    file_name,
                    peer,
                    sender,
                } => {
                    println!("Request file {file_name:?}");

                    let relay_peer_id: PeerId = RELAY_BOOTSTRAP_NODE_ONE_PEER_ID
                        .to_string()
                        .parse()
                        .expect("Can not to parse relay peer id.");
                    let relay_address = Multiaddr::empty()
                        .with(Protocol::Ip4(RELAY_BOOTSTRAP_NODE_ONE_IP))
                        .with(Protocol::Tcp(RELAY_BOOTSTRAP_NODE_ONE_TCP))
                        .with(Protocol::P2p(Multihash::from(relay_peer_id)))
                        .with(Protocol::P2pCircuit)
                        .with(Protocol::P2p(Multihash::from(peer)));

                    let swarm = self.swarm.behaviour_mut();
                    swarm.request_response.add_address(&peer, relay_address);
                    let request_id = swarm
                        .request_response
                        .send_request(&peer, FileRequest(file_name));
                    self.pending_request_file.insert(request_id, sender);
                }

                Command::RespondFile { file, channel } => {
                    println!("Respond file");
                    self.swarm
                        .behaviour_mut()
                        .request_response
                        .send_response(channel, FileResponse(file))
                        .expect("Connection to peer to be still open.");
                }
            }
        }
    }

    #[derive(NetworkBehaviour)]
    #[behaviour(out_event = "ComposedEvent", event_process = false)]
    struct MyBehaviour {
        request_response: request_response::Behaviour<FileExchangeCodec>,
        kademlia: Kademlia<MemoryStore>,
        identify: identify::Behaviour,
        gossipsub: gossipsub::Behaviour,
        relay_client: relay::client::Behaviour,
        dcutr: dcutr::Behaviour,
    }

    impl MyBehaviour {
        fn new(
            local_peer_id: PeerId,
            local_public_key: Keypair,
            client: relay::client::Behaviour,
        ) -> Self {
            Self {
                request_response: {
                    request_response::Behaviour::new(
                        FileExchangeCodec(),
                        iter::once((FileExchangeProtocol(), ProtocolSupport::Full)),
                        Default::default(),
                    )
                },
                kademlia: {
                    let store = MemoryStore::new(local_peer_id);
                    Kademlia::new(local_peer_id, store)
                },
                identify: {
                    let cfg_identify = identify::Config::new(
                        "/identify/0.1.0".to_string(),
                        local_public_key.public(),
                    );
                    identify::Behaviour::new(cfg_identify)
                },
                gossipsub: {
                    let gossipsub_config = libp2p::gossipsub::Config::default();
                    let message_authenticity =
                        gossipsub::MessageAuthenticity::Signed(local_public_key.clone());
                    gossipsub::Behaviour::new(message_authenticity, gossipsub_config)
                        .expect("Correct configuration")
                },
                relay_client: { client },
                dcutr: { dcutr::Behaviour::new(local_peer_id) },
            }
        }

        fn bootstrap_kademlia(&mut self) {
            let boot_nodes_string = fs::read_to_string(BOOTSTRAP_NODES_FILE_PATH)
                .expect("Can't read bootstrap nodes file.");
            let boot_nodes = serde_json::from_str::<NodesJson>(&boot_nodes_string)
                .expect("Can't parse bootstrap nodes file.");
            for index in 0..boot_nodes.nodes.len() {
                let node = boot_nodes.nodes[index].node.clone();
                let address = boot_nodes.nodes[index].address.clone();
                self.kademlia.add_address(
                    &node.parse().expect("Can't parse bootstrap node id"),
                    address.parse().expect("Can't parse bootstrap node address"),
                );
            }
            self.kademlia.bootstrap().expect("Cant bootstrap");
        }
    }

    #[derive(Debug)]
    #[allow(clippy::large_enum_variant)]
    enum ComposedEvent {
        RequestResponse(request_response::Event<FileRequest, FileResponse>),
        Kademlia(KademliaEvent),
        Identify(identify::Event),
        Gossipsub(gossipsub::Event),
        Relay(relay::client::Event),
        Dcutr(dcutr::Event),
    }

    impl From<request_response::Event<FileRequest, FileResponse>> for ComposedEvent {
        fn from(event: request_response::Event<FileRequest, FileResponse>) -> Self {
            ComposedEvent::RequestResponse(event)
        }
    }

    impl From<KademliaEvent> for ComposedEvent {
        fn from(event: KademliaEvent) -> Self {
            ComposedEvent::Kademlia(event)
        }
    }

    impl From<identify::Event> for ComposedEvent {
        fn from(event: identify::Event) -> Self {
            ComposedEvent::Identify(event)
        }
    }

    impl From<gossipsub::Event> for ComposedEvent {
        fn from(event: gossipsub::Event) -> Self {
            ComposedEvent::Gossipsub(event)
        }
    }

    impl From<relay::client::Event> for ComposedEvent {
        fn from(event: relay::client::Event) -> Self {
            ComposedEvent::Relay(event)
        }
    }

    impl From<dcutr::Event> for ComposedEvent {
        fn from(event: dcutr::Event) -> Self {
            ComposedEvent::Dcutr(event)
        }
    }

    #[derive(Debug)]
    enum Command {
        StartProviding {
            file_name: String,
            sender: oneshot::Sender<()>,
        },
        GetProviders {
            file_name: String,
            sender: oneshot::Sender<HashSet<PeerId>>,
        },
        PutRecord {
            key: String,
            value: String,
        },
        GetRecord {
            key: String,
            sender: oneshot::Sender<Record>,
        },
        RequestFile {
            file_name: String,
            peer: PeerId,
            sender: oneshot::Sender<Result<Vec<u8>, Box<dyn Error + Send>>>,
        },
        RespondFile {
            file: Vec<u8>,
            channel: ResponseChannel<FileResponse>,
        },
        SendMessage {
            msg: Vec<u8>,
            topic: String,
        },
        SubscribeToTopic {
            topic: String,
        },
    }

    #[derive(Debug)]
    pub enum Event {
        InboundRequest {
            request: String,
            channel: ResponseChannel<FileResponse>,
        },
    }

    #[derive(Debug, Clone)]
    struct FileExchangeProtocol();

    #[derive(Clone)]
    struct FileExchangeCodec();

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct FileRequest(String);

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct FileResponse(Vec<u8>);

    impl ProtocolName for FileExchangeProtocol {
        fn protocol_name(&self) -> &[u8] {
            "/file-exchange/0.1.0".as_bytes()
        }
    }

    #[async_trait]
    impl request_response::Codec for FileExchangeCodec {
        type Protocol = FileExchangeProtocol;
        type Request = FileRequest;
        type Response = FileResponse;

        async fn read_request<T>(
            &mut self,
            _: &FileExchangeProtocol,
            io: &mut T,
        ) -> tokio::io::Result<Self::Request>
        where
            T: AsyncRead + Unpin + Send,
        {
            let vec = read_length_prefixed(io, 1_000_000).await?; // TODO: update transfer maximum.

            if vec.is_empty() {
                return Err(tokio::io::ErrorKind::UnexpectedEof.into());
            }

            Ok(FileRequest(String::from_utf8(vec).unwrap()))
        }

        async fn read_response<T>(
            &mut self,
            _: &FileExchangeProtocol,
            io: &mut T,
        ) -> tokio::io::Result<Self::Response>
        where
            T: AsyncRead + Unpin + Send,
        {
            let vec = read_length_prefixed(io, 500_000_000).await?; // TODO: update transfer maximum.

            if vec.is_empty() {
                return Err(tokio::io::ErrorKind::UnexpectedEof.into());
            }

            Ok(FileResponse(vec))
        }

        async fn write_request<T>(
            &mut self,
            _: &FileExchangeProtocol,
            io: &mut T,
            FileRequest(data): FileRequest,
        ) -> tokio::io::Result<()>
        where
            T: AsyncWrite + Unpin + Send,
        {
            write_length_prefixed(io, data).await?;
            io.close().await?;

            Ok(())
        }

        async fn write_response<T>(
            &mut self,
            _: &FileExchangeProtocol,
            io: &mut T,
            FileResponse(data): FileResponse,
        ) -> tokio::io::Result<()>
        where
            T: AsyncWrite + Unpin + Send,
        {
            write_length_prefixed(io, data).await?;
            io.close().await?;

            Ok(())
        }
    }
}
