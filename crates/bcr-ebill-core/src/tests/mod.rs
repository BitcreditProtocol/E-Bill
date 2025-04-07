#[cfg(test)]
#[allow(clippy::module_inception)]
pub mod tests {
    use crate::Validate;
    use crate::{
        Field, OptionalPostalAddress, PostalAddress, ValidationError,
        bill::{BillKeys, BitcreditBill},
        contact::{ContactType, IdentityPublicData},
        identity::Identity,
    };
    use rstest::rstest;

    pub fn valid_address() -> PostalAddress {
        PostalAddress {
            country: "AT".into(),
            city: "Vienna".into(),
            zip: Some("1010".into()),
            address: "Kärntner Straße 1".into(),
        }
    }

    #[rstest]
    #[case::empty_country(
        PostalAddress { country: "".into(), ..valid_address() },
        ValidationError::FieldEmpty(Field::Country)
    )]
    #[case::blank_country(
        PostalAddress { country: "  ".into(), ..valid_address() },
        ValidationError::FieldEmpty(Field::Country)
    )]
    #[case::blank_city(
        PostalAddress { city: "  ".into(), ..valid_address() },
        ValidationError::FieldEmpty(Field::City)
    )]
    #[case::empty_city(
        PostalAddress { city: "".into(), ..valid_address() },
        ValidationError::FieldEmpty(Field::City)
    )]
    #[case::blank_zip(
        PostalAddress { zip: Some("   ".into()), ..valid_address() },
        ValidationError::FieldEmpty(Field::Zip)
    )]
    #[case::empty_zip(
        PostalAddress { zip: Some("".into()), ..valid_address() },
        ValidationError::FieldEmpty(Field::Zip)
    )]
    #[case::empty_address(
        PostalAddress { address: "".into(), ..valid_address() },
        ValidationError::FieldEmpty(Field::Address)
    )]
    #[case::blank_address(
        PostalAddress { address: "  ".into(), ..valid_address() },
        ValidationError::FieldEmpty(Field::Address)
    )]
    fn test_invalid_address_cases(
        #[case] address: PostalAddress,
        #[case] expected_error: ValidationError,
    ) {
        assert_eq!(address.validate(), Err(expected_error));
    }

    #[rstest]
    #[case::baseline(valid_address())]
    #[case::spaced_country(PostalAddress {
        zip: Some("1020".into()),
        country: " AT ".into(),
        ..valid_address()
    })]
    #[case::no_zip(
        PostalAddress { zip: None, ..valid_address() },
    )]
    #[case::spaced_zip(PostalAddress {
        zip: Some(" Some Street 1 ".into()),
        ..valid_address()
    })]
    #[case::spaced_zip_address(PostalAddress {
        zip: Some(" 10101 ".into()),
        address: " 56 Rue de Paris ".into(),
        ..valid_address()
    })]
    fn test_valid_addresses(#[case] address: PostalAddress) {
        assert_eq!(address.validate(), Ok(()));
    }

    pub fn valid_optional_address() -> OptionalPostalAddress {
        OptionalPostalAddress {
            country: Some("AT".into()),
            city: Some("Vienna".into()),
            zip: Some("1010".into()),
            address: Some("Kärntner Straße 1".into()),
        }
    }

    #[test]
    fn test_valid_optional_address() {
        let address = valid_optional_address();
        assert_eq!(address.validate(), Ok(()));
        assert_eq!(
            OptionalPostalAddress {
                country: None,
                city: None,
                zip: None,
                address: None
            }
            .validate(),
            Ok(())
        );
    }

    #[rstest]
    #[case::blank_country(
        OptionalPostalAddress { country: Some("  ".into()), ..valid_optional_address() },
        ValidationError::FieldEmpty(Field::Country)
    )]
    #[case::empty_country(
        OptionalPostalAddress { country: Some("".into()), ..valid_optional_address() },
        ValidationError::FieldEmpty(Field::Country)
    )]
    #[case::blank_city(
        OptionalPostalAddress { city: Some("\n\t".into()), ..valid_optional_address() },
        ValidationError::FieldEmpty(Field::City)
    )]
    #[case::empty_city(
        OptionalPostalAddress { city: Some("".into()), ..valid_optional_address() },
        ValidationError::FieldEmpty(Field::City)
    )]
    #[case::empty_zip(
        OptionalPostalAddress { zip: Some("".into()), ..valid_optional_address() },
        ValidationError::FieldEmpty(Field::Zip)
    )]
    #[case::blank_zip(
        OptionalPostalAddress { zip: Some("  ".into()), ..valid_optional_address() },
        ValidationError::FieldEmpty(Field::Zip)
    )]
    #[case::blank_address(
        OptionalPostalAddress { address: Some("    ".into()), ..valid_optional_address() },
        ValidationError::FieldEmpty(Field::Address)
    )]
    #[case::empty_address(
        OptionalPostalAddress { address: Some("".into()), ..valid_optional_address() },
        ValidationError::FieldEmpty(Field::Address)
    )]
    fn test_optional_address(
        #[case] address: OptionalPostalAddress,
        #[case] expected_error: ValidationError,
    ) {
        assert_eq!(address.validate(), Err(expected_error));
    }

    pub fn empty_identity() -> Identity {
        Identity {
            node_id: "".to_string(),
            name: "some name".to_string(),
            email: "some@example.com".to_string(),
            postal_address: valid_optional_address(),
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
            postal_address: valid_address(),
            email: None,
            nostr_relay: None,
        }
    }

    pub fn identity_public_data_only_node_id(node_id: String) -> IdentityPublicData {
        IdentityPublicData {
            t: ContactType::Person,
            node_id,
            name: "some name".to_string(),
            postal_address: valid_address(),
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
