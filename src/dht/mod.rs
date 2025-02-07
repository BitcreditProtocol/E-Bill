use crate::config::Config;
use behaviour::{ComposedEvent, Event, MyBehaviour};
use borsh::{to_vec, BorshDeserialize};
use borsh_derive::{BorshDeserialize, BorshSerialize};
use event_loop::EventLoop;
use futures::channel::mpsc;
use futures::channel::mpsc::Receiver;
use futures::prelude::*;
use libp2p::core::transport::OrTransport;
use libp2p::core::upgrade::Version;
use libp2p::core::Multiaddr;
use libp2p::dns::DnsConfig;
use libp2p::multiaddr::Protocol;
use libp2p::multihash::Multihash;
use libp2p::swarm::{SwarmBuilder, SwarmEvent};
use libp2p::{identify, noise, relay, tcp, yamux, PeerId, Transport};
use tokio::{spawn, sync::broadcast};

pub mod behaviour;
mod client;
mod event_loop;

use crate::persistence::bill::{BillChainStoreApi, BillStoreApi};
use crate::persistence::company::{CompanyChainStoreApi, CompanyStoreApi};
use crate::persistence::file_upload::FileUploadStoreApi;
use crate::persistence::identity::IdentityStoreApi;
use crate::{blockchain, persistence, util, CONFIG};
pub use client::Client;
use log::{error, info};
use std::sync::Arc;
use thiserror::Error;

/// Generic result type
pub type Result<T> = std::result::Result<T, Error>;

