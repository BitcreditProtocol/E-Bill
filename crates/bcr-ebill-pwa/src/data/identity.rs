use bcr_ebill_api::{
    data::identity::{Identity, IdentityType},
    service::Result,
    util::BcrKeys,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use super::{FileWeb, IntoWeb, OptionalPostalAddressWeb};

#[wasm_bindgen]
#[derive(Debug, Serialize, Deserialize)]
pub struct SwitchIdentity {
    pub t: Option<IdentityTypeWeb>,
    #[wasm_bindgen(getter_with_clone)]
    pub node_id: String,
}

#[wasm_bindgen]
impl SwitchIdentity {
    #[wasm_bindgen(constructor)]
    pub fn new(t: Option<IdentityTypeWeb>, node_id: String) -> SwitchIdentity {
        SwitchIdentity { t, node_id }
    }
}

#[repr(u8)]
#[derive(
    Debug, Clone, Copy, serde_repr::Serialize_repr, serde_repr::Deserialize_repr, PartialEq, Eq,
)]
#[wasm_bindgen]
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

#[derive(Debug, Serialize, Deserialize)]
#[wasm_bindgen]
pub struct NewIdentityPayload {
    #[wasm_bindgen(getter_with_clone)]
    pub name: String,
    #[wasm_bindgen(getter_with_clone)]
    pub email: String,
    #[wasm_bindgen(getter_with_clone)]
    pub postal_address: OptionalPostalAddressWeb,
    #[wasm_bindgen(getter_with_clone)]
    pub date_of_birth: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub country_of_birth: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub city_of_birth: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub identification_number: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub profile_picture_file_upload_id: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub identity_document_file_upload_id: Option<String>,
}

#[wasm_bindgen]
impl NewIdentityPayload {
    #[wasm_bindgen(constructor)]
    pub fn new(
        name: String,
        email: String,
        postal_address: OptionalPostalAddressWeb,
        date_of_birth: Option<String>,
        country_of_birth: Option<String>,
        city_of_birth: Option<String>,
        identification_number: Option<String>,
        profile_picture_file_upload_id: Option<String>,
        identity_document_file_upload_id: Option<String>,
    ) -> Self {
        Self {
            name,
            email,
            postal_address,
            date_of_birth,
            country_of_birth,
            city_of_birth,
            identification_number,
            profile_picture_file_upload_id,
            identity_document_file_upload_id,
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Serialize, Deserialize)]
pub struct ChangeIdentityPayload {
    #[wasm_bindgen(getter_with_clone)]
    pub name: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub email: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub postal_address: OptionalPostalAddressWeb,
    #[wasm_bindgen(getter_with_clone)]
    pub date_of_birth: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub country_of_birth: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub city_of_birth: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub identification_number: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub profile_picture_file_upload_id: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub identity_document_file_upload_id: Option<String>,
}

#[wasm_bindgen]
impl ChangeIdentityPayload {
    #[wasm_bindgen(constructor)]
    pub fn new(
        name: Option<String>,
        email: Option<String>,
        postal_address: OptionalPostalAddressWeb,
        date_of_birth: Option<String>,
        country_of_birth: Option<String>,
        city_of_birth: Option<String>,
        identification_number: Option<String>,
        profile_picture_file_upload_id: Option<String>,
        identity_document_file_upload_id: Option<String>,
    ) -> Self {
        Self {
            name,
            email,
            postal_address,
            date_of_birth,
            country_of_birth,
            city_of_birth,
            identification_number,
            profile_picture_file_upload_id,
            identity_document_file_upload_id,
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Serialize, Deserialize)]
pub struct IdentityWeb {
    #[wasm_bindgen(getter_with_clone)]
    pub node_id: String,
    #[wasm_bindgen(getter_with_clone)]
    pub name: String,
    #[wasm_bindgen(getter_with_clone)]
    pub email: String,
    #[wasm_bindgen(getter_with_clone)]
    pub bitcoin_public_key: String,
    #[wasm_bindgen(getter_with_clone)]
    pub npub: String,
    #[wasm_bindgen(getter_with_clone)]
    pub postal_address: OptionalPostalAddressWeb,
    #[wasm_bindgen(getter_with_clone)]
    pub date_of_birth: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub country_of_birth: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub city_of_birth: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub identification_number: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub profile_picture_file: Option<FileWeb>,
    #[wasm_bindgen(getter_with_clone)]
    pub identity_document_file: Option<FileWeb>,
    #[wasm_bindgen(getter_with_clone)]
    pub nostr_relay: Option<String>,
}

#[wasm_bindgen]
impl IdentityWeb {
    #[wasm_bindgen(constructor)]
    pub fn new(
        node_id: String,
        name: String,
        email: String,
        bitcoin_public_key: String,
        npub: String,
        postal_address: OptionalPostalAddressWeb,
        date_of_birth: Option<String>,
        country_of_birth: Option<String>,
        city_of_birth: Option<String>,
        identification_number: Option<String>,
        profile_picture_file: Option<FileWeb>,
        identity_document_file: Option<FileWeb>,
        nostr_relay: Option<String>,
    ) -> Self {
        Self {
            node_id,
            name,
            email,
            bitcoin_public_key,
            npub,
            postal_address,
            date_of_birth,
            country_of_birth,
            city_of_birth,
            identification_number,
            profile_picture_file,
            identity_document_file,
            nostr_relay,
        }
    }
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

/// Response for a private key seeed backup
#[wasm_bindgen]
#[derive(Debug, Serialize, Deserialize)]
pub struct SeedPhrase {
    /// The seed phrase of the current private key
    #[wasm_bindgen(getter_with_clone)]
    pub seed_phrase: String,
}

#[wasm_bindgen]
impl SeedPhrase {
    pub fn new(seed_phrase: String) -> Self {
        Self { seed_phrase }
    }
}
