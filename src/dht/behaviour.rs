use super::Error;
use crate::constants::{
    BILL_ATTACHMENT_PREFIX, BILL_PREFIX, COMPANY_CHAIN_PREFIX, COMPANY_KEY_PREFIX,
    COMPANY_LOGO_PREFIX, COMPANY_PREFIX, COMPANY_PROOF_PREFIX, KEY_PREFIX, MAX_FILE_SIZE_BYTES,
};
use crate::{util, CONFIG};
use anyhow::Result;
use async_trait::async_trait;
use futures::channel::oneshot;
use futures::prelude::*;
use libp2p::core::upgrade::{read_length_prefixed, write_length_prefixed, ProtocolName};
use libp2p::identity::Keypair;
use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::record::Record;
use libp2p::kad::{Kademlia, KademliaEvent};
use libp2p::request_response::{self, ProtocolSupport, ResponseChannel};
use libp2p::swarm::NetworkBehaviour;
use libp2p::{dcutr, gossipsub, identify, relay, PeerId};
use log::warn;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::iter;

#[derive(Deserialize, Serialize, Debug)]
struct Nodes {
    node: String,
    address: String,
}

#[derive(Deserialize, Serialize, Debug)]
struct NodesJson {
    nodes: Vec<Nodes>,
}

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "ComposedEvent", event_process = false)]
pub struct MyBehaviour {
    pub request_response: request_response::Behaviour<FileExchangeCodec>,
    pub kademlia: Kademlia<MemoryStore>,
    pub identify: identify::Behaviour,
    pub gossipsub: gossipsub::Behaviour,
    pub relay_client: relay::client::Behaviour,
    pub dcutr: dcutr::Behaviour,
}

