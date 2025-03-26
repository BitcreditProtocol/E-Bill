use super::{File, OptionalPostalAddress};
use crate::util::BcrKeys;
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
    pub fn get_nostr_name(&self) -> String {
        self.name.clone()
    }
}

#[derive(Clone, Debug)]
pub struct ActiveIdentityState {
    pub personal: String,
    pub company: Option<String>,
}
