use bill::LightBitcreditBillResult;
use borsh_derive::{BorshDeserialize, BorshSerialize};
use company::Company;
use contact::Contact;
use serde::{Deserialize, Serialize};
use std::fmt;

pub mod bill;
pub mod company;
pub mod contact;
pub mod identity;
pub mod notification;

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct PostalAddress {
    pub country: String,
    pub city: String,
    pub zip: Option<String>,
    pub address: String,
}

impl fmt::Display for PostalAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.zip {
            Some(ref zip) => {
                write!(
                    f,
                    "{}, {} {}, {}",
                    self.address, zip, self.city, self.country
                )
            }
            None => {
                write!(f, "{}, {}, {}", self.address, self.city, self.country)
            }
        }
    }
}

impl PostalAddress {
    #[cfg(test)]
    pub fn new_empty() -> Self {
        Self {
            country: "".to_string(),
            city: "".to_string(),
            zip: None,
            address: "".to_string(),
        }
    }
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct OptionalPostalAddress {
    pub country: Option<String>,
    pub city: Option<String>,
    pub zip: Option<String>,
    pub address: Option<String>,
}

impl OptionalPostalAddress {
    pub fn is_fully_set(&self) -> bool {
        self.country.is_some() && self.city.is_some() && self.address.is_some()
    }

    pub fn to_full_postal_address(&self) -> Option<PostalAddress> {
        if self.is_fully_set() {
            return Some(PostalAddress {
                country: self.country.clone().expect("checked above"),
                city: self.city.clone().expect("checked above"),
                zip: self.zip.clone(),
                address: self.address.clone().expect("checked above"),
            });
        }
        None
    }

    #[cfg(test)]
    pub fn new_empty() -> Self {
        Self {
            country: None,
            city: None,
            zip: None,
            address: None,
        }
    }
}

#[derive(Debug)]
pub struct GeneralSearchResult {
    pub bills: Vec<LightBitcreditBillResult>,
    pub contacts: Vec<Contact>,
    pub companies: Vec<Company>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum GeneralSearchFilterItemType {
    Company,
    Bill,
    Contact,
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct File {
    pub name: String,
    pub hash: String,
}

#[derive(Debug)]
pub struct UploadFilesResult {
    pub file_upload_id: String,
}
