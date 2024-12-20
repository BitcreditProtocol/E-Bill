use super::bill::BillOpCode;
use super::Result;
use super::{calculate_hash, Block, Blockchain};
use crate::service::company_service::{CompanyKeys, CompanyToReturn};
use crate::util::{self, crypto, rsa, BcrKeys};
use crate::web::data::File;
use borsh::to_vec;
use borsh_derive::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

#[derive(BorshSerialize, Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum CompanyOpCode {
    Create,
    Update,
    AddSignatory,
    RemoveSignatory,
    SignCompanyBill,
}

/// Structure for the block data of a company block
///
/// - `data` contains the actual data of the block, encrypted using the company's RSA pub key
/// - `key` is optional and if set, contains the company private keys encrypted by an identity RSA
///   pub key (e.g. for CreateCompany the creator's and AddSignatory the signatory's)
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct CompanyBlockData {
    data: String,
    key: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CompanyBlock {
    pub company_id: String,
    pub id: u64,
    pub hash: String,
    pub timestamp: i64,
    pub data: String,
    pub public_key: String,
    pub previous_hash: String,
    pub signature: String,
    pub op_code: CompanyOpCode,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct CompanyCreateBlockData {
    pub id: String,
    pub name: String,
    pub country_of_registration: String,
    pub city_of_registration: String,
    pub postal_address: String,
    pub email: String,
    pub registration_number: String,
    pub registration_date: String,
    pub proof_of_registration_file: Option<File>,
    pub logo_file: Option<File>,
    pub signatories: Vec<String>,
    pub public_key: String,
    pub rsa_public_key: String,
}

impl From<CompanyToReturn> for CompanyCreateBlockData {
    fn from(value: CompanyToReturn) -> Self {
        Self {
            id: value.id,
            name: value.name,
            country_of_registration: value.country_of_registration,
            city_of_registration: value.city_of_registration,
            postal_address: value.postal_address,
            email: value.email,
            registration_number: value.registration_number,
            registration_date: value.registration_date,
            proof_of_registration_file: value.proof_of_registration_file,
            logo_file: value.logo_file,
            signatories: value.signatories,
            public_key: value.public_key,
            rsa_public_key: value.rsa_public_key,
        }
    }
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct CompanyUpdateBlockData {
    pub name: Option<String>,
    pub email: Option<String>,
    pub postal_address: Option<String>,
    pub logo_file_upload_id: Option<String>,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct CompanySignCompanyBillBlockData {
    pub bill_id: String,
    pub block_id: u64,
    pub block_hash: String,
    pub operation: BillOpCode,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct CompanyAddSignatoryBlockData {
    pub signatory: String,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct CompanyRemoveSignatoryBlockData {
    pub signatory: String,
}

impl Block for CompanyBlock {
    type OpCode = CompanyOpCode;

    fn id(&self) -> u64 {
        self.id
    }

    fn timestamp(&self) -> i64 {
        self.timestamp
    }

    fn op_code(&self) -> &Self::OpCode {
        &self.op_code
    }

    fn hash(&self) -> &str {
        &self.hash
    }

    fn previous_hash(&self) -> &str {
        &self.previous_hash
    }

    fn data(&self) -> &str {
        &self.data
    }

    fn signature(&self) -> &str {
        &self.signature
    }

    fn public_key(&self) -> &str {
        &self.public_key
    }
}

impl CompanyBlock {
    fn new(
        company_id: String,
        id: u64,
        previous_hash: String,
        data: String,
        op_code: CompanyOpCode,
        _identity_keys: &BcrKeys,
        company_keys: &CompanyKeys,
        timestamp: i64,
    ) -> Result<Self> {
        // TODO: calculate aggregated public key from identity and company key
        let hash = calculate_hash(
            &id,
            &previous_hash,
            &data,
            &timestamp,
            &company_keys.public_key, // TODO: use aggregated key
            &op_code,
        )?;
        // TODO: use aggregated signature from identity and company key
        let signature = crypto::signature(&hash, &company_keys.private_key)?;

        Ok(Self {
            company_id,
            id,
            hash,
            timestamp,
            previous_hash,
            signature,
            public_key: company_keys.public_key.clone(), // TODO: use aggregated key
            data,
            op_code,
        })
    }

    pub fn create_block_for_create(
        company_id: String,
        id: u64,
        genesis_hash: String,
        company: &CompanyCreateBlockData,
        identity_keys: &BcrKeys,
        company_keys: &CompanyKeys,
        rsa_public_key_pem: &str,
        timestamp: i64,
    ) -> Result<Self> {
        let company_bytes = to_vec(company)?;
        // encrypt data using company rsa key
        let encrypted_data = util::base58_encode(&rsa::encrypt_bytes_with_public_key(
            &company_bytes,
            &company_keys.rsa_public_key,
        )?);

        let keys_bytes = to_vec(&company_keys)?;
        // encrypt company keys using creator's identity rsa key
        let encrypted_keys = util::base58_encode(&rsa::encrypt_bytes_with_public_key(
            &keys_bytes,
            rsa_public_key_pem,
        )?);

        let data = CompanyBlockData {
            data: encrypted_data,
            key: Some(encrypted_keys),
        };
        let serialized_and_hashed_data = util::base58_encode(&to_vec(&data)?);

        Self::new(
            company_id.to_owned(),
            id,
            genesis_hash,
            serialized_and_hashed_data,
            CompanyOpCode::Create,
            identity_keys,
            company_keys,
            timestamp,
        )
    }

    #[allow(dead_code)]
    pub fn create_block_for_update(
        company_id: String,
        previous_block: &Self,
        data: &CompanyUpdateBlockData,
        identity_keys: &BcrKeys,
        company_keys: &CompanyKeys,
        rsa_public_key_pem: &str,
        timestamp: i64,
    ) -> Result<Self> {
        let block = Self::encrypt_data_create_block_and_validate(
            company_id,
            previous_block,
            data,
            identity_keys,
            company_keys,
            rsa_public_key_pem,
            timestamp,
            CompanyOpCode::Update,
        )?;
        Ok(block)
    }

    #[allow(dead_code)]
    pub fn create_block_for_sign_company_bill(
        company_id: String,
        previous_block: &Self,
        data: &CompanySignCompanyBillBlockData,
        identity_keys: &BcrKeys,
        company_keys: &CompanyKeys,
        rsa_public_key_pem: &str,
        timestamp: i64,
    ) -> Result<Self> {
        let block = Self::encrypt_data_create_block_and_validate(
            company_id,
            previous_block,
            data,
            identity_keys,
            company_keys,
            rsa_public_key_pem,
            timestamp,
            CompanyOpCode::SignCompanyBill,
        )?;
        Ok(block)
    }

    #[allow(dead_code)]
    pub fn create_block_for_add_signatory(
        company_id: String,
        previous_block: &Self,
        data: &CompanyAddSignatoryBlockData,
        identity_keys: &BcrKeys,
        company_keys: &CompanyKeys,
        rsa_public_key_pem: &str, // the signatory's public rsa key
        timestamp: i64,
    ) -> Result<Self> {
        let block = Self::encrypt_data_create_block_and_validate(
            company_id,
            previous_block,
            data,
            identity_keys,
            company_keys,
            rsa_public_key_pem,
            timestamp,
            CompanyOpCode::AddSignatory,
        )?;
        Ok(block)
    }

    #[allow(dead_code)]
    pub fn create_block_for_remove_signatory(
        company_id: String,
        previous_block: &Self,
        data: &CompanyRemoveSignatoryBlockData,
        identity_keys: &BcrKeys,
        company_keys: &CompanyKeys,
        rsa_public_key_pem: &str,
        timestamp: i64,
    ) -> Result<Self> {
        let block = Self::encrypt_data_create_block_and_validate(
            company_id,
            previous_block,
            data,
            identity_keys,
            company_keys,
            rsa_public_key_pem,
            timestamp,
            CompanyOpCode::RemoveSignatory,
        )?;
        Ok(block)
    }

    #[allow(dead_code)]
    fn encrypt_data_create_block_and_validate<T: borsh::BorshSerialize>(
        company_id: String,
        previous_block: &Self,
        data: &T,
        identity_keys: &BcrKeys,
        company_keys: &CompanyKeys,
        rsa_public_key_pem: &str,
        timestamp: i64,
        op_code: CompanyOpCode,
    ) -> Result<Self> {
        let bytes = to_vec(&data)?;
        // encrypt data using the company rsa key
        let encrypted_data = util::base58_encode(&rsa::encrypt_bytes_with_public_key(
            &bytes,
            &company_keys.rsa_public_key,
        )?);

        let mut keys = None;

        // in case there are keys to encrypt, encrypt them using the receiver's identity rsa pub
        // key
        if op_code == CompanyOpCode::AddSignatory {
            let keys_bytes = to_vec(&company_keys)?;
            let encrypted_keys = util::base58_encode(&rsa::encrypt_bytes_with_public_key(
                &keys_bytes,
                rsa_public_key_pem,
            )?);
            keys = Some(encrypted_keys);
        }

        let data = CompanyBlockData {
            data: encrypted_data,
            key: keys,
        };
        let serialized_and_hashed_data = util::base58_encode(&to_vec(&data)?);

        let new_block = Self::new(
            company_id,
            previous_block.id + 1,
            previous_block.hash.clone(),
            serialized_and_hashed_data,
            op_code,
            identity_keys,
            company_keys,
            timestamp,
        )?;

        if !new_block.validate_with_previous(previous_block) {
            return Err(super::Error::BlockInvalid);
        }
        Ok(new_block)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CompanyBlockchain {
    blocks: Vec<CompanyBlock>,
}

impl Blockchain for CompanyBlockchain {
    type Block = CompanyBlock;

    fn blocks(&self) -> &Vec<Self::Block> {
        &self.blocks
    }

    fn blocks_mut(&mut self) -> &mut Vec<Self::Block> {
        &mut self.blocks
    }
}

impl CompanyBlockchain {
    /// Creates a new company chain
    pub fn new(
        company: &CompanyCreateBlockData,
        node_id: &str,
        identity_keys: &BcrKeys,
        company_keys: &CompanyKeys,
        rsa_public_key_pem: &str,
        timestamp: i64,
    ) -> Result<Self> {
        let genesis_hash = util::base58_encode(node_id.as_bytes());

        let first_block = CompanyBlock::create_block_for_create(
            company.id.clone(),
            1,
            genesis_hash,
            company,
            identity_keys,
            company_keys,
            rsa_public_key_pem,
            timestamp,
        )?;

        Ok(Self {
            blocks: vec![first_block],
        })
    }
}
