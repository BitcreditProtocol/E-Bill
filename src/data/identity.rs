use super::{File, OptionalPostalAddress};
use crate::{blockchain::bill::block::BillSignatoryBlockData, util::BcrKeys};
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
pub enum IdentityType {
    Person = 0,
    Company = 1,
}

#[derive(Clone, Debug)]
pub struct IdentityWithAll {
    pub identity: Identity,
    pub key_pair: BcrKeys,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Identity {
    pub node_id: String,
    pub name: String,
    pub email: String,
    pub postal_address: OptionalPostalAddress,
    pub date_of_birth: Option<String>,
    pub country_of_birth: Option<String>,
    pub city_of_birth: Option<String>,
    pub identification_number: Option<String>,
    pub nostr_relay: Option<String>,
    pub profile_picture_file: Option<File>,
    pub identity_document_file: Option<File>,
}

impl Identity {
    #[cfg(test)]
    pub fn new_empty() -> Self {
        Self {
            node_id: "".to_string(),
            name: "".to_string(),
            email: "".to_string(),
            postal_address: OptionalPostalAddress::new_empty(),
            date_of_birth: None,
            country_of_birth: None,
            city_of_birth: None,
            identification_number: None,
            nostr_relay: None,
            profile_picture_file: None,
            identity_document_file: None,
        }
    }

    pub fn get_nostr_name(&self) -> String {
        self.name.clone()
    }
}

impl From<Identity> for BillSignatoryBlockData {
    fn from(value: Identity) -> Self {
        Self {
            name: value.name,
            node_id: value.node_id,
        }
    }
}
