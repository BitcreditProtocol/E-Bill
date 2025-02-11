use super::behaviour::{Command, ComposedEvent, Event, MyBehaviour};
use super::{GossipsubEvent, GossipsubEventId};
use crate::blockchain::bill::{BillBlock, BillBlockchain};
use crate::blockchain::Blockchain;
use crate::constants::{BILL_PREFIX, COMPANY_PREFIX};
use crate::dht::behaviour::{CompanyEvent, FileRequest, FileResponse};
use crate::persistence::bill::{BillChainStoreApi, BillStoreApi};
use crate::CONFIG;
use anyhow::{anyhow, Result};
use borsh::{from_slice, to_vec};
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
use log::{error, info, warn};
use std::collections::{HashMap, HashSet};
use std::iter;
use std::sync::Arc;
use tokio::sync::broadcast;

type PendingDial = HashMap<PeerId, oneshot::Sender<Result<()>>>;
type PendingRequestFile = HashMap<RequestId, oneshot::Sender<Result<Vec<u8>>>>;

pub struct EventLoop {
    swarm: Swarm<MyBehaviour>,
    command_receiver: Receiver<Command>,
    event_sender: mpsc::Sender<Event>,
    #[allow(dead_code)]
    bill_store: Arc<dyn BillStoreApi>,
    bill_blockchain_store: Arc<dyn BillChainStoreApi>,
    pending_dial: PendingDial,
    pending_start_providing: HashMap<QueryId, oneshot::Sender<()>>,
    pending_get_providers: HashMap<QueryId, oneshot::Sender<HashSet<PeerId>>>,
    pending_get_records: HashMap<QueryId, oneshot::Sender<Result<Record>>>,
    pending_request_file: PendingRequestFile,
}

impl EventLoop {
    pub fn new(
        swarm: Swarm<MyBehaviour>,
        command_receiver: Receiver<Command>,
        event_sender: mpsc::Sender<Event>,
        bill_store: Arc<dyn BillStoreApi>,
        bill_blockchain_store: Arc<dyn BillChainStoreApi>,
    ) -> Self {
        Self {
            swarm,
            command_receiver,
            event_sender,
            bill_store,
            bill_blockchain_store,
            pending_dial: Default::default(),
            pending_start_providing: Default::default(),
            pending_get_providers: Default::default(),
            pending_get_records: Default::default(),
            pending_request_file: Default::default(),
        }
    }

    pub async fn run(mut self, mut shutdown_event_loop_receiver: broadcast::Receiver<bool>) {
        loop {
            tokio::select! {
                event = self.swarm.next() => {
                    if let Some(evt) = event {
                        if let Err(e) = self.handle_event(evt).await {
                            error!("EventLoop Error while handling event: {e}");
                        }
                    }
                },
                command = self.command_receiver.next() => {
                    if let Some(c) = command {
                        if let Err(e) = self.handle_command(c).await {
                            error!("EventLoop Error while handling command: {e}");
                        }
                    }
                },
                _ = shutdown_event_loop_receiver.recv() => {
                    info!("Shutting down event loop...");
                    break;
                }
            }
        }
    }

