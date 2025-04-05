#[cfg(test)]
#[allow(clippy::module_inception)]
pub mod tests {
    use crate::{
        OptionalPostalAddress, PostalAddress,
        bill::{BillKeys, BitcreditBill},
        contact::{ContactType, IdentityPublicData},
        identity::Identity,
    };

    pub fn empty_address() -> PostalAddress {
        PostalAddress {
            country: "AT".to_string(),
            city: "Vienna".to_string(),
            zip: None,
            address: "Some address".to_string(),
        }
    }

    pub fn empty_optional_address() -> OptionalPostalAddress {
        OptionalPostalAddress {
            country: None,
            city: None,
            zip: None,
            address: None,
        }
    }

    pub fn empty_identity() -> Identity {
        Identity {
            node_id: "".to_string(),
            name: "some name".to_string(),
            email: "some@example.com".to_string(),
            postal_address: empty_optional_address(),
            date_of_birth: None,
            country_of_birth: None,
            city_of_birth: None,
            identification_number: None,
            nostr_relay: None,
            profile_picture_file: None,
            identity_document_file: None,
        }
    }

    pub fn empty_identity_public_data() -> IdentityPublicData {
        IdentityPublicData {
            t: ContactType::Person,
            node_id: "".to_string(),
            name: "some name".to_string(),
            postal_address: empty_address(),
            email: None,
            nostr_relay: None,
        }
    }

    pub fn identity_public_data_only_node_id(node_id: String) -> IdentityPublicData {
        IdentityPublicData {
            t: ContactType::Person,
            node_id,
            name: "some name".to_string(),
            postal_address: empty_address(),
            email: None,
            nostr_relay: None,
        }
    }

    pub fn empty_bitcredit_bill() -> BitcreditBill {
        BitcreditBill {
            id: TEST_BILL_ID.to_owned(),
            country_of_issuing: "AT".to_string(),
            city_of_issuing: "Vienna".to_string(),
            drawee: empty_identity_public_data(),
            drawer: empty_identity_public_data(),
            payee: empty_identity_public_data(),
            endorsee: None,
            currency: "sat".to_string(),
            sum: 500,
            maturity_date: "2099-11-12".to_string(),
            issue_date: "2099-08-12".to_string(),
            city_of_payment: "Vienna".to_string(),
            country_of_payment: "AT".to_string(),
            language: "DE".to_string(),
            files: vec![],
        }
    }

    pub fn get_bill_keys() -> BillKeys {
        BillKeys {
            private_key: TEST_PRIVATE_KEY_SECP.to_owned(),
            public_key: TEST_PUB_KEY_SECP.to_owned(),
        }
    }

    pub const TEST_PUB_KEY_SECP: &str =
        "02295fb5f4eeb2f21e01eaf3a2d9a3be10f39db870d28f02146130317973a40ac0";

    pub const TEST_BILL_ID: &str = "KmtMUia3ezhshD9EyzvpT62DUPLr66M5LESy6j8ErCtv1USUDtoTA8JkXnCCGEtZxp41aKne5wVcCjoaFbjDqD4aFk";

    pub const TEST_PRIVATE_KEY_SECP: &str =
        "d1ff7427912d3b81743d3b67ffa1e65df2156d3dab257316cbc8d0f35eeeabe9";

    pub const TEST_NODE_ID_SECP: &str =
        "03205b8dec12bc9e879f5b517aa32192a2550e88adcee3e54ec2c7294802568fef";

    pub const TEST_NODE_ID_SECP_AS_NPUB_HEX: &str =
        "205b8dec12bc9e879f5b517aa32192a2550e88adcee3e54ec2c7294802568fef";
}
