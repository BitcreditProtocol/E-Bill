use super::bill::BillOpCode;
use super::Result;
use super::{calculate_hash, Block, Blockchain};
use crate::service::company_service::{Company, CompanyKeys};
use crate::util::{self, crypto, rsa, BcrKeys};
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CompanyBlock {
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
    pub name: String,
    pub company_key: String, // TODO: encrypted private key of company
}

impl From<Company> for CompanyCreateBlockData {
    fn from(value: Company) -> Self {
        Self {
            name: value.name,
            company_key: "123".to_string(), // TODO: encrypted private key
        }
    }
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct CompanyUpdateBlockData {
    pub name: Option<String>,
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
    pub company_key: String, // TODO: encrypted private key of company
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
        id: u64,
        previous_hash: String,
        data: String,
        op_code: CompanyOpCode,
        identity_keys: &BcrKeys,
        company_keys: &CompanyKeys,
        timestamp: i64,
    ) -> Result<Self> {
        let hash = calculate_hash(
            &id,
            &previous_hash,
            &data,
            &timestamp,
            &identity_keys.get_public_key(),
            &op_code,
        )?;
        let signature = crypto::signature(&hash, &identity_keys.get_private_key_string())?;

        Ok(Self {
            id,
            hash,
            timestamp,
            previous_hash,
            signature,
            public_key: identity_keys.get_public_key(),
            data,
            op_code,
        })
    }

    pub fn create_block_for_create(
        id: u64,
        genesis_hash: String,
        company: &CompanyCreateBlockData,
        identity_keys: &BcrKeys,
        company_keys: &CompanyKeys,
        rsa_public_key_pem: &str,
        timestamp: i64,
    ) -> Result<Self> {
        let company_bytes = to_vec(company)?;

        let encrypted_data = util::base58_encode(&rsa::encrypt_bytes_with_public_key(
            &company_bytes,
            rsa_public_key_pem,
        )?);

        Self::new(
            id,
            genesis_hash,
            encrypted_data,
            CompanyOpCode::Create,
            identity_keys,
            company_keys,
            timestamp,
        )
    }

    pub fn create_block_for_update(
        previous_block: &Self,
        data: &CompanyUpdateBlockData,
        identity_keys: &BcrKeys,
        company_keys: &CompanyKeys,
        rsa_public_key_pem: &str,
        timestamp: i64,
    ) -> Result<Self> {
        let block = Self::encrypt_data_create_block_and_validate(
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

    pub fn create_block_for_sign_company_bill(
        previous_block: &Self,
        data: &CompanySignCompanyBillBlockData,
        identity_keys: &BcrKeys,
        company_keys: &CompanyKeys,
        rsa_public_key_pem: &str,
        timestamp: i64,
    ) -> Result<Self> {
        let block = Self::encrypt_data_create_block_and_validate(
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

    pub fn create_block_for_add_signatory(
        previous_block: &Self,
        data: &CompanyAddSignatoryBlockData,
        identity_keys: &BcrKeys,
        company_keys: &CompanyKeys,
        rsa_public_key_pem: &str,
        timestamp: i64,
    ) -> Result<Self> {
        let block = Self::encrypt_data_create_block_and_validate(
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

    pub fn create_block_for_remove_signatory(
        previous_block: &Self,
        data: &CompanyRemoveSignatoryBlockData,
        identity_keys: &BcrKeys,
        company_keys: &CompanyKeys,
        rsa_public_key_pem: &str,
        timestamp: i64,
    ) -> Result<Self> {
        let block = Self::encrypt_data_create_block_and_validate(
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

    fn encrypt_data_create_block_and_validate<T: borsh::BorshSerialize>(
        previous_block: &Self,
        data: &T,
        identity_keys: &BcrKeys,
        company_keys: &CompanyKeys,
        rsa_public_key_pem: &str,
        timestamp: i64,
        op_code: CompanyOpCode,
    ) -> Result<Self> {
        let bytes = to_vec(&data)?;

        let encrypted_data = util::base58_encode(&rsa::encrypt_bytes_with_public_key(
            &bytes,
            rsa_public_key_pem,
        )?);

        let new_block = Self::new(
            previous_block.id + 1,
            previous_block.hash.clone(),
            encrypted_data,
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
