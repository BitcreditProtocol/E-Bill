use super::super::{Error, Result};
use crate::{
    blockchain::{
        company::{CompanyBlock, CompanyBlockchain, CompanyOpCode},
        Block,
    },
    constants::{
        DB_BLOCK_ID, DB_COMPANY_ID, DB_DATA, DB_HASH, DB_OP_CODE, DB_PREVIOUS_HASH, DB_PUBLIC_KEY,
        DB_SIGNATORY_NODE_ID, DB_SIGNATURE, DB_TABLE, DB_TIMESTAMP,
    },
    persistence::company::CompanyChainStoreApi,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use surrealdb::{engine::any::Any, Surreal};

const CREATE_BLOCK_QUERY: &str = r#"CREATE type::table($table) CONTENT {
                                    company_id: $company_id,
                                    block_id: $block_id,
                                    hash: $hash,
                                    previous_hash: $previous_hash,
                                    signature: $signature,
                                    timestamp: $timestamp,
                                    public_key: $public_key,
                                    signatory_node_id: $signatory_node_id,
                                    data: $data,
                                    op_code: $op_code
                                };"#;

#[derive(Clone)]
pub struct SurrealCompanyChainStore {
    db: Surreal<Any>,
}

impl SurrealCompanyChainStore {
    const TABLE: &'static str = "company_chain";

    pub fn new(db: Surreal<Any>) -> Self {
        Self { db }
    }

    async fn create_block(&self, query: &str, entity: CompanyBlockDb) -> Result<()> {
        let _ = self
            .db
            .query(query)
            .bind((DB_TABLE, Self::TABLE))
            .bind((DB_COMPANY_ID, entity.company_id))
            .bind((DB_BLOCK_ID, entity.block_id))
            .bind((DB_HASH, entity.hash))
            .bind((DB_PREVIOUS_HASH, entity.previous_hash))
            .bind((DB_SIGNATURE, entity.signature))
            .bind((DB_TIMESTAMP, entity.timestamp))
            .bind((DB_PUBLIC_KEY, entity.public_key))
            .bind((DB_SIGNATORY_NODE_ID, entity.signatory_node_id))
            .bind((DB_DATA, entity.data))
            .bind((DB_OP_CODE, entity.op_code))
            .await?
            .check()?;
        Ok(())
    }
}

#[async_trait]
impl CompanyChainStoreApi for SurrealCompanyChainStore {
    async fn get_latest_block(&self, id: &str) -> Result<CompanyBlock> {
        let result: Vec<CompanyBlockDb> = self
            .db
            .query("SELECT * FROM type::table($table) WHERE company_id = $company_id ORDER BY block_id DESC LIMIT 1")
            .bind((DB_TABLE, Self::TABLE))
            .bind((DB_COMPANY_ID, id.to_owned()))
            .await?
            .take(0)?;

        match result.first() {
            None => Err(Error::NoCompanyBlock),
            Some(block) => Ok(block.to_owned().into()),
        }
    }

