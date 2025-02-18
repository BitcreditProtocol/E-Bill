use super::super::{Error, Result};
use super::BillOpCode;
use super::BillOpCode::{
    Accept, Endorse, Issue, Mint, OfferToSell, Recourse, RejectToAccept, RejectToBuy, RejectToPay,
    RejectToPayRecourse, RequestRecourse, RequestToAccept, RequestToPay, Sell,
};

use crate::blockchain::{Block, FIRST_BLOCK_ID};
use crate::data::{
    bill::{BillKeys, BitcreditBill},
    contact::{ContactType, IdentityPublicData},
};
use crate::util::BcrKeys;
use crate::util::{self, crypto};

use crate::data::{File, PostalAddress};
use borsh::{from_slice, to_vec};
use borsh_derive::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct BillBlock {
    pub bill_id: String,
    pub id: u64,
    pub hash: String,
    pub previous_hash: String,
    pub timestamp: u64,
    pub data: String,
    pub public_key: String,
    pub signature: String,
    pub op_code: BillOpCode,
}

#[derive(BorshSerialize)]
pub struct BillBlockDataToHash {
    pub bill_id: String,
    id: u64,
    previous_hash: String,
    data: String,
    timestamp: u64,
    public_key: String,
    op_code: BillOpCode,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct BillRejectBlockData {
    pub rejecter: BillIdentityBlockData,
    pub signatory: Option<BillSignatoryBlockData>,
    pub signing_timestamp: u64,
    pub signing_address: PostalAddress,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct BillIssueBlockData {
    pub id: String,
    pub country_of_issuing: String,
    pub city_of_issuing: String,
    pub drawee: BillIdentityBlockData,
    pub drawer: BillIdentityBlockData,
    pub payee: BillIdentityBlockData,
    pub currency: String,
    pub sum: u64,
    pub maturity_date: String,
    pub issue_date: String,
    pub country_of_payment: String,
    pub city_of_payment: String,
    pub language: String,
    pub files: Vec<File>,
    pub signatory: Option<BillSignatoryBlockData>,
    pub signing_timestamp: u64,
    pub signing_address: PostalAddress,
}

impl BillIssueBlockData {
    pub fn from(
        value: BitcreditBill,
        signatory: Option<BillSignatoryBlockData>,
        timestamp: u64,
    ) -> Self {
        let signing_address = value.drawer.postal_address.clone();
        Self {
            id: value.id,
            country_of_issuing: value.country_of_issuing,
            city_of_issuing: value.city_of_issuing,
            drawee: value.drawee.into(),
            drawer: value.drawer.into(),
            payee: value.payee.into(),
            currency: value.currency,
            sum: value.sum,
            maturity_date: value.maturity_date,
            issue_date: value.issue_date,
            country_of_payment: value.country_of_payment,
            city_of_payment: value.city_of_payment,
            language: value.language,
            files: value.files,
            signatory,
            signing_timestamp: timestamp,
            signing_address, // address of the issuer
        }
    }
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct BillAcceptBlockData {
    pub accepter: BillIdentityBlockData,
    pub signatory: Option<BillSignatoryBlockData>,
    pub signing_timestamp: u64,
    pub signing_address: PostalAddress, // address of the accepter
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct BillRequestToPayBlockData {
    pub requester: BillIdentityBlockData,
    pub currency: String,
    pub signatory: Option<BillSignatoryBlockData>,
    pub signing_timestamp: u64,
    pub signing_address: PostalAddress, // address of the requester
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct BillRequestToAcceptBlockData {
    pub requester: BillIdentityBlockData,
    pub signatory: Option<BillSignatoryBlockData>,
    pub signing_timestamp: u64,
    pub signing_address: PostalAddress, // address of the requester
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct BillMintBlockData {
    pub endorser: BillIdentityBlockData,
    pub endorsee: BillIdentityBlockData,
    pub currency: String,
    pub sum: u64,
    pub signatory: Option<BillSignatoryBlockData>,
    pub signing_timestamp: u64,
    pub signing_address: PostalAddress, // address of the endorser
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct BillOfferToSellBlockData {
    pub seller: BillIdentityBlockData,
    pub buyer: BillIdentityBlockData,
    pub currency: String,
    pub sum: u64,
    pub payment_address: String,
    pub signatory: Option<BillSignatoryBlockData>,
    pub signing_timestamp: u64,
    pub signing_address: PostalAddress, // address of the seller
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct BillSellBlockData {
    pub seller: BillIdentityBlockData,
    pub buyer: BillIdentityBlockData,
    pub currency: String,
    pub sum: u64,
    pub payment_address: String,
    pub signatory: Option<BillSignatoryBlockData>,
    pub signing_timestamp: u64,
    pub signing_address: PostalAddress, // address of the seller
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct BillEndorseBlockData {
    pub endorser: BillIdentityBlockData,
    pub endorsee: BillIdentityBlockData,
    pub signatory: Option<BillSignatoryBlockData>,
    pub signing_timestamp: u64,
    pub signing_address: PostalAddress, // address of the endorser
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct BillRequestRecourseBlockData {
    pub recourser: BillIdentityBlockData,
    pub recoursee: BillIdentityBlockData,
    pub sum: u64,
    pub currency: String,
    pub signatory: Option<BillSignatoryBlockData>,
    pub signing_timestamp: u64,
    pub signing_address: PostalAddress, // address of the endorser
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct BillRecourseBlockData {
    pub recourser: BillIdentityBlockData,
    pub recoursee: BillIdentityBlockData,
    pub sum: u64,
    pub currency: String,
    pub signatory: Option<BillSignatoryBlockData>,
    pub signing_timestamp: u64,
    pub signing_address: PostalAddress, // address of the endorser
}

/// Legal data for parties within a bill transaction
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq, Eq)]
pub struct BillIdentityBlockData {
    pub t: ContactType,
    pub node_id: String,
    pub name: String,
    pub postal_address: PostalAddress,
}

impl From<IdentityPublicData> for BillIdentityBlockData {
    fn from(value: IdentityPublicData) -> Self {
        Self {
            t: value.t,
            node_id: value.node_id,
            name: value.name,
            postal_address: value.postal_address,
        }
    }
}

/// The name and node_id of a company signatory
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct BillSignatoryBlockData {
    pub node_id: String,
    pub name: String,
}

/// The data of the new holder in a holder-changing block, with the signatory data from the block
#[derive(Clone, Debug)]
pub struct HolderFromBlock {
    pub holder: BillIdentityBlockData,
    pub signer: BillIdentityBlockData,
    pub signatory: Option<BillSignatoryBlockData>,
}

impl Block for BillBlock {
    type OpCode = BillOpCode;
    type BlockDataToHash = BillBlockDataToHash;

    fn id(&self) -> u64 {
        self.id
    }

    fn timestamp(&self) -> u64 {
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

    fn get_block_data_to_hash(&self) -> Self::BlockDataToHash {
        let data = BillBlockDataToHash {
            bill_id: self.bill_id.clone(),
            id: self.id(),
            previous_hash: self.previous_hash().to_owned(),
            data: self.data().to_owned(),
            timestamp: self.timestamp(),
            public_key: self.public_key().to_owned(),
            op_code: self.op_code().to_owned(),
        };
        data
    }
}

/// Structure for the block data of a bill block
///
/// - `data` contains the actual data of the block, encrypted using the bill's pub key
/// - `key` is optional and if set, contains the bill private key encrypted by an identity
///   pub key (e.g. for Issue the issuer's and Endorse the endorsee's)
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq)]
pub struct BillBlockData {
    data: String,
    key: Option<String>,
}

impl BillBlock {
    /// Create a new block and sign it with an aggregated key, combining the identity key of the
    /// signer, and the company key if it exists and the bill key
    pub fn new(
        bill_id: String,
        id: u64,
        previous_hash: String,
        data: String,
        op_code: BillOpCode,
        identity_keys: &BcrKeys,
        company_keys: Option<&BcrKeys>,
        bill_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<Self> {
        // The order here is important: identity -> company -> bill
        let mut keys: Vec<String> = vec![];
        keys.push(identity_keys.get_private_key_string());
        if let Some(company_key) = company_keys {
            keys.push(company_key.get_private_key_string());
        }
        keys.push(bill_keys.get_private_key_string());

        let aggregated_public_key = crypto::get_aggregated_public_key(&keys)?;
        let hash = Self::calculate_hash(BillBlockDataToHash {
            bill_id: bill_id.clone(),
            id,
            previous_hash: previous_hash.clone(),
            data: data.clone(),
            timestamp,
            public_key: aggregated_public_key.clone(),
            op_code: op_code.clone(),
        })?;
        let signature = crypto::aggregated_signature(&hash, &keys)?;

        Ok(Self {
            bill_id,
            id,
            hash,
            timestamp,
            previous_hash,
            signature,
            public_key: aggregated_public_key,
            data,
            op_code,
        })
    }

    pub fn create_block_for_issue(
        bill_id: String,
        genesis_hash: String,
        bill: &BillIssueBlockData,
        drawer_keys: &BcrKeys,
        drawer_company_keys: Option<&BcrKeys>,
        bill_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<Self> {
        let key_bytes = to_vec(&bill_keys.get_private_key_string())?;
        // If drawer is a company, use drawer_company_keys for encryption
        let encrypted_key = match drawer_company_keys {
            None => util::base58_encode(&util::crypto::encrypt_ecies(
                &key_bytes,
                &drawer_keys.get_public_key(),
            )?),
            Some(company_keys) => util::base58_encode(&util::crypto::encrypt_ecies(
                &key_bytes,
                &company_keys.get_public_key(),
            )?),
        };

        let encrypted_and_hashed_bill_data = util::base58_encode(&util::crypto::encrypt_ecies(
            &to_vec(bill)?,
            &bill_keys.get_public_key(),
        )?);

        let data = BillBlockData {
            data: encrypted_and_hashed_bill_data,
            key: Some(encrypted_key),
        };
        let serialized_and_hashed_data = util::base58_encode(&to_vec(&data)?);

        Self::new(
            bill_id,
            FIRST_BLOCK_ID,
            genesis_hash,
            serialized_and_hashed_data,
            BillOpCode::Issue,
            drawer_keys,
            drawer_company_keys,
            bill_keys,
            timestamp,
        )
    }

    pub fn create_block_for_reject_to_accept(
        bill_id: String,
        previous_block: &Self,
        data: &BillRejectBlockData,
        identity_keys: &BcrKeys,
        company_keys: Option<&BcrKeys>,
        bill_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<Self> {
        let block = Self::encrypt_data_create_block_and_validate(
            bill_id,
            previous_block,
            data,
            identity_keys,
            company_keys,
            bill_keys,
            None,
            timestamp,
            BillOpCode::RejectToAccept,
        )?;
        Ok(block)
    }

    pub fn create_block_for_reject_to_pay(
        bill_id: String,
        previous_block: &Self,
        data: &BillRejectBlockData,
        identity_keys: &BcrKeys,
        company_keys: Option<&BcrKeys>,
        bill_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<Self> {
        let block = Self::encrypt_data_create_block_and_validate(
            bill_id,
            previous_block,
            data,
            identity_keys,
            company_keys,
            bill_keys,
            None,
            timestamp,
            BillOpCode::RejectToPay,
        )?;
        Ok(block)
    }

    pub fn create_block_for_reject_to_buy(
        bill_id: String,
        previous_block: &Self,
        data: &BillRejectBlockData,
        identity_keys: &BcrKeys,
        company_keys: Option<&BcrKeys>,
        bill_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<Self> {
        let block = Self::encrypt_data_create_block_and_validate(
            bill_id,
            previous_block,
            data,
            identity_keys,
            company_keys,
            bill_keys,
            None,
            timestamp,
            BillOpCode::RejectToBuy,
        )?;
        Ok(block)
    }

    pub fn create_block_for_reject_to_pay_recourse(
        bill_id: String,
        previous_block: &Self,
        data: &BillRejectBlockData,
        identity_keys: &BcrKeys,
        company_keys: Option<&BcrKeys>,
        bill_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<Self> {
        let block = Self::encrypt_data_create_block_and_validate(
            bill_id,
            previous_block,
            data,
            identity_keys,
            company_keys,
            bill_keys,
            None,
            timestamp,
            BillOpCode::RejectToPayRecourse,
        )?;
        Ok(block)
    }

    pub fn create_block_for_request_recourse(
        bill_id: String,
        previous_block: &Self,
        data: &BillRequestRecourseBlockData,
        identity_keys: &BcrKeys,
        company_keys: Option<&BcrKeys>,
        bill_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<Self> {
        let block = Self::encrypt_data_create_block_and_validate(
            bill_id,
            previous_block,
            data,
            identity_keys,
            company_keys,
            bill_keys,
            None,
            timestamp,
            BillOpCode::RequestRecourse,
        )?;
        Ok(block)
    }

    pub fn create_block_for_recourse(
        bill_id: String,
        previous_block: &Self,
        data: &BillRecourseBlockData,
        identity_keys: &BcrKeys,
        company_keys: Option<&BcrKeys>,
        bill_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<Self> {
        let block = Self::encrypt_data_create_block_and_validate(
            bill_id,
            previous_block,
            data,
            identity_keys,
            company_keys,
            bill_keys,
            None,
            timestamp,
            BillOpCode::Recourse,
        )?;
        Ok(block)
    }

    pub fn create_block_for_accept(
        bill_id: String,
        previous_block: &Self,
        data: &BillAcceptBlockData,
        identity_keys: &BcrKeys,
        company_keys: Option<&BcrKeys>,
        bill_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<Self> {
        let block = Self::encrypt_data_create_block_and_validate(
            bill_id,
            previous_block,
            data,
            identity_keys,
            company_keys,
            bill_keys,
            None,
            timestamp,
            BillOpCode::Accept,
        )?;
        Ok(block)
    }

    pub fn create_block_for_request_to_pay(
        bill_id: String,
        previous_block: &Self,
        data: &BillRequestToPayBlockData,
        identity_keys: &BcrKeys,
        company_keys: Option<&BcrKeys>,
        bill_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<Self> {
        let block = Self::encrypt_data_create_block_and_validate(
            bill_id,
            previous_block,
            data,
            identity_keys,
            company_keys,
            bill_keys,
            None,
            timestamp,
            BillOpCode::RequestToPay,
        )?;
        Ok(block)
    }

    pub fn create_block_for_request_to_accept(
        bill_id: String,
        previous_block: &Self,
        data: &BillRequestToAcceptBlockData,
        identity_keys: &BcrKeys,
        company_keys: Option<&BcrKeys>,
        bill_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<Self> {
        let block = Self::encrypt_data_create_block_and_validate(
            bill_id,
            previous_block,
            data,
            identity_keys,
            company_keys,
            bill_keys,
            None,
            timestamp,
            BillOpCode::RequestToAccept,
        )?;
        Ok(block)
    }

    pub fn create_block_for_mint(
        bill_id: String,
        previous_block: &Self,
        data: &BillMintBlockData,
        identity_keys: &BcrKeys,
        company_keys: Option<&BcrKeys>,
        bill_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<Self> {
        let block = Self::encrypt_data_create_block_and_validate(
            bill_id,
            previous_block,
            data,
            identity_keys,
            company_keys,
            bill_keys,
            Some(data.endorsee.node_id.as_str()),
            timestamp,
            BillOpCode::Mint,
        )?;
        Ok(block)
    }

    pub fn create_block_for_offer_to_sell(
        bill_id: String,
        previous_block: &Self,
        data: &BillOfferToSellBlockData,
        identity_keys: &BcrKeys,
        company_keys: Option<&BcrKeys>,
        bill_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<Self> {
        let block = Self::encrypt_data_create_block_and_validate(
            bill_id,
            previous_block,
            data,
            identity_keys,
            company_keys,
            bill_keys,
            None,
            timestamp,
            BillOpCode::OfferToSell,
        )?;
        Ok(block)
    }

    pub fn create_block_for_sell(
        bill_id: String,
        previous_block: &Self,
        data: &BillSellBlockData,
        identity_keys: &BcrKeys,
        company_keys: Option<&BcrKeys>,
        bill_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<Self> {
        let block = Self::encrypt_data_create_block_and_validate(
            bill_id,
            previous_block,
            data,
            identity_keys,
            company_keys,
            bill_keys,
            Some(data.buyer.node_id.as_str()),
            timestamp,
            BillOpCode::Sell,
        )?;
        Ok(block)
    }

    pub fn create_block_for_endorse(
        bill_id: String,
        previous_block: &Self,
        data: &BillEndorseBlockData,
        identity_keys: &BcrKeys,
        company_keys: Option<&BcrKeys>,
        bill_keys: &BcrKeys,
        timestamp: u64,
    ) -> Result<Self> {
        let block = Self::encrypt_data_create_block_and_validate(
            bill_id,
            previous_block,
            data,
            identity_keys,
            company_keys,
            bill_keys,
            Some(data.endorsee.node_id.as_str()),
            timestamp,
            BillOpCode::Endorse,
        )?;
        Ok(block)
    }

    fn encrypt_data_create_block_and_validate<T: borsh::BorshSerialize>(
        bill_id: String,
        previous_block: &Self,
        data: &T,
        identity_keys: &BcrKeys,
        company_keys: Option<&BcrKeys>,
        bill_keys: &BcrKeys,
        public_key_for_keys: Option<&str>, // when encrypting keys for a new holder
        timestamp: u64,
        op_code: BillOpCode,
    ) -> Result<Self> {
        let bytes = to_vec(&data)?;
        // encrypt data using the bill pub key
        let encrypted_data = util::base58_encode(&util::crypto::encrypt_ecies(
            &bytes,
            &bill_keys.get_public_key(),
        )?);

        let mut key = None;

        // in case there are keys to encrypt, encrypt them using the receiver's identity pub key
        if op_code == BillOpCode::Endorse
            || op_code == BillOpCode::Sell
            || op_code == BillOpCode::Mint
        {
            if let Some(new_holder_public_key) = public_key_for_keys {
                let key_bytes = to_vec(&bill_keys.get_private_key_string())?;
                let encrypted_key = util::base58_encode(&util::crypto::encrypt_ecies(
                    &key_bytes,
                    new_holder_public_key,
                )?);
                key = Some(encrypted_key);
            }
        }

        let data = BillBlockData {
            data: encrypted_data,
            key,
        };
        let serialized_and_hashed_data = util::base58_encode(&to_vec(&data)?);

        let new_block = Self::new(
            bill_id,
            previous_block.id + 1,
            previous_block.hash.clone(),
            serialized_and_hashed_data,
            op_code,
            identity_keys,
            company_keys,
            bill_keys,
            timestamp,
        )?;

        if !new_block.validate_with_previous(previous_block) {
            return Err(Error::BlockInvalid);
        }
        Ok(new_block)
    }

    /// Decrypts the block data using the bill's private key, returning the raw bytes
    pub fn get_decrypted_block_bytes<T: borsh::BorshDeserialize>(
        &self,
        bill_keys: &BillKeys,
    ) -> Result<T> {
        let bytes = util::base58_decode(&self.data)?;
        let block_data: BillBlockData = from_slice(&bytes)?;
        let decoded_data_bytes = util::base58_decode(&block_data.data)?;
        let decrypted_bytes =
            util::crypto::decrypt_ecies(&decoded_data_bytes, &bill_keys.private_key)?;
        let deserialized = from_slice::<T>(&decrypted_bytes)?;
        Ok(deserialized)
    }

    /// Extracts a list of unique node IDs involved in a block operation.
    ///
    /// # Parameters
    /// - `bill_keys`: The bill's keys
    ///
    /// # Returns
    /// A `Vec<String>` containing the unique peer IDs involved in the block. Peer IDs are included
    /// only if they are non-empty.
    ///
    pub fn get_nodes_from_block(&self, bill_keys: &BillKeys) -> Result<Vec<String>> {
        let mut nodes = HashSet::new();
        match self.op_code {
            Issue => {
                let bill: BillIssueBlockData = self.get_decrypted_block_bytes(bill_keys)?;
                nodes.insert(bill.drawer.node_id);
                nodes.insert(bill.payee.node_id);
                nodes.insert(bill.drawee.node_id);
            }
            Endorse => {
                let block_data_decrypted: BillEndorseBlockData =
                    self.get_decrypted_block_bytes(bill_keys)?;
                nodes.insert(block_data_decrypted.endorsee.node_id);
                nodes.insert(block_data_decrypted.endorser.node_id);
            }
            Mint => {
                let block_data_decrypted: BillMintBlockData =
                    self.get_decrypted_block_bytes(bill_keys)?;
                nodes.insert(block_data_decrypted.endorsee.node_id);
                nodes.insert(block_data_decrypted.endorser.node_id);
            }
            RequestToAccept => {
                let block_data_decrypted: BillRequestToAcceptBlockData =
                    self.get_decrypted_block_bytes(bill_keys)?;
                nodes.insert(block_data_decrypted.requester.node_id);
            }
            Accept => {
                let block_data_decrypted: BillAcceptBlockData =
                    self.get_decrypted_block_bytes(bill_keys)?;
                nodes.insert(block_data_decrypted.accepter.node_id);
            }
            RequestToPay => {
                let block_data_decrypted: BillRequestToPayBlockData =
                    self.get_decrypted_block_bytes(bill_keys)?;
                nodes.insert(block_data_decrypted.requester.node_id);
            }
            OfferToSell => {
                let block_data_decrypted: BillOfferToSellBlockData =
                    self.get_decrypted_block_bytes(bill_keys)?;
                nodes.insert(block_data_decrypted.buyer.node_id);
                nodes.insert(block_data_decrypted.seller.node_id);
            }
            Sell => {
                let block_data_decrypted: BillSellBlockData =
                    self.get_decrypted_block_bytes(bill_keys)?;
                nodes.insert(block_data_decrypted.buyer.node_id);
                nodes.insert(block_data_decrypted.seller.node_id);
            }
            RejectToAccept | RejectToBuy | RejectToPay | RejectToPayRecourse => {
                let block_data_decrypted: BillRejectBlockData =
                    self.get_decrypted_block_bytes(bill_keys)?;
                nodes.insert(block_data_decrypted.rejecter.node_id);
            }
            RequestRecourse => {
                let block_data_decrypted: BillRequestRecourseBlockData =
                    self.get_decrypted_block_bytes(bill_keys)?;
                nodes.insert(block_data_decrypted.recourser.node_id);
                nodes.insert(block_data_decrypted.recoursee.node_id);
            }
            Recourse => {
                let block_data_decrypted: BillRecourseBlockData =
                    self.get_decrypted_block_bytes(bill_keys)?;
                nodes.insert(block_data_decrypted.recourser.node_id);
                nodes.insert(block_data_decrypted.recoursee.node_id);
            }
        }
        Ok(nodes.into_iter().collect())
    }

    /// If the block is holder-changing block (issue, endorse, sell, mint, recourse), returns
    /// the new holder and signer data from the block
    pub fn get_holder_from_block(&self, bill_keys: &BillKeys) -> Result<Option<HolderFromBlock>> {
        match self.op_code {
            Issue => {
                let bill: BillIssueBlockData = self.get_decrypted_block_bytes(bill_keys)?;
                Ok(Some(HolderFromBlock {
                    holder: bill.payee,
                    signer: bill.drawer,
                    signatory: bill.signatory,
                }))
            }
            Endorse => {
                let block: BillEndorseBlockData = self.get_decrypted_block_bytes(bill_keys)?;
                Ok(Some(HolderFromBlock {
                    holder: block.endorsee,
                    signer: block.endorser,
                    signatory: block.signatory,
                }))
            }
            Mint => {
                let block: BillMintBlockData = self.get_decrypted_block_bytes(bill_keys)?;
                Ok(Some(HolderFromBlock {
                    holder: block.endorsee,
                    signer: block.endorser,
                    signatory: block.signatory,
                }))
            }
            Sell => {
                let block: BillSellBlockData = self.get_decrypted_block_bytes(bill_keys)?;
                Ok(Some(HolderFromBlock {
                    holder: block.buyer,
                    signer: block.seller,
                    signatory: block.signatory,
                }))
            }
            Recourse => {
                let block: BillRecourseBlockData = self.get_decrypted_block_bytes(bill_keys)?;
                Ok(Some(HolderFromBlock {
                    holder: block.recoursee,
                    signer: block.recourser,
                    signatory: block.signatory,
                }))
            }
            _ => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        blockchain::bill::tests::get_baseline_identity,
        tests::tests::{get_bill_keys, TEST_PRIVATE_KEY_SECP},
    };

    fn get_first_block() -> BillBlock {
        let mut bill = BitcreditBill::new_empty();
        bill.id = "some id".to_owned();
        let mut drawer = IdentityPublicData::new_empty();
        let node_id = BcrKeys::new().get_public_key();
        let mut payer = IdentityPublicData::new_empty();
        let payer_node_id = BcrKeys::new().get_public_key();
        payer.node_id = payer_node_id.clone();
        drawer.node_id = node_id.clone();

        bill.drawer = drawer.clone();
        bill.payee = drawer.clone();
        bill.drawee = payer;

        BillBlock::create_block_for_issue(
            "some id".to_string(),
            String::from("prevhash"),
            &BillIssueBlockData::from(bill, None, 1731593928),
            &get_baseline_identity().key_pair,
            None,
            &BcrKeys::from_private_key(&get_bill_keys().private_key).unwrap(),
            1731593928,
        )
        .unwrap()
    }

    #[test]
    fn signature_can_be_verified() {
        let block = BillBlock::new(
            "some id".to_string(),
            1,
            String::from("prevhash"),
            String::from("some_data"),
            BillOpCode::Issue,
            &BcrKeys::new(),
            None,
            &BcrKeys::new(),
            1731593928,
        )
        .unwrap();
        assert!(block.verify());
    }

    #[test]
    fn get_nodes_from_block_issue() {
        let mut bill = BitcreditBill::new_empty();
        let mut drawer = IdentityPublicData::new_empty();
        let node_id = BcrKeys::new().get_public_key();
        let mut payer = IdentityPublicData::new_empty();
        let payer_node_id = BcrKeys::new().get_public_key();
        payer.node_id = payer_node_id.clone();
        drawer.node_id = node_id.clone();
        bill.drawer = drawer.clone();
        bill.payee = drawer.clone();
        bill.drawee = payer;

        let block = BillBlock::create_block_for_issue(
            "some id".to_string(),
            String::from("prevhash"),
            &BillIssueBlockData::from(bill, None, 1731593928),
            &BcrKeys::new(),
            None,
            &BcrKeys::from_private_key(&get_bill_keys().private_key).unwrap(),
            1731593928,
        )
        .unwrap();
        let res = block.get_nodes_from_block(&get_bill_keys());
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().len(), 2);
        assert!(res.as_ref().unwrap().contains(&node_id));
        assert!(res.as_ref().unwrap().contains(&payer_node_id));
    }

    #[test]
    fn get_nodes_from_block_endorse() {
        let mut endorsee = IdentityPublicData::new_empty();
        let node_id = BcrKeys::new().get_public_key();
        endorsee.node_id = node_id.clone();
        let endorser =
            IdentityPublicData::new_only_node_id(get_baseline_identity().key_pair.get_public_key());
        let block = BillBlock::create_block_for_endorse(
            "some id".to_owned(),
            &get_first_block(),
            &BillEndorseBlockData {
                endorser: endorser.clone().into(),
                endorsee: endorsee.into(),
                signatory: None,
                signing_timestamp: 1731593928,
                signing_address: endorser.postal_address,
            },
            &get_baseline_identity().key_pair,
            None,
            &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1731593928,
        )
        .unwrap();
        let res = block.get_nodes_from_block(&get_bill_keys());
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().len(), 2);
        assert!(res.as_ref().unwrap().contains(&node_id));
        assert!(res.as_ref().unwrap().contains(&endorser.node_id));
    }

    #[test]
    fn get_nodes_from_block_mint() {
        let mut mint = IdentityPublicData::new_empty();
        let node_id = BcrKeys::new().get_public_key();
        mint.node_id = node_id.clone();
        let mut minter = IdentityPublicData::new_empty();
        let minter_node_id = BcrKeys::new().get_public_key();
        minter.node_id = minter_node_id.clone();
        let block = BillBlock::create_block_for_mint(
            "some id".to_owned(),
            &get_first_block(),
            &BillMintBlockData {
                endorser: minter.clone().into(),
                endorsee: mint.into(),
                sum: 5000,
                currency: "sat".to_string(),
                signatory: None,
                signing_timestamp: 1731593928,
                signing_address: minter.postal_address,
            },
            &get_baseline_identity().key_pair,
            None,
            &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1731593928,
        )
        .unwrap();
        let res = block.get_nodes_from_block(&get_bill_keys());
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().len(), 2);
        assert!(res.as_ref().unwrap().contains(&node_id));
        assert!(res.as_ref().unwrap().contains(&minter_node_id));
    }

    #[test]
    fn get_nodes_from_block_req_to_accept() {
        let mut requester = IdentityPublicData::new_empty();
        let node_id = BcrKeys::new().get_public_key();
        requester.node_id = node_id.clone();

        let block = BillBlock::create_block_for_request_to_accept(
            "some id".to_owned(),
            &get_first_block(),
            &BillRequestToAcceptBlockData {
                requester: requester.clone().into(),
                signatory: None,
                signing_timestamp: 1731593928,
                signing_address: requester.postal_address,
            },
            &get_baseline_identity().key_pair,
            None,
            &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1731593928,
        )
        .unwrap();
        let res = block.get_nodes_from_block(&get_bill_keys());
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().len(), 1);
        assert!(res.as_ref().unwrap().contains(&node_id));
    }

    #[test]
    fn get_nodes_from_block_accept() {
        let mut accepter = IdentityPublicData::new_empty();
        let node_id = BcrKeys::new().get_public_key();
        accepter.node_id = node_id.clone();
        accepter.postal_address = PostalAddress {
            country: String::from("Austria"),
            city: String::from("Vienna"),
            zip: Some(String::from("1020")),
            address: String::from("Hayekweg 12"),
        };

        let block = BillBlock::create_block_for_accept(
            "some id".to_owned(),
            &get_first_block(),
            &BillAcceptBlockData {
                accepter: accepter.clone().into(),
                signatory: None,
                signing_timestamp: 1731593928,
                signing_address: accepter.postal_address,
            },
            &get_baseline_identity().key_pair,
            None,
            &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1731593928,
        )
        .unwrap();
        let res = block.get_nodes_from_block(&get_bill_keys());
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().len(), 1);
        assert!(res.as_ref().unwrap().contains(&node_id));
    }

    #[test]
    fn get_nodes_from_block_req_to_pay() {
        let mut requester = IdentityPublicData::new_empty();
        let node_id = BcrKeys::new().get_public_key();
        requester.node_id = node_id.clone();

        let block = BillBlock::create_block_for_request_to_pay(
            "some id".to_string(),
            &get_first_block(),
            &BillRequestToPayBlockData {
                requester: requester.clone().into(),
                currency: "sat".to_string(),
                signatory: None,
                signing_timestamp: 1731593928,
                signing_address: requester.postal_address,
            },
            &get_baseline_identity().key_pair,
            None,
            &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1731593928,
        )
        .unwrap();
        let res = block.get_nodes_from_block(&get_bill_keys());
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().len(), 1);
        assert!(res.as_ref().unwrap().contains(&node_id));
    }

    #[test]
    fn get_nodes_from_block_offer_to_sell() {
        let mut buyer = IdentityPublicData::new_empty();
        let node_id = BcrKeys::new().get_public_key();
        buyer.node_id = node_id.clone();
        let seller =
            IdentityPublicData::new_only_node_id(get_baseline_identity().key_pair.get_public_key());
        let block = BillBlock::create_block_for_offer_to_sell(
            "some id".to_string(),
            &get_first_block(),
            &BillOfferToSellBlockData {
                buyer: buyer.clone().into(),
                seller: seller.clone().into(),
                sum: 5000,
                currency: "sat".to_string(),
                payment_address: "1234".to_string(),
                signatory: None,
                signing_timestamp: 1731593928,
                signing_address: seller.postal_address,
            },
            &get_baseline_identity().key_pair,
            None,
            &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1731593928,
        )
        .unwrap();
        let res = block.get_nodes_from_block(&get_bill_keys());
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().len(), 2);
        assert!(res.as_ref().unwrap().contains(&node_id));
        assert!(res.as_ref().unwrap().contains(&seller.node_id));
    }

    #[test]
    fn get_nodes_from_block_sell() {
        let mut buyer = IdentityPublicData::new_empty();
        let node_id = BcrKeys::new().get_public_key();
        buyer.node_id = node_id.clone();
        let seller =
            IdentityPublicData::new_only_node_id(get_baseline_identity().key_pair.get_public_key());
        let block = BillBlock::create_block_for_sell(
            "some id".to_string(),
            &get_first_block(),
            &BillSellBlockData {
                buyer: buyer.clone().into(),
                seller: seller.clone().into(),
                sum: 5000,
                currency: "sat".to_string(),
                payment_address: "1234".to_string(),
                signatory: Some(BillSignatoryBlockData {
                    node_id: buyer.node_id.clone(),
                    name: buyer.name.clone(),
                }),
                signing_timestamp: 1731593928,
                signing_address: seller.postal_address,
            },
            &get_baseline_identity().key_pair,
            None,
            &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1731593928,
        )
        .unwrap();
        let res = block.get_nodes_from_block(&get_bill_keys());
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().len(), 2);
        assert!(res.as_ref().unwrap().contains(&node_id));
        assert!(res.as_ref().unwrap().contains(&seller.node_id));
    }

    #[test]
    fn get_nodes_from_block_reject_to_accept() {
        let rejecter = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        let block = BillBlock::create_block_for_reject_to_accept(
            "some id".to_string(),
            &get_first_block(),
            &BillRejectBlockData {
                rejecter: rejecter.clone().into(),
                signatory: None,
                signing_timestamp: 1731593928,
                signing_address: rejecter.postal_address,
            },
            &get_baseline_identity().key_pair,
            None,
            &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1731593928,
        )
        .unwrap();
        let res = block.get_nodes_from_block(&get_bill_keys());
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().len(), 1);
        assert!(res.as_ref().unwrap().contains(&rejecter.node_id));
    }

    #[test]
    fn get_nodes_from_block_reject_to_pay() {
        let rejecter = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        let block = BillBlock::create_block_for_reject_to_pay(
            "some id".to_string(),
            &get_first_block(),
            &BillRejectBlockData {
                rejecter: rejecter.clone().into(),
                signatory: None,
                signing_timestamp: 1731593928,
                signing_address: rejecter.postal_address,
            },
            &get_baseline_identity().key_pair,
            None,
            &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1731593928,
        )
        .unwrap();
        let res = block.get_nodes_from_block(&get_bill_keys());
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().len(), 1);
        assert!(res.as_ref().unwrap().contains(&rejecter.node_id));
    }

    #[test]
    fn get_nodes_from_block_reject_to_buy() {
        let rejecter = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        let block = BillBlock::create_block_for_reject_to_buy(
            "some id".to_string(),
            &get_first_block(),
            &BillRejectBlockData {
                rejecter: rejecter.clone().into(),
                signatory: None,
                signing_timestamp: 1731593928,
                signing_address: rejecter.postal_address,
            },
            &get_baseline_identity().key_pair,
            None,
            &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1731593928,
        )
        .unwrap();
        let res = block.get_nodes_from_block(&get_bill_keys());
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().len(), 1);
        assert!(res.as_ref().unwrap().contains(&rejecter.node_id));
    }

    #[test]
    fn get_nodes_from_block_reject_to_pay_recourse() {
        let rejecter = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        let block = BillBlock::create_block_for_reject_to_pay_recourse(
            "some id".to_string(),
            &get_first_block(),
            &BillRejectBlockData {
                rejecter: rejecter.clone().into(),
                signatory: None,
                signing_timestamp: 1731593928,
                signing_address: rejecter.postal_address,
            },
            &get_baseline_identity().key_pair,
            None,
            &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1731593928,
        )
        .unwrap();
        let res = block.get_nodes_from_block(&get_bill_keys());
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().len(), 1);
        assert!(res.as_ref().unwrap().contains(&rejecter.node_id));
    }

    #[test]
    fn get_nodes_from_block_request_recourse() {
        let recoursee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        let recourser = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        let block = BillBlock::create_block_for_request_recourse(
            "some id".to_string(),
            &get_first_block(),
            &BillRequestRecourseBlockData {
                recourser: recourser.clone().into(),
                recoursee: recoursee.clone().into(),
                sum: 15000,
                currency: "sat".to_string(),
                signatory: None,
                signing_timestamp: 1731593928,
                signing_address: recourser.postal_address,
            },
            &get_baseline_identity().key_pair,
            None,
            &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1731593928,
        )
        .unwrap();
        let res = block.get_nodes_from_block(&get_bill_keys());
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().len(), 2);
        assert!(res.as_ref().unwrap().contains(&recourser.node_id));
        assert!(res.as_ref().unwrap().contains(&recoursee.node_id));
    }

    #[test]
    fn get_nodes_from_block_recourse() {
        let recoursee = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        let recourser = IdentityPublicData::new_only_node_id(BcrKeys::new().get_public_key());
        let block = BillBlock::create_block_for_recourse(
            "some id".to_string(),
            &get_first_block(),
            &BillRecourseBlockData {
                recourser: recourser.clone().into(),
                recoursee: recoursee.clone().into(),
                sum: 15000,
                currency: "sat".to_string(),
                signatory: None,
                signing_timestamp: 1731593928,
                signing_address: recourser.postal_address,
            },
            &get_baseline_identity().key_pair,
            None,
            &BcrKeys::from_private_key(TEST_PRIVATE_KEY_SECP).unwrap(),
            1731593928,
        )
        .unwrap();
        let res = block.get_nodes_from_block(&get_bill_keys());
        assert!(res.is_ok());
        assert_eq!(res.as_ref().unwrap().len(), 2);
        assert!(res.as_ref().unwrap().contains(&recourser.node_id));
        assert!(res.as_ref().unwrap().contains(&recoursee.node_id));
    }
}
