use bcr_ebill_api::{
    data::{
        File, OptionalPostalAddress,
        identity::{Identity, IdentityType},
    },
    service::Result,
    util::BcrKeys,
};
use serde::{Deserialize, Serialize};

/// A structure describing the currently selected identity between the personal and multiple
/// possible company identities
#[derive(Clone, Debug)]
pub struct SwitchIdentityState {
    pub personal: String,
    pub company: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SwitchIdentity {
    #[serde(rename = "type")]
    pub t: Option<IdentityTypeWeb>,
    pub node_id: String,
}

#[repr(u8)]
#[derive(Debug, Clone, serde_repr::Serialize_repr, serde_repr::Deserialize_repr, PartialEq, Eq)]
pub enum IdentityTypeWeb {
    Person = 0,
    Company = 1,
}

impl IntoWeb<IdentityTypeWeb> for IdentityType {
    fn into_web(self) -> IdentityTypeWeb {
        match self {
            IdentityType::Person => IdentityTypeWeb::Person,
            IdentityType::Company => IdentityTypeWeb::Company,
        }
    }
}

pub trait IntoWeb<T> {
    fn into_web(self) -> T;
}

pub trait FromWeb<T> {
    fn from_web(value: T) -> Self;
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NewIdentityPayload {
    pub name: String,
    pub email: String,
    #[serde(flatten)]
    pub postal_address: OptionalPostalAddressWeb,
    pub date_of_birth: Option<String>,
    pub country_of_birth: Option<String>,
    pub city_of_birth: Option<String>,
    pub identification_number: Option<String>,
    pub profile_picture_file_upload_id: Option<String>,
    pub identity_document_file_upload_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionalPostalAddressWeb {
    pub country: Option<String>,
    pub city: Option<String>,
    pub zip: Option<String>,
    pub address: Option<String>,
}

impl FromWeb<OptionalPostalAddressWeb> for OptionalPostalAddress {
    fn from_web(value: OptionalPostalAddressWeb) -> Self {
        Self {
            country: value.country,
            city: value.city,
            zip: value.zip,
            address: value.address,
        }
    }
}

impl IntoWeb<OptionalPostalAddressWeb> for OptionalPostalAddress {
    fn into_web(self) -> OptionalPostalAddressWeb {
        OptionalPostalAddressWeb {
            country: self.country,
            city: self.city,
            zip: self.zip,
            address: self.address,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IdentityWeb {
    pub node_id: String,
    pub name: String,
    pub email: String,
    pub bitcoin_public_key: String,
    pub npub: String,
    #[serde(flatten)]
    pub postal_address: OptionalPostalAddressWeb,
    pub date_of_birth: Option<String>,
    pub country_of_birth: Option<String>,
    pub city_of_birth: Option<String>,
    pub identification_number: Option<String>,
    pub profile_picture_file: Option<FileWeb>,
    pub identity_document_file: Option<FileWeb>,
    pub nostr_relay: Option<String>,
}

impl IdentityWeb {
    pub fn from(identity: Identity, keys: BcrKeys) -> Result<Self> {
        Ok(Self {
            node_id: identity.node_id.clone(),
            name: identity.name,
            email: identity.email,
            bitcoin_public_key: identity.node_id.clone(),
            npub: keys.get_nostr_npub()?,
            postal_address: identity.postal_address.into_web(),
            date_of_birth: identity.date_of_birth,
            country_of_birth: identity.country_of_birth,
            city_of_birth: identity.city_of_birth,
            identification_number: identity.identification_number,
            profile_picture_file: identity.profile_picture_file.map(|f| f.into_web()),
            identity_document_file: identity.identity_document_file.map(|f| f.into_web()),
            nostr_relay: identity.nostr_relay,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileWeb {
    pub name: String,
    pub hash: String,
}

impl FromWeb<FileWeb> for File {
    fn from_web(value: FileWeb) -> Self {
        Self {
            name: value.name,
            hash: value.hash,
        }
    }
}

impl IntoWeb<FileWeb> for File {
    fn into_web(self) -> FileWeb {
        FileWeb {
            name: self.name,
            hash: self.hash,
        }
    }
}