    async fn add_block(&self, id: &str, block: &CompanyBlock) -> Result<()> {
        let entity: CompanyBlockDb = block.into();
        match self.get_latest_block(id).await {
            Err(Error::NoCompanyBlock) => {
                // if there is no latest block, ensure it's a valid first block
                if block.id == 1 && block.verify() && block.validate_hash() {
                    // Atomically ensure it's the first block
                    let query = format!(
                        r#"
                        BEGIN TRANSACTION;
                        LET $blocks = (RETURN count(SELECT * FROM type::table($table) WHERE company_id = $company_id));
                        IF $blocks = 0 AND $block_id = 1 {{
                            {}
                        }} ELSE {{
                            THROW "invalid block - not the first block";
                        }};
                        COMMIT TRANSACTION;
                    "#,
                        CREATE_BLOCK_QUERY
                    );
                    self.create_block(&query, entity).await?;
                    Ok(())
                } else {
                    return Err(Error::AddCompanyBlock(format!(
                        "First Block validation error: block id: {}",
                        block.id
                    )));
                }
            }
            Ok(latest_block) => {
                // if there is a latest block, ensure it's a valid follow-up block
                if !block.validate_with_previous(&latest_block) {
                    return Err(Error::AddCompanyBlock(format!(
                        "Block validation error: block id: {}, latest block id: {}",
                        block.id, latest_block.id
                    )));
                }
                // Atomically ensure the block is valid
                let query = format!(
                    r#"
                    BEGIN TRANSACTION;
                    LET $latest_block = (SELECT block_id, hash FROM type::table($table) WHERE company_id = $company_id ORDER BY block_id DESC LIMIT 1)[0];
                    IF $latest_block.block_id + 1 = $block_id AND $latest_block.hash = $previous_hash {{
                        {}
                    }} ELSE {{
                        THROW "invalid block";
                    }};
                    COMMIT TRANSACTION;
                "#,
                    CREATE_BLOCK_QUERY
                );
                self.create_block(&query, entity).await?;
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    async fn remove(&self, id: &str) -> Result<()> {
        self.db
            .query("DELETE FROM type::table($table) WHERE company_id = $company_id")
            .bind((DB_TABLE, Self::TABLE))
            .bind((DB_COMPANY_ID, id.to_owned()))
            .await?;
        Ok(())
    }

    async fn get_chain(&self, id: &str) -> Result<CompanyBlockchain> {
        let result: Vec<CompanyBlockDb> = self
            .db
            .query("SELECT * FROM type::table($table) WHERE company_id = $company_id ORDER BY block_id ASC")
            .bind((DB_TABLE, Self::TABLE))
            .bind((DB_COMPANY_ID, id.to_owned()))
            .await?
            .take(0)?;

        let blocks: Vec<CompanyBlock> = result.into_iter().map(|b| b.into()).collect();
        let chain = CompanyBlockchain::new_from_blocks(blocks)?;

        Ok(chain)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyBlockDb {
    pub company_id: String,
    pub block_id: u64,
    pub hash: String,
    pub previous_hash: String,
    pub signature: String,
    pub timestamp: u64,
    pub public_key: String,
    pub signatory_node_id: String,
    pub data: String,
    pub op_code: CompanyOpCode,
}

impl From<CompanyBlockDb> for CompanyBlock {
    fn from(value: CompanyBlockDb) -> Self {
        Self {
            company_id: value.company_id,
            id: value.block_id,
            hash: value.hash,
            timestamp: value.timestamp,
            data: value.data,
            public_key: value.public_key,
            signatory_node_id: value.signatory_node_id,
            previous_hash: value.previous_hash,
            signature: value.signature,
            op_code: value.op_code,
        }
    }
}

impl From<&CompanyBlock> for CompanyBlockDb {
    fn from(value: &CompanyBlock) -> Self {
        Self {
            company_id: value.company_id.clone(),
            block_id: value.id,
            hash: value.hash.clone(),
            previous_hash: value.previous_hash.clone(),
            signature: value.signature.clone(),
            timestamp: value.timestamp,
            public_key: value.public_key.clone(),
            signatory_node_id: value.signatory_node_id.clone(),
            data: value.data.clone(),
            op_code: value.op_code.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        blockchain::company::CompanyUpdateBlockData,
        data::{
            company::{Company, CompanyKeys},
            OptionalPostalAddress, PostalAddress,
        },
        persistence::db::get_memory_db,
        tests::tests::{TEST_PRIVATE_KEY_SECP, TEST_PUB_KEY_SECP},
        util::BcrKeys,
    };

    async fn get_store() -> SurrealCompanyChainStore {
        let mem_db = get_memory_db("test", "company_chain")
            .await
            .expect("could not create get_memory_db");
        SurrealCompanyChainStore::new(mem_db)
    }

    fn get_company_keys() -> CompanyKeys {
        CompanyKeys {
            private_key: TEST_PRIVATE_KEY_SECP.to_string(),
            public_key: TEST_PUB_KEY_SECP.to_string(),
        }
    }

    #[tokio::test]
    async fn test_add_block() {
        let store = get_store().await;
        let block = CompanyBlock::create_block_for_create(
            "some_id".to_string(),
            "genesis hash".to_string(),
            &Company {
                id: "some_id".to_string(),
                name: "Hayek Ltd".to_string(),
                country_of_registration: Some("AT".to_string()),
                city_of_registration: Some("Vienna".to_string()),
                postal_address: PostalAddress::new_empty(),
                email: "hayekltd@example.com".to_string(),
                registration_number: Some("123124123".to_string()),
                registration_date: Some("2024-01-01".to_string()),
                proof_of_registration_file: None,
                logo_file: None,
                signatories: vec!["self".to_string()],
            }
            .into(),
            &BcrKeys::new(),
            &get_company_keys(),
            1731593928,
        )
        .unwrap();
        store.add_block("some_id", &block).await.unwrap();
        let last_block = store.get_latest_block("some_id").await;
        assert!(last_block.is_ok());
        assert_eq!(last_block.as_ref().unwrap().id, 1);

        let block2 = CompanyBlock::create_block_for_update(
            "some_id".to_string(),
            &block,
            &CompanyUpdateBlockData {
                name: None,
                email: None,
                postal_address: OptionalPostalAddress::new_empty(),
                country_of_registration: None,
                city_of_registration: None,
                registration_number: None,
                registration_date: None,
                logo_file: None,
                proof_of_registration_file: None,
            },
            &BcrKeys::new(),
            &get_company_keys(),
            1731593928,
        )
        .unwrap();
        store.add_block("some_id", &block2).await.unwrap();
        let last_block = store.get_latest_block("some_id").await;
        assert!(last_block.is_ok());
        assert_eq!(last_block.as_ref().unwrap().id, 2);
    }

    #[tokio::test]
    async fn test_remove_blockchain() {
        let store = get_store().await;
        let block = CompanyBlock::create_block_for_create(
            "some_id".to_string(),
            "genesis hash".to_string(),
            &Company {
                id: "some_id".to_string(),
                name: "Hayek Ltd".to_string(),
                country_of_registration: Some("AT".to_string()),
                city_of_registration: Some("Vienna".to_string()),
                postal_address: PostalAddress::new_empty(),
                email: "hayekltd@example.com".to_string(),
                registration_number: Some("123124123".to_string()),
                registration_date: Some("2024-01-01".to_string()),
                proof_of_registration_file: None,
                logo_file: None,
                signatories: vec!["self".to_string()],
            }
            .into(),
            &BcrKeys::new(),
            &get_company_keys(),
            1731593928,
        )
        .unwrap();
        store.add_block("some_id", &block).await.unwrap();
        let result = store.remove("some_id").await;
        assert!(result.is_ok());
    }
}
