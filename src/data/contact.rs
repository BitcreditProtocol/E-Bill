use super::{company::Company, identity::Identity, File, PostalAddress};
use crate::blockchain::bill::block::BillIdentityBlockData;
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
)]
#[borsh(use_discriminant = true)]
pub enum ContactType {
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

#[derive(BorshSerialize, BorshDeserialize, Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct IdentityPublicData {
    /// The type of identity (0 = person, 1 = company)
    #[serde(rename = "type")]
    pub t: ContactType,
    /// The P2P node id of the identity
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LightIdentityPublicData {
    #[serde(rename = "type")]
    pub t: ContactType,
    pub name: String,
    pub node_id: String,
}

impl From<IdentityPublicData> for LightIdentityPublicData {
    fn from(value: IdentityPublicData) -> Self {
        Self {
            t: value.t,
            name: value.name,
            node_id: value.node_id,
        }
    }
}

impl From<BillIdentityBlockData> for LightIdentityPublicData {
    fn from(value: BillIdentityBlockData) -> Self {
        Self {
            t: value.t,
            name: value.name,
            node_id: value.node_id,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LightIdentityPublicDataWithAddress {
    #[serde(rename = "type")]
    pub t: ContactType,
    pub name: String,
    pub node_id: String,
    #[serde(flatten)]
    pub postal_address: PostalAddress,
}

impl From<IdentityPublicData> for LightIdentityPublicDataWithAddress {
    fn from(value: IdentityPublicData) -> Self {
        Self {
            t: value.t,
            name: value.name,
            node_id: value.node_id,
            postal_address: value.postal_address,
        }
    }
}

impl From<BillIdentityBlockData> for LightIdentityPublicDataWithAddress {
    fn from(value: BillIdentityBlockData) -> Self {
        Self {
            t: value.t,
            name: value.name,
            node_id: value.node_id,
            postal_address: value.postal_address,
        }
    }
}

impl From<Contact> for IdentityPublicData {
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

impl From<Company> for IdentityPublicData {
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

impl IdentityPublicData {
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

    #[cfg(test)]
    pub fn new_empty() -> Self {
        Self {
            t: ContactType::Person,
            node_id: "".to_string(),
            name: "".to_string(),
            postal_address: PostalAddress::new_empty(),
            email: None,
            nostr_relay: None,
        }
    }

    #[cfg(test)]
    pub fn new_only_node_id(node_id: String) -> Self {
        Self {
            t: ContactType::Person,
            node_id,
            name: "".to_string(),
            postal_address: PostalAddress::new_empty(),
            email: None,
            nostr_relay: None,
        }
    }
}
