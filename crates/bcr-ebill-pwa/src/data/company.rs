use bcr_ebill_api::data::{company::Company, contact::Contact};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use super::{
    FileWeb, IntoWeb, OptionalPostalAddressWeb, PostalAddressWeb, contact::ContactTypeWeb,
};

#[wasm_bindgen]
#[derive(Debug, Serialize)]
pub struct CompaniesResponse {
    #[wasm_bindgen(getter_with_clone)]
    pub companies: Vec<CompanyWeb>,
}

#[wasm_bindgen]
impl CompaniesResponse {
    #[wasm_bindgen(constructor)]
    pub fn new(companies: Vec<CompanyWeb>) -> Self {
        Self { companies }
    }
}

#[wasm_bindgen]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CompanyWeb {
    #[wasm_bindgen(getter_with_clone)]
    pub id: String,
    #[wasm_bindgen(getter_with_clone)]
    pub name: String,
    #[wasm_bindgen(getter_with_clone)]
    pub country_of_registration: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub city_of_registration: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub postal_address: PostalAddressWeb,
    #[wasm_bindgen(getter_with_clone)]
    pub email: String,
    #[wasm_bindgen(getter_with_clone)]
    pub registration_number: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub registration_date: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub proof_of_registration_file: Option<FileWeb>,
    #[wasm_bindgen(getter_with_clone)]
    pub logo_file: Option<FileWeb>,
    #[wasm_bindgen(getter_with_clone)]
    pub signatories: Vec<String>,
}

#[wasm_bindgen]
impl CompanyWeb {
    #[wasm_bindgen(constructor)]
    pub fn new(
        id: String,
        name: String,
        country_of_registration: Option<String>,
        city_of_registration: Option<String>,
        postal_address: PostalAddressWeb,
        email: String,
        registration_number: Option<String>,
        registration_date: Option<String>,
        proof_of_registration_file: Option<FileWeb>,
        logo_file: Option<FileWeb>,
        signatories: Vec<String>,
    ) -> Self {
        Self {
            id,
            name,
            country_of_registration,
            city_of_registration,
            postal_address,
            email,
            registration_number,
            registration_date,
            proof_of_registration_file,
            logo_file,
            signatories,
        }
    }
}

impl IntoWeb<CompanyWeb> for Company {
    fn into_web(self) -> CompanyWeb {
        CompanyWeb {
            id: self.id,
            name: self.name,
            country_of_registration: self.country_of_registration,
            city_of_registration: self.city_of_registration,
            postal_address: self.postal_address.into_web(),
            email: self.email,
            registration_number: self.registration_number,
            registration_date: self.registration_date,
            proof_of_registration_file: self.proof_of_registration_file.map(|f| f.into_web()),
            logo_file: self.logo_file.map(|f| f.into_web()),
            signatories: self.signatories,
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CreateCompanyPayload {
    #[wasm_bindgen(getter_with_clone)]
    pub name: String,
    #[wasm_bindgen(getter_with_clone)]
    pub country_of_registration: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub city_of_registration: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub postal_address: PostalAddressWeb,
    #[wasm_bindgen(getter_with_clone)]
    pub email: String,
    #[wasm_bindgen(getter_with_clone)]
    pub registration_number: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub registration_date: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub proof_of_registration_file_upload_id: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub logo_file_upload_id: Option<String>,
}

#[wasm_bindgen]
impl CreateCompanyPayload {
    #[wasm_bindgen(constructor)]
    pub fn new(
        name: String,
        country_of_registration: Option<String>,
        city_of_registration: Option<String>,
        postal_address: PostalAddressWeb,
        email: String,
        registration_number: Option<String>,
        registration_date: Option<String>,
        proof_of_registration_file_upload_id: Option<String>,
        logo_file_upload_id: Option<String>,
    ) -> Self {
        Self {
            name,
            country_of_registration,
            city_of_registration,
            postal_address,
            email,
            registration_number,
            registration_date,
            proof_of_registration_file_upload_id,
            logo_file_upload_id,
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EditCompanyPayload {
    #[wasm_bindgen(getter_with_clone)]
    pub id: String,
    #[wasm_bindgen(getter_with_clone)]
    pub name: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub email: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub postal_address: OptionalPostalAddressWeb,
    #[wasm_bindgen(getter_with_clone)]
    pub country_of_registration: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub city_of_registration: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub registration_number: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub registration_date: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub logo_file_upload_id: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub proof_of_registration_file_upload_id: Option<String>,
}

#[wasm_bindgen]
impl EditCompanyPayload {
    #[wasm_bindgen(constructor)]
    pub fn new(
        id: String,
        name: Option<String>,
        postal_address: OptionalPostalAddressWeb,
        email: Option<String>,
        country_of_registration: Option<String>,
        city_of_registration: Option<String>,
        registration_number: Option<String>,
        registration_date: Option<String>,
        proof_of_registration_file_upload_id: Option<String>,
        logo_file_upload_id: Option<String>,
    ) -> Self {
        Self {
            id,
            name,
            country_of_registration,
            email,
            city_of_registration,
            postal_address,
            registration_number,
            registration_date,
            proof_of_registration_file_upload_id,
            logo_file_upload_id,
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AddSignatoryPayload {
    #[wasm_bindgen(getter_with_clone)]
    pub id: String,
    #[wasm_bindgen(getter_with_clone)]
    pub signatory_node_id: String,
}

#[wasm_bindgen]
impl AddSignatoryPayload {
    #[wasm_bindgen(constructor)]
    pub fn new(id: String, signatory_node_id: String) -> Self {
        Self {
            id,
            signatory_node_id,
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RemoveSignatoryPayload {
    #[wasm_bindgen(getter_with_clone)]
    pub id: String,
    #[wasm_bindgen(getter_with_clone)]
    pub signatory_node_id: String,
}

#[wasm_bindgen]
impl RemoveSignatoryPayload {
    #[wasm_bindgen(constructor)]
    pub fn new(id: String, signatory_node_id: String) -> Self {
        Self {
            id,
            signatory_node_id,
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ListSignatoriesResponse {
    #[wasm_bindgen(getter_with_clone)]
    pub signatories: Vec<SignatoryResponse>,
}

#[wasm_bindgen]
impl ListSignatoriesResponse {
    #[wasm_bindgen(constructor)]
    pub fn new(signatories: Vec<SignatoryResponse>) -> Self {
        Self { signatories }
    }
}

#[wasm_bindgen]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SignatoryResponse {
    pub t: ContactTypeWeb,
    #[wasm_bindgen(getter_with_clone)]
    pub node_id: String,
    #[wasm_bindgen(getter_with_clone)]
    pub name: String,
    #[wasm_bindgen(getter_with_clone)]
    pub postal_address: PostalAddressWeb,
    #[wasm_bindgen(getter_with_clone)]
    pub avatar_file: Option<FileWeb>,
}

#[wasm_bindgen]
impl SignatoryResponse {
    #[wasm_bindgen(constructor)]
    pub fn new(
        t: ContactTypeWeb,
        node_id: String,
        name: String,
        postal_address: PostalAddressWeb,
        avatar_file: Option<FileWeb>,
    ) -> Self {
        Self {
            t,
            node_id,
            name,
            postal_address,
            avatar_file,
        }
    }
}

impl From<Contact> for SignatoryResponse {
    fn from(value: Contact) -> Self {
        Self {
            t: value.t.into_web(),
            node_id: value.node_id,
            name: value.name,
            postal_address: value.postal_address.into_web(),
            avatar_file: value.avatar_file.map(|f| f.into_web()),
        }
    }
}
