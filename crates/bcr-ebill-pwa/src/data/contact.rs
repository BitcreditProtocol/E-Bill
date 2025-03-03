use bcr_ebill_api::{
    data::contact::{Contact, ContactType},
    service::Error,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use super::{FileWeb, FromWeb, IntoWeb, OptionalPostalAddressWeb, PostalAddressWeb};

#[wasm_bindgen]
#[derive(Debug, Serialize)]
pub struct ContactsResponse {
    #[wasm_bindgen(getter_with_clone)]
    pub contacts: Vec<ContactWeb>,
}

#[wasm_bindgen]
impl ContactsResponse {
    #[wasm_bindgen(constructor)]
    pub fn new(contacts: Vec<ContactWeb>) -> Self {
        Self { contacts }
    }
}

#[wasm_bindgen]
#[derive(Debug, Serialize, Deserialize)]
pub struct NewContactPayload {
    pub t: u64,
    #[wasm_bindgen(getter_with_clone)]
    pub node_id: String,
    #[wasm_bindgen(getter_with_clone)]
    pub name: String,
    #[wasm_bindgen(getter_with_clone)]
    pub email: String,
    #[wasm_bindgen(getter_with_clone)]
    pub postal_address: PostalAddressWeb,
    #[wasm_bindgen(getter_with_clone)]
    pub date_of_birth_or_registration: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub country_of_birth_or_registration: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub city_of_birth_or_registration: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub identification_number: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub avatar_file_upload_id: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub proof_document_file_upload_id: Option<String>,
}

#[wasm_bindgen]
impl NewContactPayload {
    #[wasm_bindgen(constructor)]
    pub fn new(
        t: u64,
        node_id: String,
        name: String,
        email: String,
        postal_address: PostalAddressWeb,
        date_of_birth_or_registration: Option<String>,
        country_of_birth_or_registration: Option<String>,
        city_of_birth_or_registration: Option<String>,
        identification_number: Option<String>,
        avatar_file_upload_id: Option<String>,
        proof_document_file_upload_id: Option<String>,
    ) -> Self {
        Self {
            t,
            node_id,
            name,
            email,
            postal_address,
            date_of_birth_or_registration,
            country_of_birth_or_registration,
            city_of_birth_or_registration,
            identification_number,
            avatar_file_upload_id,
            proof_document_file_upload_id,
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Serialize, Deserialize)]
pub struct EditContactPayload {
    #[wasm_bindgen(getter_with_clone)]
    pub node_id: String,
    #[wasm_bindgen(getter_with_clone)]
    pub name: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub email: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub postal_address: OptionalPostalAddressWeb,
    #[wasm_bindgen(getter_with_clone)]
    pub date_of_birth_or_registration: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub country_of_birth_or_registration: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub city_of_birth_or_registration: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub identification_number: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub avatar_file_upload_id: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub proof_document_file_upload_id: Option<String>,
}

#[wasm_bindgen]
impl EditContactPayload {
    #[wasm_bindgen(constructor)]
    pub fn new(
        node_id: String,
        name: Option<String>,
        email: Option<String>,
        postal_address: OptionalPostalAddressWeb,
        date_of_birth_or_registration: Option<String>,
        country_of_birth_or_registration: Option<String>,
        city_of_birth_or_registration: Option<String>,
        identification_number: Option<String>,
        avatar_file_upload_id: Option<String>,
        proof_document_file_upload_id: Option<String>,
    ) -> Self {
        Self {
            node_id,
            name,
            email,
            postal_address,
            date_of_birth_or_registration,
            country_of_birth_or_registration,
            city_of_birth_or_registration,
            identification_number,
            avatar_file_upload_id,
            proof_document_file_upload_id,
        }
    }
}

#[wasm_bindgen]
#[repr(u8)]
#[derive(
    Debug, Copy, Clone, serde_repr::Serialize_repr, serde_repr::Deserialize_repr, PartialEq, Eq,
)]
pub enum ContactTypeWeb {
    Person = 0,
    Company = 1,
}

impl TryFrom<u64> for ContactTypeWeb {
    type Error = Error;

    fn try_from(value: u64) -> std::result::Result<Self, Self::Error> {
        match value {
            0 => Ok(ContactTypeWeb::Person),
            1 => Ok(ContactTypeWeb::Company),
            _ => Err(Error::Validation(format!(
                "Invalid contact type found: {value}"
            ))),
        }
    }
}

impl IntoWeb<ContactTypeWeb> for ContactType {
    fn into_web(self) -> ContactTypeWeb {
        match self {
            ContactType::Person => ContactTypeWeb::Person,
            ContactType::Company => ContactTypeWeb::Company,
        }
    }
}

impl FromWeb<ContactTypeWeb> for ContactType {
    fn from_web(value: ContactTypeWeb) -> Self {
        match value {
            ContactTypeWeb::Person => ContactType::Person,
            ContactTypeWeb::Company => ContactType::Company,
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactWeb {
    pub t: ContactTypeWeb,
    #[wasm_bindgen(getter_with_clone)]
    pub node_id: String,
    #[wasm_bindgen(getter_with_clone)]
    pub name: String,
    #[wasm_bindgen(getter_with_clone)]
    pub email: String,
    #[wasm_bindgen(getter_with_clone)]
    pub postal_address: PostalAddressWeb,
    #[wasm_bindgen(getter_with_clone)]
    pub date_of_birth_or_registration: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub country_of_birth_or_registration: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub city_of_birth_or_registration: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub identification_number: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub avatar_file: Option<FileWeb>,
    #[wasm_bindgen(getter_with_clone)]
    pub proof_document_file: Option<FileWeb>,
    #[wasm_bindgen(getter_with_clone)]
    pub nostr_relays: Vec<String>,
}

#[wasm_bindgen]
impl ContactWeb {
    #[wasm_bindgen(constructor)]
    pub fn new(
        t: ContactTypeWeb,
        node_id: String,
        name: String,
        email: String,
        postal_address: PostalAddressWeb,
        date_of_birth_or_registration: Option<String>,
        country_of_birth_or_registration: Option<String>,
        city_of_birth_or_registration: Option<String>,
        identification_number: Option<String>,
        avatar_file: Option<FileWeb>,
        proof_document_file: Option<FileWeb>,
        nostr_relays: Vec<String>,
    ) -> Self {
        Self {
            t,
            node_id,
            name,
            email,
            postal_address,
            date_of_birth_or_registration,
            country_of_birth_or_registration,
            city_of_birth_or_registration,
            identification_number,
            avatar_file,
            proof_document_file,
            nostr_relays,
        }
    }
}

impl IntoWeb<ContactWeb> for Contact {
    fn into_web(self) -> ContactWeb {
        ContactWeb {
            t: self.t.into_web(),
            node_id: self.node_id,
            name: self.name,
            email: self.email,
            postal_address: self.postal_address.into_web(),
            date_of_birth_or_registration: self.date_of_birth_or_registration,
            country_of_birth_or_registration: self.country_of_birth_or_registration,
            city_of_birth_or_registration: self.city_of_birth_or_registration,
            identification_number: self.identification_number,
            avatar_file: self.avatar_file.map(|f| f.into_web()),
            proof_document_file: self.proof_document_file.map(|f| f.into_web()),
            nostr_relays: self.nostr_relays,
        }
    }
}