impl MyBehaviour {
    pub fn new(
        local_peer_id: PeerId,
        local_keypair: Keypair,
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
                let cfg_identify =
                    identify::Config::new("/identify/0.1.0".to_string(), local_keypair.public());
                identify::Behaviour::new(cfg_identify)
            },
            gossipsub: {
                let gossipsub_config = libp2p::gossipsub::Config::default();
                let message_authenticity =
                    gossipsub::MessageAuthenticity::Signed(local_keypair.clone());
                gossipsub::Behaviour::new(message_authenticity, gossipsub_config)
                    .expect("Correct configuration")
            },
            relay_client: { client },
            dcutr: { dcutr::Behaviour::new(local_peer_id) },
        }
    }

    pub fn bootstrap_kademlia(&mut self) {
        self.kademlia.add_address(
            &CONFIG
                .relay_bootstrap_peer_id
                .parse()
                .expect("Can't parse bootstrap node id"),
            CONFIG
                .relay_bootstrap_address
                .parse()
                .expect("Can't parse bootstrap node address"),
        );
        self.kademlia.bootstrap().expect("Cant bootstrap");
    }
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ComposedEvent {
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
pub enum Command {
    StartProviding {
        entry: String,
        sender: oneshot::Sender<()>,
    },
    StopProviding {
        entry: String,
    },
    GetProviders {
        entry: String,
        sender: oneshot::Sender<HashSet<PeerId>>,
    },
    PutRecord {
        key: String,
        value: Vec<u8>,
    },
    GetRecord {
        key: String,
        sender: oneshot::Sender<anyhow::Result<Record>>,
    },
    RequestFile {
        file_name: String,
        peer: PeerId,
        sender: oneshot::Sender<anyhow::Result<Vec<u8>>>,
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
    UnsubscribeFromTopic {
        topic: String,
    },
}

#[derive(Debug)]
pub enum Event {
    InboundRequest {
        request: String,
        peer: PeerId,
        channel: ResponseChannel<FileResponse>,
    },
    CompanyUpdate {
        event: CompanyEvent,
        company_id: String,
        signatory: String,
    },
}

#[derive(Debug, Clone)]
pub enum CompanyEvent {
    AddSignatory,
    RemoveSignatory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedInboundFileRequest {
    Bill(BillFileRequest),
    BillKeys(BillKeysFileRequest),
    BillAttachment(BillAttachmentFileRequest),
    CompanyData(CompanyDataRequest),
    CompanyKeys(CompanyKeysRequest),
    CompanyChain(CompanyChainRequest),
    CompanyLogo(CompanyLogoRequest),
    CompanyProof(CompanyProofRequest),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanyDataRequest {
    pub node_id: String,
    pub company_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanyKeysRequest {
    pub node_id: String,
    pub company_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanyChainRequest {
    pub node_id: String,
    pub company_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanyLogoRequest {
    pub company_id: String,
    pub node_id: String,
    pub file_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanyProofRequest {
    pub company_id: String,
    pub node_id: String,
    pub file_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BillFileRequest {
    pub bill_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BillKeysFileRequest {
    pub node_id: String,
    pub bill_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BillAttachmentFileRequest {
    pub node_id: String,
    pub bill_id: String,
    pub file_name: String,
}

pub fn file_request_for_bill_attachment(node_id: &str, bill_id: &str, file_name: &str) -> String {
    format!("{node_id}_{BILL_ATTACHMENT_PREFIX}_{bill_id}_{file_name}")
}

pub fn file_request_for_bill(node_id: &str, bill_id: &str) -> String {
    format!("{node_id}_{BILL_PREFIX}_{bill_id}")
}

pub fn file_request_for_bill_keys(node_id: &str, bill_id: &str) -> String {
    format!("{node_id}_{KEY_PREFIX}_{bill_id}")
}

pub fn file_request_for_company_data(node_id: &str, company_id: &str) -> String {
    format!("{node_id}_{COMPANY_PREFIX}_{company_id}")
}

pub fn file_request_for_company_keys(node_id: &str, company_id: &str) -> String {
    format!("{node_id}_{COMPANY_KEY_PREFIX}_{company_id}")
}

pub fn file_request_for_company_chain(node_id: &str, company_id: &str) -> String {
    format!("{node_id}_{COMPANY_CHAIN_PREFIX}_{company_id}")
}

pub fn file_request_for_company_logo(node_id: &str, company_id: &str, file_name: &str) -> String {
    format!("{node_id}_{COMPANY_LOGO_PREFIX}_{company_id}_{file_name}")
}

pub fn file_request_for_company_proof(node_id: &str, company_id: &str, file_name: &str) -> String {
    format!("{node_id}_{COMPANY_PROOF_PREFIX}_{company_id}_{file_name}")
}

pub fn parse_inbound_file_request(
    request: &str,
    peer_id: &PeerId,
) -> super::Result<ParsedInboundFileRequest> {
    let parts = request.splitn(4, "_").collect::<Vec<&str>>();
    if parts.len() < 3 {
        return Err(Error::InvalidFileRequest(format!(
            "need at least 3 parts in {request}"
        )));
    }

    let node_id = parts[0].to_owned();
    if !util::crypto::is_peer_id_from_this_node_id(&node_id, peer_id) {
        // TODO: add back as error - or remove libp2p file transfer entirely
        // return Err(Error::InvalidFileRequest(
        warn!("request was not sent by the owner of node_id: {node_id}, but by {peer_id:?}: {request}");
        // ));
    }
    let prefix = parts[1];
    match prefix {
        BILL_PREFIX => Ok(ParsedInboundFileRequest::Bill(BillFileRequest {
            bill_id: parts[2].to_owned(),
        })),
        KEY_PREFIX => Ok(ParsedInboundFileRequest::BillKeys(BillKeysFileRequest {
            node_id,
            bill_id: parts[2].to_owned(),
        })),
        BILL_ATTACHMENT_PREFIX => {
            if parts.len() < 4 {
                return Err(Error::InvalidFileRequest(format!(
                    "need at least 4 parts in {request}"
                )));
            }
            Ok(ParsedInboundFileRequest::BillAttachment(
                BillAttachmentFileRequest {
                    node_id,
                    bill_id: parts[2].to_owned(),
                    file_name: parts[3].to_owned(),
                },
            ))
        }
        COMPANY_PREFIX => Ok(ParsedInboundFileRequest::CompanyData(CompanyDataRequest {
            company_id: parts[2].to_owned(),
            node_id,
        })),
        COMPANY_KEY_PREFIX => Ok(ParsedInboundFileRequest::CompanyKeys(CompanyKeysRequest {
            company_id: parts[2].to_owned(),
            node_id,
        })),
        COMPANY_CHAIN_PREFIX => Ok(ParsedInboundFileRequest::CompanyChain(
            CompanyChainRequest {
                company_id: parts[2].to_owned(),
                node_id,
            },
        )),
        COMPANY_LOGO_PREFIX => {
            if parts.len() < 4 {
                return Err(Error::InvalidFileRequest(format!(
                    "need at least 4 parts in {request}"
                )));
            }
            Ok(ParsedInboundFileRequest::CompanyLogo(CompanyLogoRequest {
                company_id: parts[2].to_owned(),
                file_name: parts[3].to_owned(),
                node_id,
            }))
        }
        COMPANY_PROOF_PREFIX => {
            if parts.len() < 4 {
                return Err(Error::InvalidFileRequest(format!(
                    "need at least 4 parts in {request}"
                )));
            }
            Ok(ParsedInboundFileRequest::CompanyProof(
                CompanyProofRequest {
                    company_id: parts[2].to_owned(),
                    file_name: parts[3].to_owned(),
                    node_id,
                },
            ))
        }
        _ => Err(Error::InvalidFileRequest(format!(
            "no prefix matched in {request}"
        ))),
    }
}

#[derive(Debug, Clone)]
pub struct FileExchangeProtocol();

#[derive(Clone)]
pub struct FileExchangeCodec();

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRequest(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileResponse(pub Vec<u8>);

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
        let vec = read_length_prefixed(io, MAX_FILE_SIZE_BYTES).await?;

        if vec.is_empty() {
            return Err(tokio::io::ErrorKind::UnexpectedEof.into());
        }

        Ok(FileRequest(
            String::from_utf8(vec).map_err(|_| tokio::io::ErrorKind::InvalidData)?,
        ))
    }

    async fn read_response<T>(
        &mut self,
        _: &FileExchangeProtocol,
        io: &mut T,
    ) -> tokio::io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let vec = read_length_prefixed(io, MAX_FILE_SIZE_BYTES).await?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use util::BcrKeys;

    #[test]
    fn parse_inbound_file_request_too_short() {
        assert!(parse_inbound_file_request("", &PeerId::random()).is_err());
        assert!(parse_inbound_file_request("a_b", &PeerId::random()).is_err());
        assert!(parse_inbound_file_request("_b", &PeerId::random()).is_err());
        assert!(parse_inbound_file_request("b_", &PeerId::random()).is_err());
    }

    #[test]
    fn parse_inbound_file_request_prefixes() {
        let keys = BcrKeys::new();
        let peer_id = keys.get_libp2p_keys().unwrap().public().to_peer_id();
        let node_id = keys.get_public_key();
        assert!(parse_inbound_file_request("node_id_BLA_TEST", &PeerId::random()).is_err());
        assert!(parse_inbound_file_request("node_id_BLA_TEST_TEST", &PeerId::random()).is_err());
        assert!(parse_inbound_file_request(&format!("{node_id}_BILL_TEST"), &peer_id).is_ok());
        assert!(parse_inbound_file_request(&format!("{node_id}_KEY_TEST"), &peer_id).is_ok());
        assert!(
            parse_inbound_file_request(&format!("{node_id}_BILLATT_TEST_TEST"), &peer_id).is_ok()
        );
        assert!(parse_inbound_file_request(&format!("{node_id}_COMPANY_TEST"), &peer_id).is_ok());
        assert!(
            parse_inbound_file_request(&format!("{node_id}_COMPANYKEY_TEST"), &peer_id).is_ok()
        );
        assert!(
            parse_inbound_file_request(&format!("{node_id}_COMPANYLOGO_TEST_TEST"), &peer_id)
                .is_ok()
        );
        assert!(
            parse_inbound_file_request(&format!("{node_id}_COMPANYPROOF_TEST_TEST"), &peer_id)
                .is_ok()
        );
    }

    #[test]
    fn parse_inbound_file_request_content_bill() {
        let keys = BcrKeys::new();
        let peer_id = keys.get_libp2p_keys().unwrap().public().to_peer_id();
        let node_id = keys.get_public_key();
        assert_eq!(
            parse_inbound_file_request(&format!("{node_id}_BILL_TEST"), &peer_id).unwrap(),
            ParsedInboundFileRequest::Bill(BillFileRequest {
                bill_id: "TEST".to_string()
            })
        );
    }

    #[test]
    fn file_request_parse_inbound_file_request_bill() {
        let keys = BcrKeys::new();
        let peer_id = keys.get_libp2p_keys().unwrap().public().to_peer_id();
        let node_id = keys.get_public_key();
        assert_eq!(
            parse_inbound_file_request(&file_request_for_bill(&node_id, "TEST"), &peer_id).unwrap(),
            ParsedInboundFileRequest::Bill(BillFileRequest {
                bill_id: "TEST".to_string()
            })
        );
    }

    #[test]
    fn parse_inbound_file_request_content_key() {
        let keys = BcrKeys::new();
        let peer_id = keys.get_libp2p_keys().unwrap().public().to_peer_id();
        let node_id = keys.get_public_key();
        assert_eq!(
            parse_inbound_file_request(&format!("{node_id}_KEY_TEST"), &peer_id).unwrap(),
            ParsedInboundFileRequest::BillKeys(BillKeysFileRequest {
                node_id,
                bill_id: "TEST".to_string()
            })
        );
    }

    #[test]
    fn file_request_parse_inbound_file_request_content_key() {
        let keys = BcrKeys::new();
        let peer_id = keys.get_libp2p_keys().unwrap().public().to_peer_id();
        let node_id = keys.get_public_key();
        assert_eq!(
            parse_inbound_file_request(&file_request_for_bill_keys(&node_id, "TEST"), &peer_id)
                .unwrap(),
            ParsedInboundFileRequest::BillKeys(BillKeysFileRequest {
                node_id,
                bill_id: "TEST".to_string()
            })
        );
    }

    #[test]
    fn parse_inbound_file_request_attachment_length() {
        let keys = BcrKeys::new();
        let peer_id = keys.get_libp2p_keys().unwrap().public().to_peer_id();
        let node_id = keys.get_public_key();
        assert!(parse_inbound_file_request(&format!("{node_id}_BILLATT_TEST"), &peer_id).is_err(),);
    }

    #[test]
    fn parse_inbound_file_request_content_attachment() {
        let keys = BcrKeys::new();
        let peer_id = keys.get_libp2p_keys().unwrap().public().to_peer_id();
        let node_id = keys.get_public_key();
        assert_eq!(
            parse_inbound_file_request(&format!("{node_id}_BILLATT_TEST_FILE"), &peer_id).unwrap(),
            ParsedInboundFileRequest::BillAttachment(BillAttachmentFileRequest {
                node_id,
                bill_id: "TEST".to_string(),
                file_name: "FILE".to_string(),
            })
        );
    }

    #[test]
    fn file_request_parse_inbound_file_request_content_attachment() {
        let keys = BcrKeys::new();
        let peer_id = keys.get_libp2p_keys().unwrap().public().to_peer_id();
        let node_id = keys.get_public_key();
        assert_eq!(
            parse_inbound_file_request(
                &file_request_for_bill_attachment(&node_id, "TEST", "FILE"),
                &peer_id
            )
            .unwrap(),
            ParsedInboundFileRequest::BillAttachment(BillAttachmentFileRequest {
                node_id,
                bill_id: "TEST".to_string(),
                file_name: "FILE".to_string(),
            })
        );
    }

    #[test]
    fn parse_inbound_file_request_content_company_data() {
        let keys = BcrKeys::new();
        let peer_id = keys.get_libp2p_keys().unwrap().public().to_peer_id();
        let node_id = keys.get_public_key();
        assert_eq!(
            parse_inbound_file_request(&format!("{node_id}_COMPANY_TEST"), &peer_id).unwrap(),
            ParsedInboundFileRequest::CompanyData(CompanyDataRequest {
                node_id,
                company_id: "TEST".to_string(),
            })
        );
    }

    #[test]
    fn file_request_parse_inbound_file_request_content_company_data() {
        let keys = BcrKeys::new();
        let peer_id = keys.get_libp2p_keys().unwrap().public().to_peer_id();
        let node_id = keys.get_public_key();
        assert_eq!(
            parse_inbound_file_request(&file_request_for_company_data(&node_id, "TEST"), &peer_id)
                .unwrap(),
            ParsedInboundFileRequest::CompanyData(CompanyDataRequest {
                node_id,
                company_id: "TEST".to_string(),
            })
        );
    }

    #[test]
    fn parse_inbound_file_request_content_company_keys() {
        let keys = BcrKeys::new();
        let peer_id = keys.get_libp2p_keys().unwrap().public().to_peer_id();
        let node_id = keys.get_public_key();
        assert_eq!(
            parse_inbound_file_request(&format!("{node_id}_COMPANYKEY_TEST"), &peer_id).unwrap(),
            ParsedInboundFileRequest::CompanyKeys(CompanyKeysRequest {
                node_id,
                company_id: "TEST".to_string(),
            })
        );
    }

    #[test]
    fn file_request_parse_inbound_file_request_content_company_keys() {
        let keys = BcrKeys::new();
        let peer_id = keys.get_libp2p_keys().unwrap().public().to_peer_id();
        let node_id = keys.get_public_key();
        assert_eq!(
            parse_inbound_file_request(&file_request_for_company_keys(&node_id, "TEST"), &peer_id)
                .unwrap(),
            ParsedInboundFileRequest::CompanyKeys(CompanyKeysRequest {
                node_id,
                company_id: "TEST".to_string(),
            })
        );
    }

    #[test]
    fn parse_inbound_file_request_content_company_logo() {
        let keys = BcrKeys::new();
        let peer_id = keys.get_libp2p_keys().unwrap().public().to_peer_id();
        let node_id = keys.get_public_key();
        assert_eq!(
            parse_inbound_file_request(&format!("{node_id}_COMPANYLOGO_TEST_TEST"), &peer_id)
                .unwrap(),
            ParsedInboundFileRequest::CompanyLogo(CompanyLogoRequest {
                node_id,
                company_id: "TEST".to_string(),
                file_name: "TEST".to_string(),
            })
        );
    }

    #[test]
    fn file_request_parse_inbound_file_request_content_company_logo() {
        let keys = BcrKeys::new();
        let peer_id = keys.get_libp2p_keys().unwrap().public().to_peer_id();
        let node_id = keys.get_public_key();
        assert_eq!(
            parse_inbound_file_request(
                &file_request_for_company_logo(&node_id, "TEST", "TEST"),
                &peer_id
            )
            .unwrap(),
            ParsedInboundFileRequest::CompanyLogo(CompanyLogoRequest {
                node_id,
                company_id: "TEST".to_string(),
                file_name: "TEST".to_string(),
            })
        );
    }

    #[test]
    fn parse_inbound_file_request_content_company_proof() {
        let keys = BcrKeys::new();
        let peer_id = keys.get_libp2p_keys().unwrap().public().to_peer_id();
        let node_id = keys.get_public_key();
        assert_eq!(
            parse_inbound_file_request(&format!("{node_id}_COMPANYPROOF_TEST_TEST"), &peer_id)
                .unwrap(),
            ParsedInboundFileRequest::CompanyProof(CompanyProofRequest {
                node_id,
                company_id: "TEST".to_string(),
                file_name: "TEST".to_string(),
            })
        );
    }

    #[test]
    fn file_request_parse_inbound_file_request_content_company_proof() {
        let keys = BcrKeys::new();
        let peer_id = keys.get_libp2p_keys().unwrap().public().to_peer_id();
        let node_id = keys.get_public_key();
        assert_eq!(
            parse_inbound_file_request(
                &file_request_for_company_proof(&node_id, "TEST", "TEST"),
                &peer_id
            )
            .unwrap(),
            ParsedInboundFileRequest::CompanyProof(CompanyProofRequest {
                node_id,
                company_id: "TEST".to_string(),
                file_name: "TEST".to_string(),
            })
        );
    }
}
