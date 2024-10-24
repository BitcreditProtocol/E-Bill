use super::behaviour::{Command, ComposedEvent, Event, MyBehaviour};
use crate::blockchain::{Block, Chain, GossipsubEvent, GossipsubEventId};
use crate::constants::{
    RELAY_BOOTSTRAP_NODE_ONE_IP, RELAY_BOOTSTRAP_NODE_ONE_PEER_ID, RELAY_BOOTSTRAP_NODE_ONE_TCP,
};
use crate::dht::behaviour::{FileRequest, FileResponse};
use futures::channel::mpsc::Receiver;
use futures::channel::{mpsc, oneshot};
use futures::prelude::*;
use libp2p::core::Multiaddr;
use libp2p::kad::record::{Key, Record};
use libp2p::kad::{
    self, GetProvidersOk, GetRecordError, GetRecordOk, KademliaEvent, PeerRecord, QueryId,
    QueryResult, Quorum,
};
use libp2p::multiaddr::Protocol;
use libp2p::multihash::Multihash;
use libp2p::request_response::{self, RequestId};
use libp2p::swarm::{Swarm, SwarmEvent};
use libp2p::{gossipsub, relay, PeerId};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::iter;

type PendingDial = HashMap<PeerId, oneshot::Sender<Result<(), Box<dyn Error + Send>>>>;
type PendingRequestFile =
    HashMap<RequestId, oneshot::Sender<Result<Vec<u8>, Box<dyn Error + Send>>>>;

pub struct EventLoop {
    swarm: Swarm<MyBehaviour>,
    command_receiver: Receiver<Command>,
    event_sender: mpsc::Sender<Event>,
    pending_dial: PendingDial,
    pending_start_providing: HashMap<QueryId, oneshot::Sender<()>>,
    pending_get_providers: HashMap<QueryId, oneshot::Sender<HashSet<PeerId>>>,
    pending_get_records: HashMap<QueryId, oneshot::Sender<Record>>,
    pending_request_file: PendingRequestFile,
}

impl EventLoop {
    pub fn new(
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

    async fn handle_event<T>(&mut self, event: SwarmEvent<ComposedEvent, T>)
    where
        T: std::fmt::Debug,
    {
        match event {
            //--------------KADEMLIA EVENTS--------------
            SwarmEvent::Behaviour(ComposedEvent::Kademlia(
                KademliaEvent::OutboundQueryProgressed { result, id, .. },
            )) => match result {
                QueryResult::StartProviding(Ok(kad::AddProviderOk { key: _ })) => {
                    let sender: oneshot::Sender<()> = self
                        .pending_start_providing
                        .remove(&id)
                        .expect("Completed query to be previously pending.");
                    let _ = sender.send(());
                }

                QueryResult::GetRecord(Ok(GetRecordOk::FoundRecord(PeerRecord {
                    record, ..
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
                    providers, ..
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
                println!("Got message with id: {id} from peer: {peer_id} in topic: {bill_name}",);
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
                    let chain_bytes = serde_json::to_vec(&chain).expect("Can not serialize chain.");
                    let event = GossipsubEvent::new(GossipsubEventId::Chain, chain_bytes);
                    let message = event.to_byte_array();
                    self.swarm
                        .behaviour_mut()
                        .gossipsub
                        .publish(gossipsub::IdentTopic::new(bill_name.clone()), message)
                        .expect("Can not publish message.");
                } else {
                    println!("Unknown event id: {id} from peer: {peer_id} in topic: {bill_name}");
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