/// Generic error type
#[derive(Debug, Error)]
pub enum Error {
    /// all errors originating from file, or network io, or binary serialization or deserialization
    #[error("io error {0}")]
    Io(#[from] std::io::Error),

    /// all errors originating from Noise Transport errors
    #[error("Noise error {0}")]
    Noise(#[from] libp2p::noise::Error),

    /// all errors originating from Transport errors
    #[error("Transport error {0}")]
    Transport(#[from] libp2p::TransportError<std::io::Error>),

    /// all errors originating from Identity parsing
    #[error("Identity parse error {0}")]
    IdentityParse(#[from] libp2p::identity::ParseError),

    /// all errors originating from Dial errors
    #[error("Dial error {0}")]
    Dial(#[from] libp2p::swarm::DialError),

    /// all errors originating invalid file requests
    #[error("Invalid File Request: {0}")]
    InvalidFileRequest(String),

    /// all errors originating from the persistence layer
    #[error("Persistence error: {0}")]
    Persistence(#[from] persistence::Error),

    /// all errors originating from running into utf8-related errors
    #[error("utf-8 error when parsing string {0}")]
    Utf8(#[from] std::str::Utf8Error),

    /// all errors originating from using a broken channel
    #[error("channel error {0}")]
    SendChannel(#[from] futures::channel::mpsc::SendError),

    /// all errors originating from using a closed onsehot channel
    #[error("oneshot channel cancel error {0}")]
    ChannelCanceled(#[from] futures::channel::oneshot::Canceled),

    /// error if there are no providers
    #[error("No providers found: {0}")]
    NoProviders(String),

    /// error with address parsing
    #[error("invalid address: {0}")]
    MultiAddr(#[from] libp2p::multiaddr::Error),

    /// error if a file wasn't returned from any provider
    #[error("No file returned from providers: {0}")]
    NoFileFromProviders(String),

    /// error if file hashes of two files did not match
    #[error("File hashes did not match: {0}")]
    FileHashesDidNotMatch(String),

    /// error if the caller is not a part of the bill
    #[allow(dead_code)]
    #[error("The caller {0} is not a part of the bill: {1}")]
    CallerNotPartOfBill(String, String),

    /// error if the caller is not a signatory for the company they want files on
    #[error("The caller {0} is not a signatory of the company {1}")]
    CallerNotSignatoryOfCompany(String, String),

    /// error if the caller requests a file for a company that doesn't exist
    #[error("The requested file {0} for the company {1} didn't exist")]
    NoFileForCompanyFound(String, String),

    /// error if requesting a file fails
    #[error("request file error: {0}")]
    RequestFile(String),

    /// error if getting a record fails
    #[error("get record error: {0}")]
    GetRecord(String),

    /// error if the listen url is invalid
    #[error("invalid listen p2p url error")]
    ListenP2pUrlInvalid,

    /// errors from internal crypto operations
    #[error("Cryptography error: {0}")]
    CryptoUtil(#[from] util::crypto::Error),

    /// errors that stem from interacting with a blockchain
    #[error("Blockchain error: {0}")]
    Blockchain(#[from] blockchain::Error),
}

pub struct Dht {
    pub client: Client,
    pub shutdown_sender: broadcast::Sender<bool>,
}

pub async fn dht_main(
    conf: &Config,
    bill_store: Arc<dyn BillStoreApi>,
    bill_blockchain_store: Arc<dyn BillChainStoreApi>,
    company_store: Arc<dyn CompanyStoreApi>,
    company_blockchain_store: Arc<dyn CompanyChainStoreApi>,
    identity_store: Arc<dyn IdentityStoreApi>,
    file_upload_store: Arc<dyn FileUploadStoreApi>,
) -> Result<Dht> {
    let (network_client, network_events, network_event_loop) = new(
        conf,
        bill_store,
        bill_blockchain_store,
        company_store,
        company_blockchain_store,
        identity_store,
        file_upload_store,
    )
    .await?;

    let (shutdown_sender, shutdown_receiver) = broadcast::channel::<bool>(100);

    spawn(network_event_loop.run(shutdown_receiver));

    let network_client_to_return = network_client.clone();
    let network_client_for_terminal_client = network_client.clone();

    spawn(network_client.run(network_events, shutdown_sender.subscribe()));

    if conf.terminal_client {
        spawn(util::terminal::run_terminal_client(
            shutdown_sender.subscribe(),
            network_client_for_terminal_client,
        ));
    }

    Ok(Dht {
        client: network_client_to_return,
        shutdown_sender,
    })
}

async fn new(
    conf: &Config,
    bill_store: Arc<dyn BillStoreApi>,
    bill_blockchain_store: Arc<dyn BillChainStoreApi>,
    company_store: Arc<dyn CompanyStoreApi>,
    company_blockchain_store: Arc<dyn CompanyChainStoreApi>,
    identity_store: Arc<dyn IdentityStoreApi>,
    file_upload_store: Arc<dyn FileUploadStoreApi>,
) -> Result<(Client, Receiver<Event>, EventLoop)> {
    let keys = identity_store.get_or_create_key_pair().await?;
    let local_public_key = keys.get_libp2p_keys()?;
    let local_node_id = keys.get_public_key();
    let local_peer_id = local_public_key.public().to_peer_id();
    info!("Local peer id: {local_peer_id:?}");
    info!("Local node id: {local_node_id:?}");
    info!("Local npub: {:?}", keys.get_nostr_npub()?);
    info!("Local npub as hex: {:?}", keys.get_nostr_npub_as_hex());

    let (relay_transport, client) = relay::client::new(local_peer_id);

    let dns_cfg = DnsConfig::system(tcp::tokio::Transport::new(
        tcp::Config::default().port_reuse(true),
    ))
    .await?;
    let transport = OrTransport::new(relay_transport, dns_cfg)
        .upgrade(Version::V1Lazy)
        .authenticate(noise::Config::new(&local_public_key)?)
        .multiplex(yamux::Config::default())
        .timeout(std::time::Duration::from_secs(20))
        .boxed();

    let behaviour = MyBehaviour::new(local_peer_id, local_public_key.clone(), client);

    let mut swarm = SwarmBuilder::with_tokio_executor(transport, behaviour, local_peer_id).build();

    swarm.listen_on(
        conf.p2p_listen_url()
            .map_err(|_| Error::ListenP2pUrlInvalid)?,
    )?;

    // Wait to listen on all interfaces.
    let sleep = tokio::time::sleep(std::time::Duration::from_secs(1));
    tokio::pin!(sleep);

    loop {
        tokio::select! {
            event = swarm.next() => {
                if let Some(evt) = event {
                    match evt {
                        SwarmEvent::NewListenAddr { address, .. } => {
                            info!("Listening on {:?}", address);
                        }
                        SwarmEvent::Behaviour { .. } => {
                        }
                        event => unreachable!("Unexpected event: {event:?}"),
                    }
                }
            }
            _ = &mut sleep => {
                // Likely listening on all interfaces now, thus continuing by breaking the loop.
                break;
            }
        }
    }

    let relay_peer_id: PeerId = CONFIG.relay_bootstrap_peer_id.clone().parse()?;
    let relay_address = CONFIG
        .relay_bootstrap_address
        .parse::<Multiaddr>()?
        .with(Protocol::P2p(Multihash::from(relay_peer_id)));
    info!("Relay address: {:?}", relay_address);

    swarm.dial(relay_address.clone())?;
    let mut learned_observed_addr = false;
    let mut told_relay_observed_addr = false;

    loop {
        if let Some(event) = swarm.next().await {
            match event {
                SwarmEvent::NewListenAddr { .. } => {}
                SwarmEvent::Dialing { .. } => {}
                SwarmEvent::ConnectionEstablished { .. } => {}
                SwarmEvent::Behaviour(ComposedEvent::Identify(identify::Event::Sent {
                    ..
                })) => {
                    info!("Told relay its public address.");
                    told_relay_observed_addr = true;
                }
                SwarmEvent::Behaviour(ComposedEvent::Identify(identify::Event::Received {
                    info: identify::Info { observed_addr, .. },
                    ..
                })) => {
                    info!("Relay told us our public address: {:?}", observed_addr);
                    learned_observed_addr = true;
                }
                SwarmEvent::Behaviour { .. } => {}
                event => unreachable!("Unexpected event: {event:?}"),
            }

            if learned_observed_addr && told_relay_observed_addr {
                break;
            }
        }
    }

    swarm.behaviour_mut().bootstrap_kademlia();

    swarm.listen_on(relay_address.clone().with(Protocol::P2pCircuit))?;

    loop {
        if let Some(event) = swarm.next().await {
            match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    info!("Listening on {:?}", address);
                    break;
                }
                SwarmEvent::Behaviour(ComposedEvent::Relay(
                    relay::client::Event::ReservationReqAccepted { .. },
                )) => {
                    info!("Relay accepted our reservation request.");
                }
                SwarmEvent::Behaviour(ComposedEvent::Relay(event)) => {
                    info!("Relay event: {:?}", event)
                }
                SwarmEvent::Behaviour(ComposedEvent::Dcutr(event)) => {
                    info!("Dcutr event: {:?}", event)
                }
                SwarmEvent::Behaviour(ComposedEvent::Identify(event)) => {
                    info!("Identify event: {:?}", event)
                }
                SwarmEvent::ConnectionEstablished {
                    peer_id, endpoint, ..
                } => {
                    info!("Established connection to {:?} via {:?}", peer_id, endpoint);
                }
                SwarmEvent::OutgoingConnectionError { peer_id, error } => {
                    error!("Outgoing connection error to {:?}: {:?}", peer_id, error);
                }
                SwarmEvent::Behaviour(event) => {
                    info!("Behaviour event: {event:?}")
                }
                _ => {}
            }
        }
    }

    let (command_sender, command_receiver) = mpsc::channel(0);
    let (event_sender, event_receiver) = mpsc::channel(0);
    let event_loop = EventLoop::new(
        swarm,
        command_receiver,
        event_sender,
        bill_store.clone(),
        bill_blockchain_store.clone(),
    );

    Ok((
        Client::new(
            command_sender,
            bill_store,
            bill_blockchain_store,
            company_store,
            company_blockchain_store,
            identity_store,
            file_upload_store,
        ),
        event_receiver,
        event_loop,
    ))
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct GossipsubEvent {
    pub id: GossipsubEventId,
    pub message: Vec<u8>,
}

impl GossipsubEvent {
    pub fn new(id: GossipsubEventId, message: Vec<u8>) -> Self {
        Self { id, message }
    }

    pub fn to_byte_array(&self) -> Result<Vec<u8>> {
        let res = to_vec(self)?;
        Ok(res)
    }

    pub fn from_byte_array(bytes: &[u8]) -> Result<Self> {
        let res = Self::try_from_slice(bytes)?;
        Ok(res)
    }
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub enum GossipsubEventId {
    BillBlock,
    BillBlockchain,
    CommandGetBillBlockchain,
    AddSignatoryFromCompany,
    RemoveSignatoryFromCompany,
}
