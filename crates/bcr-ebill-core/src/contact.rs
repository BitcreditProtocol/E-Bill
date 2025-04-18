use crate::blockchain::bill::block::NodeId;

use super::{File, PostalAddress, company::Company, identity::Identity};
use borsh_derive::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

#[repr(u8)]
#[derive(
    Debug,
    Clone,
    serde_repr::Serialize_repr,
    serde_repr::Deserialize_repr,
    PartialEq,
    Eq,
    BorshSerialize,
    BorshDeserialize,
    Default,
)]
#[borsh(use_discriminant = true)]
pub enum ContactType {
    #[default]
    Person = 0,
    Company = 1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    #[serde(rename = "type")]
    pub t: ContactType,
    pub node_id: String,
    pub name: String,
    pub email: String,
    #[serde(flatten)]
    pub postal_address: PostalAddress,
    pub date_of_birth_or_registration: Option<String>,
    pub country_of_birth_or_registration: Option<String>,
    pub city_of_birth_or_registration: Option<String>,
    pub identification_number: Option<String>,
    pub avatar_file: Option<File>,
    pub proof_document_file: Option<File>,
    pub nostr_relays: Vec<String>,
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum BillParticipant {
    Anonymous(BillAnonymousParticipant),
    Identified(BillIdentifiedParticipant),
}

impl BillParticipant {
    pub fn postal_address(&self) -> Option<PostalAddress> {
        match self {
            BillParticipant::Identified(data) => Some(data.postal_address.clone()),
            BillParticipant::Anonymous(_) => None,
        }
    }

    pub fn name(&self) -> Option<String> {
        match self {
            BillParticipant::Identified(data) => Some(data.name.to_owned()),
            BillParticipant::Anonymous(_) => None,
        }
    }

    pub fn email(&self) -> Option<String> {
        match self {
            BillParticipant::Identified(data) => data.email.to_owned(),
            BillParticipant::Anonymous(data) => data.email.to_owned(),
        }
    }

    pub fn nostr_relay(&self) -> Option<String> {
        match self {
            BillParticipant::Identified(data) => data.nostr_relay.to_owned(),
            BillParticipant::Anonymous(data) => data.nostr_relay.to_owned(),
        }
    }
}

impl NodeId for BillParticipant {
    fn node_id(&self) -> String {
        match self {
            BillParticipant::Identified(data) => data.node_id.clone(),
            BillParticipant::Anonymous(data) => data.node_id.clone(),
        }
    }
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct BillAnonymousParticipant {
    /// The node id of the participant
    pub node_id: String,
    /// email address of the participant
    pub email: Option<String>,
    /// The preferred Nostr relay to deliver Nostr messages to
    pub nostr_relay: Option<String>,
}

#[derive(
    BorshSerialize, BorshDeserialize, Debug, Serialize, Deserialize, Clone, Eq, PartialEq, Default,
)]
pub struct BillIdentifiedParticipant {
    /// The type of identity (0 = person, 1 = company)
    #[serde(rename = "type")]
    pub t: ContactType,
    /// The node id of the identity
    pub node_id: String,
    /// The name of the identity
    pub name: String,
    /// Full postal address of the identity
    #[serde(flatten)]
    pub postal_address: PostalAddress,
    /// email address of the identity
    pub email: Option<String>,
    /// The preferred Nostr relay to deliver Nostr messages to
    pub nostr_relay: Option<String>,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Serialize, Deserialize, Clone)]
pub enum LightBillParticipant {
    Anonymous(LightBillAnonymousParticipant),
    Identified(LightBillIdentifiedParticipant),
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Serialize, Deserialize, Clone, Default)]
pub struct LightBillAnonymousParticipant {
    pub node_id: String,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Serialize, Deserialize, Clone, Default)]
pub struct LightBillIdentifiedParticipant {
    #[serde(rename = "type")]
    pub t: ContactType,
    pub name: String,
    pub node_id: String,
}

impl From<BillParticipant> for LightBillParticipant {
    fn from(value: BillParticipant) -> Self {
        match value {
            BillParticipant::Identified(data) => LightBillParticipant::Identified(data.into()),
            BillParticipant::Anonymous(data) => LightBillParticipant::Anonymous(data.into()),
        }
    }
}

impl From<BillIdentifiedParticipant> for LightBillIdentifiedParticipant {
    fn from(value: BillIdentifiedParticipant) -> Self {
        Self {
            t: value.t,
            name: value.name,
            node_id: value.node_id,
        }
    }
}

impl From<BillAnonymousParticipant> for LightBillAnonymousParticipant {
    fn from(value: BillAnonymousParticipant) -> Self {
        Self {
            node_id: value.node_id,
        }
    }
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Serialize, Deserialize, Clone, Default)]
pub struct LightBillIdentifiedParticipantWithAddress {
    #[serde(rename = "type")]
    pub t: ContactType,
    pub name: String,
    pub node_id: String,
    #[serde(flatten)]
    pub postal_address: PostalAddress,
}

impl From<BillIdentifiedParticipant> for LightBillIdentifiedParticipantWithAddress {
    fn from(value: BillIdentifiedParticipant) -> Self {
        Self {
            t: value.t,
            name: value.name,
            node_id: value.node_id,
            postal_address: value.postal_address,
        }
    }
}

impl From<Contact> for BillIdentifiedParticipant {
    fn from(value: Contact) -> Self {
        Self {
            t: value.t,
            node_id: value.node_id.clone(),
            name: value.name,
            postal_address: value.postal_address,
            email: Some(value.email),
            nostr_relay: value.nostr_relays.first().cloned(),
        }
    }
}

impl From<Company> for BillIdentifiedParticipant {
    fn from(value: Company) -> Self {
        Self {
            t: ContactType::Company,
            node_id: value.id.clone(),
            name: value.name,
            postal_address: value.postal_address,
            email: Some(value.email),
            nostr_relay: None,
        }
    }
}

impl BillIdentifiedParticipant {
    pub fn new(identity: Identity) -> Option<Self> {
        match identity.postal_address.to_full_postal_address() {
            Some(postal_address) => Some(Self {
                t: ContactType::Person,
                node_id: identity.node_id,
                name: identity.name,
                postal_address,
                email: Some(identity.email),
                nostr_relay: identity.nostr_relay,
            }),
            None => None,
        }
    }
}
