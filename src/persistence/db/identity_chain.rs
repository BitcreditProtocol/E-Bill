use super::super::{Error, Result};
use crate::{
    blockchain::{
        identity::{IdentityBlock, IdentityOpCode},
        Block,
    },
    constants::{
        DB_BLOCK_ID, DB_DATA, DB_HASH, DB_OP_CODE, DB_PREVIOUS_HASH, DB_PUBLIC_KEY, DB_SIGNATURE,
        DB_TABLE, DB_TIMESTAMP,
    },
    persistence::identity::IdentityChainStoreApi,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use surrealdb::{engine::any::Any, Surreal};

const CREATE_BLOCK_QUERY: &str = r#"CREATE type::table($table) CONTENT {
                                    block_id: $block_id,
                                    hash: $hash,
                                    previous_hash: $previous_hash,
                                    signature: $signature,
                                    timestamp: $timestamp,
                                    public_key: $public_key,
                                    data: $data,
                                    op_code: $op_code
                                };"#;

#[derive(Clone)]
pub struct SurrealIdentityChainStore {
    db: Surreal<Any>,
}

impl SurrealIdentityChainStore {
    const TABLE: &'static str = "identity_chain";

    pub fn new(db: Surreal<Any>) -> Self {
        Self { db }
    }

    async fn create_block(&self, query: &str, entity: IdentityBlockDb) -> Result<()> {
        let _ = self
            .db
            .query(query)
            .bind((DB_TABLE, Self::TABLE))
            .bind((DB_BLOCK_ID, entity.block_id))
            .bind((DB_HASH, entity.hash))
            .bind((DB_PREVIOUS_HASH, entity.previous_hash))
            .bind((DB_SIGNATURE, entity.signature))
            .bind((DB_TIMESTAMP, entity.timestamp))
            .bind((DB_PUBLIC_KEY, entity.public_key))
            .bind((DB_DATA, entity.data))
            .bind((DB_OP_CODE, entity.op_code))
            .await?
            .check()?;
        Ok(())
    }
}

#[async_trait]
impl IdentityChainStoreApi for SurrealIdentityChainStore {
    async fn get_latest_block(&self) -> Result<IdentityBlock> {
        let result: Vec<IdentityBlockDb> = self
            .db
            .query("SELECT * FROM type::table($table) ORDER BY block_id DESC LIMIT 1")
            .bind((DB_TABLE, Self::TABLE))
            .await?
            .take(0)?;

        match result.first() {
            None => Err(Error::NoIdentityBlock),
            Some(block) => Ok(block.to_owned().into()),
        }
    }

    async fn add_block(&self, block: &IdentityBlock) -> Result<()> {
        let entity: IdentityBlockDb = block.into();
        match self.get_latest_block().await {
            Err(Error::NoIdentityBlock) => {
                // if there is no latest block, ensure it's a valid first block
                if block.id == 1 && block.verify() && block.validate_hash() {
                    // Atomically ensure it's the first block
                    let query = format!(
                        r#"
                        BEGIN TRANSACTION;
                        LET $blocks = (RETURN count(SELECT * FROM type::table($table)));
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
                    return Err(Error::AddIdentityBlock(format!(
                        "First Block validation error: block id: {}",
                        block.id
                    )));
                }
            }
            Ok(latest_block) => {
                // if there is a latest block, ensure it's a valid follow-up block
                if !block.validate_with_previous(&latest_block) {
                    return Err(Error::AddIdentityBlock(format!(
                        "Block validation error: block id: {}, latest block id: {}",
                        block.id, latest_block.id
                    )));
                }
                // Atomically ensure the block is valid
                let query = format!(
                    r#"
                    BEGIN TRANSACTION;
                    LET $latest_block = (SELECT block_id, hash FROM type::table($table) ORDER BY block_id DESC LIMIT 1)[0];
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityBlockDb {
    pub block_id: u64,
    pub hash: String,
    pub previous_hash: String,
    pub signature: String,
    pub timestamp: u64,
    pub public_key: String,
    pub data: String,
    pub op_code: IdentityOpCode,
}

impl From<IdentityBlockDb> for IdentityBlock {
    fn from(value: IdentityBlockDb) -> Self {
        Self {
            id: value.block_id,
            hash: value.hash,
            timestamp: value.timestamp,
            data: value.data,
            public_key: value.public_key,
            previous_hash: value.previous_hash,
            signature: value.signature,
            op_code: value.op_code,
        }
    }
}

impl From<&IdentityBlock> for IdentityBlockDb {
    fn from(value: &IdentityBlock) -> Self {
        Self {
            block_id: value.id,
            hash: value.hash.clone(),
            previous_hash: value.previous_hash.clone(),
            signature: value.signature.clone(),
            timestamp: value.timestamp,
            public_key: value.public_key.clone(),
            data: value.data.clone(),
            op_code: value.op_code.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        blockchain::identity::IdentityUpdateBlockData,
        data::{identity::Identity, OptionalPostalAddress},
        persistence::db::get_memory_db,
        util::BcrKeys,
    };

    async fn get_store() -> SurrealIdentityChainStore {
        let mem_db = get_memory_db("test", "identity_chain")
            .await
            .expect("could not create get_memory_db");
        SurrealIdentityChainStore::new(mem_db)
    }

    #[tokio::test]
    async fn test_add_block() {
        let store = get_store().await;
        let block = IdentityBlock::create_block_for_create(
            "genesis hash".to_string(),
            &Identity::new_empty().into(),
            &BcrKeys::new(),
            1731593928,
        )
        .unwrap();
        store.add_block(&block).await.unwrap();
        let last_block = store.get_latest_block().await;
        assert!(last_block.is_ok());
        assert_eq!(last_block.as_ref().unwrap().id, 1);

        let block2 = IdentityBlock::create_block_for_update(
            &block,
            &IdentityUpdateBlockData {
                name: None,
                email: None,
                postal_address: OptionalPostalAddress::new_empty(),
                date_of_birth: None,
                country_of_birth: None,
                city_of_birth: None,
                identification_number: None,
                profile_picture_file: None,
                identity_document_file: None,
            },
            &BcrKeys::new(),
            1731593928,
        )
        .unwrap();
        store.add_block(&block2).await.unwrap();
        let last_block = store.get_latest_block().await;
        assert!(last_block.is_ok());
        assert_eq!(last_block.as_ref().unwrap().id, 2);
    }
}