    async fn handle_event<T>(&mut self, event: SwarmEvent<ComposedEvent, T>) -> Result<()>
    where
        T: std::fmt::Debug,
    {
        match event {
            //--------------KADEMLIA EVENTS--------------
            SwarmEvent::Behaviour(ComposedEvent::Kademlia(
                KademliaEvent::OutboundQueryProgressed { result, id, .. },
            )) => match result {
                QueryResult::StartProviding(Ok(kad::AddProviderOk { key: _ })) => {
                    info!("Successfully started providing query id: {id:?}");
                    if let Some(sender) = self.pending_start_providing.remove(&id) {
                        sender.send(()).map_err(|e| {
                            anyhow!("Error sending event on start providing channel: {e:?}")
                        })?;
                    }
                }

                QueryResult::GetRecord(Ok(GetRecordOk::FoundRecord(PeerRecord {
                    record, ..
                }))) => {
                    if let Some(sender) = self.pending_get_records.remove(&id) {
                        info!(
                            "Got record {:?} {:?}",
                            std::str::from_utf8(record.key.as_ref()),
                            std::str::from_utf8(&record.value),
                        );

                        sender.send(Ok(record)).map_err(|e| {
                            anyhow!("Error sending record from get_record event: {e:?}")
                        })?;

                        // Finish the query. We are only interested in the first result.
                        //TODO: think how to do it better.
                        if let Some(mut query) = self.swarm.behaviour_mut().kademlia.query_mut(&id)
                        {
                            query.finish();
                        }
                    }
                }

                QueryResult::GetRecord(Ok(GetRecordOk::FinishedWithNoAdditionalRecord {
                    ..
                })) => {
                    self.pending_get_records.remove(&id);
                    info!("No records.");
                }

                QueryResult::GetRecord(Err(GetRecordError::NotFound { key, .. })) => {
                    if let Some(sender) = self.pending_get_records.remove(&id) {
                        sender
                            .send(Err(anyhow!("Get Record Error: NotFound for {key:?}")))
                            .map_err(|e| {
                                anyhow!("Error sending Get Record NotFound error: {e:?}")
                            })?;
                    }
                }

                QueryResult::GetRecord(Err(GetRecordError::Timeout { key })) => {
                    if let Some(sender) = self.pending_get_records.remove(&id) {
                        sender
                            .send(Err(anyhow!("Get Record Error: Timeout for {key:?}")))
                            .map_err(|e| {
                                anyhow!("Error sending Get Record Timeout error: {e:?}")
                            })?;
                    }
                }

                QueryResult::GetRecord(Err(GetRecordError::QuorumFailed { key, .. })) => {
                    if let Some(sender) = self.pending_get_records.remove(&id) {
                        sender
                            .send(Err(anyhow!("Get Record Error: Quorumfailed for {key:?}")))
                            .map_err(|e| {
                                anyhow!("Error sending Get Record QuorumFailed error: {e:?}")
                            })?;
                    }
                }

                QueryResult::GetProviders(Ok(GetProvidersOk::FoundProviders {
                    providers, ..
                })) => {
                    if let Some(sender) = self.pending_get_providers.remove(&id) {
                        for peer in &providers {
                            info!("Get Providers: PEER {peer:?}");
                        }

                        sender.send(providers).map_err(|e| {
                            anyhow!("Error sending providers from get_providers event: {e:?}")
                        })?;

                        // Finish the query. We are only interested in the first result.
                        //TODO: think how to do it better.
                        if let Some(mut query) = self.swarm.behaviour_mut().kademlia.query_mut(&id)
                        {
                            query.finish();
                        }
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
                if let Some(sender) = self.pending_request_file.remove(&request_id) {
                    sender
                        .send(Err(error.into()))
                        .map_err(|e| anyhow!("Error sending File request error: {e:?}"))?;
                }
            }

            SwarmEvent::Behaviour(ComposedEvent::RequestResponse(
                request_response::Event::Message { message, peer },
            )) => match message {
                request_response::Message::Request {
                    request, channel, ..
                } => {
                    self.event_sender
                        .send(Event::InboundRequest {
                            request: request.0,
                            channel,
                            peer,
                        })
                        .await
                        .map_err(|e| anyhow!("Error sending File request: {e:?}"))?;
                }

                request_response::Message::Response {
                    request_id,
                    response,
                } => {
                    if let Some(sender) = self.pending_request_file.remove(&request_id) {
                        sender
                            .send(Ok(response.0))
                            .map_err(|e| anyhow!("Error sending File response: {e:?}"))?;
                    }
                }
            },

            SwarmEvent::Behaviour(ComposedEvent::RequestResponse(
                request_response::Event::ResponseSent { .. },
            )) => {
                info!("ResponseSent event: {event:?}")
            }

            //--------------IDENTIFY EVENTS--------------
            SwarmEvent::Behaviour(ComposedEvent::Identify(event)) => {
                info!("Identify event: {:?}", event)
            }

            //--------------DCUTR EVENTS--------------
            SwarmEvent::Behaviour(ComposedEvent::Dcutr(event)) => {
                info!("Dcutr event: {:?}", event)
            }

            //--------------RELAY EVENTS--------------
            SwarmEvent::Behaviour(ComposedEvent::Relay(
                relay::client::Event::ReservationReqAccepted { .. },
            )) => {
                info!("ReservationReqAccepted event: {event:?}");
                info!("Relay accepted our reservation request.");
            }

            SwarmEvent::Behaviour(ComposedEvent::Relay(event)) => {
                info!("{:?}", event)
            }

            //--------------GOSSIPSUB EVENTS--------------
            SwarmEvent::Behaviour(ComposedEvent::Gossipsub(gossipsub::Event::Message {
                propagation_source: node_id,
                message_id: id,
                message,
            })) => {
                info!(
                    "Got message with id: {id} from peer: {node_id} in topic: {}",
                    message.topic.as_str()
                );
                if message.topic.as_str().starts_with(COMPANY_PREFIX) {
                    if let Some(company_id) = message.topic.as_str().strip_prefix(COMPANY_PREFIX) {
                        if let Ok(event) = GossipsubEvent::from_byte_array(&message.data) {
                            match event.id {
                                GossipsubEventId::AddSignatoryFromCompany => {
                                    if let Ok(signatory) = String::from_utf8(event.message) {
                                        if let Err(e) = self
                                            .event_sender
                                            .send(Event::CompanyUpdate {
                                                event: CompanyEvent::AddSignatory,
                                                company_id: company_id.to_string(),
                                                signatory,
                                            })
                                            .await
                                        {
                                            error!("Could not send event to DHT client: {e}");
                                        }
                                    }
                                }
                                GossipsubEventId::RemoveSignatoryFromCompany => {
                                    if let Ok(signatory) = String::from_utf8(event.message) {
                                        if let Err(e) = self
                                            .event_sender
                                            .send(Event::CompanyUpdate {
                                                event: CompanyEvent::RemoveSignatory,
                                                company_id: company_id.to_string(),
                                                signatory,
                                            })
                                            .await
                                        {
                                            error!("Could not send event to DHT client: {e}");
                                        }
                                    }
                                }
                                _ => {
                                    warn!("Unknown event: {event:?}");
                                }
                            }
                        }
                    }
                } else if message.topic.as_str().starts_with(BILL_PREFIX) {
                    if let Some(bill_id) = message.topic.as_str().strip_prefix(BILL_PREFIX) {
                        if let Ok(event) = GossipsubEvent::from_byte_array(&message.data) {
                            match event.id {
                                GossipsubEventId::BillBlock => {
                                    if let Ok(block) = from_slice::<BillBlock>(&event.message) {
                                        if let Ok(mut chain) =
                                            self.bill_blockchain_store.get_chain(bill_id).await
                                        {
                                            chain.try_add_block(block.clone());
                                            if chain.is_chain_valid() {
                                                if let Err(e) = self
                                                    .bill_blockchain_store
                                                    .add_block(bill_id, &block)
                                                    .await
                                                {
                                                    error!("Could not add block (id: {}, hash: {}) for bill {bill_id}: {e}", block.id, block.hash);
                                                }
                                            }
                                        }
                                    }
                                }
                                GossipsubEventId::BillBlockchain => {
                                    if let Ok(remote_chain) =
                                        from_slice::<BillBlockchain>(&event.message)
                                    {
                                        if let Ok(mut local_chain) =
                                            self.bill_blockchain_store.get_chain(bill_id).await
                                        {
                                            info!("Received blockchain for bill: {bill_id} from peer: {node_id}, local block height: {}, remote block height: {}", local_chain.block_height(), remote_chain.block_height());
                                            let blocks_to_add = local_chain
                                                .get_blocks_to_add_from_other_chain(&remote_chain);
                                            for block in blocks_to_add {
                                                if let Err(e) = self
                                                    .bill_blockchain_store
                                                    .add_block(bill_id, &block)
                                                    .await
                                                {
                                                    error!("Could not add block (id: {}, hash: {}) for bill {bill_id}: {e}", block.id, block.hash);
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }
                                GossipsubEventId::CommandGetBillBlockchain => {
                                    if let Ok(chain) =
                                        self.bill_blockchain_store.get_chain(bill_id).await
                                    {
                                        if let Ok(chain_bytes) = to_vec(&chain) {
                                            let event = GossipsubEvent::new(
                                                GossipsubEventId::BillBlockchain,
                                                chain_bytes,
                                            );
                                            if let Ok(message) = event.to_byte_array() {
                                                if let Err(e) =
                                                    self.swarm.behaviour_mut().gossipsub.publish(
                                                        gossipsub::IdentTopic::new(format!(
                                                            "{BILL_PREFIX}{bill_id}"
                                                        )),
                                                        message,
                                                    )
                                                {
                                                    error!(
                                                        "Could not publish event: {event:?}: {e}"
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                                _ => {
                                    warn!("Unknown event: {event:?}");
                                }
                            }
                        }
                    }
                }
            }
            //--------------OTHERS BEHAVIOURS EVENTS--------------
            SwarmEvent::Behaviour(event) => {
                info!("Behaviour event: {event:?}")
            }

            //--------------COMMON EVENTS--------------
            SwarmEvent::NewListenAddr { address, .. } => {
                info!("Listening on {:?}", address);
            }

            SwarmEvent::IncomingConnection { .. } => {
                info!("IncomingConnection event: {event:?}")
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
                info!("ConnectionClosed event: {event:?}")
            }

            SwarmEvent::OutgoingConnectionError { .. } => {
                error!("OutgoingConnectionError event {event:?}");
                // if let Some(node_id) = node_id {
                //     if let Some(sender) = self.pending_dial.remove(&node_id) {
                //         let _ = sender.send(Err(Box::new(error)));
                //     }
                // }
            }

            SwarmEvent::IncomingConnectionError { .. } => {
                error!("IncomingConnectionError event: {event:?}")
            }

            _ => {}
        }
        Ok(())
    }

    async fn handle_command(&mut self, command: Command) -> Result<()> {
        match command {
            Command::StartProviding { entry, sender } => {
                info!("Start providing {entry:?}");
                let query_id = self
                    .swarm
                    .behaviour_mut()
                    .kademlia
                    .start_providing(entry.clone().into_bytes().into())
                    .map_err(|e| anyhow!("Error providing entry {:?}: {e:?}", &entry))?;
                self.pending_start_providing.insert(query_id, sender);
            }

            Command::StopProviding { entry } => {
                info!("Stop providing {entry:?}");
                self.swarm
                    .behaviour_mut()
                    .kademlia
                    .stop_providing(&entry.into_bytes().into());
            }

            Command::PutRecord { key, value } => {
                info!("Put record {key:?}");
                let key_record = Key::new(&key);
                let record = Record {
                    key: key_record,
                    value,
                    publisher: None,
                    expires: None,
                };

                let relay_peer_id: PeerId = CONFIG
                    .relay_bootstrap_peer_id
                    .clone()
                    .parse()
                    .map_err(|e| {
                        anyhow!("Error parsing relay peer id when putting record: {e:?}")
                    })?;

                let _query_id = self
                    .swarm
                    .behaviour_mut()
                    .kademlia
                    //TODO: what quorum use?
                    .put_record_to(record, iter::once(relay_peer_id), Quorum::All);
            }

            Command::SendMessage { msg, topic } => {
                if !topic.is_empty() {
                    info!("Send message to topic {:?}", &topic);
                    let swarm = self.swarm.behaviour_mut();
                    swarm
                        .gossipsub
                        .publish(gossipsub::IdentTopic::new(&topic), msg)
                        .map_err(|e| {
                            anyhow!("Error publishing message to topic {:?}: {e:?}", &topic)
                        })?;
                }
            }

            Command::SubscribeToTopic { topic } => {
                info!("Subscribe to topic {topic:?}");
                self.swarm
                    .behaviour_mut()
                    .gossipsub
                    .subscribe(&gossipsub::IdentTopic::new(topic))
                    .map_err(|e| anyhow!("Error subscribing to topic: {e:?}"))?;
            }

            Command::UnsubscribeFromTopic { topic } => {
                info!("Unsubscribe from topic {topic:?}");
                self.swarm
                    .behaviour_mut()
                    .gossipsub
                    .unsubscribe(&gossipsub::IdentTopic::new(topic))
                    .map_err(|e| anyhow!("Error unsubsribing from topic: {e:?}"))?;
            }

            Command::GetRecord { key, sender } => {
                info!("Get record {key:?}");
                let key_record = Key::new(&key);
                let query_id = self.swarm.behaviour_mut().kademlia.get_record(key_record);
                self.pending_get_records.insert(query_id, sender);
            }

            Command::GetProviders { entry, sender } => {
                info!("Get providers {entry:?}");
                let query_id = self
                    .swarm
                    .behaviour_mut()
                    .kademlia
                    .get_providers(entry.into_bytes().into());
                self.pending_get_providers.insert(query_id, sender);
            }

            Command::RequestFile {
                file_name,
                peer,
                sender,
            } => {
                info!("Request file {file_name:?}");

                let relay_peer_id: PeerId = CONFIG
                    .relay_bootstrap_peer_id
                    .clone()
                    .parse()
                    .map_err(|e| {
                        anyhow!("Error parsing relay peer id when requesting file: {e:?}")
                    })?;

                let relay_address = CONFIG
                    .relay_bootstrap_address
                    .parse::<Multiaddr>()?
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
                info!("Respond file");
                self.swarm
                    .behaviour_mut()
                    .request_response
                    .send_response(channel, FileResponse(file))
                    .map_err(|e| anyhow!("Error while sending file response: {e:?}"))?;
            }
        }
        Ok(())
    }
}
