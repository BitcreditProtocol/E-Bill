#![cfg(any(target_arch = "wasm32", test))]
use super::super::{Error, Result, file_upload::FileUploadStoreApi};
#[cfg(target_arch = "wasm32")]
use super::get_new_surreal_files_db;
use crate::constants::{DB_ENTITY_ID, DB_FILE_NAME, DB_TABLE};
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use surrealdb::{Surreal, engine::any::Any};

pub struct FileUploadStore {
    #[allow(dead_code)]
    db: Surreal<Any>,
}

impl FileUploadStore {
    const TEMP_FILES_TABLE: &'static str = "temp_files";
    const ATTACHED_FILES_TABLE: &'static str = "attached_files";

    pub fn new(db: Surreal<Any>) -> Self {
        Self { db }
    }

    #[cfg(target_arch = "wasm32")]
    async fn db(&self) -> Result<Surreal<Any>> {
        get_new_surreal_files_db().await
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn db(&self) -> Result<Surreal<Any>> {
        Ok(self.db.clone())
    }

    pub async fn cleanup_temp_uploads(&self) -> Result<()> {
        log::info!("cleaning up temp uploads");
        let _: Vec<FileDb> = self.db().await?.delete(Self::TEMP_FILES_TABLE).await?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDb {
    pub file_upload_id: String,
    pub file_name: String,
    pub file_bytes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachedFileDb {
    pub entity_id: String, // entity the file is attached to, e.g. a bill/company/contact
    pub file_name: String,
    pub encrypted_bytes: String,
}

#[async_trait]
impl FileUploadStoreApi for FileUploadStore {
    async fn create_temp_upload_folder(&self, _file_upload_id: &str) -> Result<()> {
        // NOOP for wasm32
        Ok(())
    }

    async fn remove_temp_upload_folder(&self, _file_upload_id: &str) -> Result<()> {
        // NOOP for wasm32
        Ok(())
    }

    async fn write_temp_upload_file(
        &self,
        file_upload_id: &str,
        file_name: &str,
        file_bytes: &[u8],
    ) -> Result<()> {
        let entity = FileDb {
            file_upload_id: file_upload_id.to_owned(),
            file_name: file_name.to_owned(),
            file_bytes: STANDARD.encode(file_bytes),
        };
        let _: Option<FileDb> = self
            .db()
            .await?
            .create((Self::TEMP_FILES_TABLE, file_upload_id.to_owned()))
            .content(entity)
            .await?;
        Ok(())
    }

    async fn read_temp_upload_file(&self, file_upload_id: &str) -> Result<(String, Vec<u8>)> {
        let result: Option<FileDb> = self
            .db()
            .await?
            .select((Self::TEMP_FILES_TABLE, file_upload_id))
            .await?;
        match result {
            None => Err(Error::NoSuchEntity(
                "file".to_string(),
                file_upload_id.to_owned(),
            )),
            Some(f) => Ok((
                f.file_name,
                STANDARD
                    .decode(&f.file_bytes)
                    .map_err(|_| Error::EncodingError)?,
            )),
        }
    }

    async fn save_attached_file(
        &self,
        encrypted_bytes: &[u8],
        id: &str,
        file_name: &str,
    ) -> Result<()> {
        let entity = AttachedFileDb {
            entity_id: id.to_owned(),
            file_name: file_name.to_owned(),
            encrypted_bytes: STANDARD.encode(encrypted_bytes),
        };
        let _: Option<AttachedFileDb> = self
            .db()
            .await?
            .create(Self::ATTACHED_FILES_TABLE)
            .content(entity)
            .await?;
        Ok(())
    }

    async fn open_attached_file(&self, id: &str, file_name: &str) -> Result<Vec<u8>> {
        let result: Vec<AttachedFileDb> = self
            .db().await?
            .query("SELECT * from type::table($table) WHERE entity_id = $entity_id AND file_name = $file_name")
            .bind((DB_TABLE, Self::ATTACHED_FILES_TABLE))
            .bind((DB_ENTITY_ID, id.to_owned()))
            .bind((DB_FILE_NAME, file_name.to_owned()))
            .await?
            .take(0)?;
        if let Some(attached_file) = result.into_iter().next() {
            Ok(STANDARD
                .decode(attached_file.encrypted_bytes)
                .map_err(|_| Error::EncodingError)?)
        } else {
            Err(Error::NoSuchEntity(
                "attached file".to_string(),
                id.to_owned(),
            ))
        }
    }

    async fn delete_attached_files(&self, id: &str) -> Result<()> {
        let _: Vec<AttachedFileDb> = self
            .db()
            .await?
            .query("DELETE from type::table($table) WHERE entity_id = $entity_id")
            .bind((DB_TABLE, Self::ATTACHED_FILES_TABLE))
            .bind((DB_ENTITY_ID, id.to_owned()))
            .await?
            .take(0)?;
        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::db::get_memory_db;

    #[tokio::test]
    async fn test_temp_file() {
        let temp_store = get_temp_store().await;
        temp_store
            .write_temp_upload_file("some_id", "file_name.jpg", &[])
            .await
            .unwrap();
        let temp_file = temp_store
            .read_temp_upload_file("some_id")
            .await
            .unwrap()
            .clone();
        assert_eq!(temp_file.0, String::from("file_name.jpg"));
    }

    #[tokio::test]
    async fn test_attached_file() {
        let attached_store = get_attached_store().await;
        attached_store
            .save_attached_file(&[], "some_id", "file_name.jpg")
            .await
            .unwrap();
        attached_store
            .save_attached_file("hello_world".as_bytes(), "some_id", "other_file.jpg")
            .await
            .unwrap();
        let attached_file_1 = attached_store
            .open_attached_file("some_id", "file_name.jpg")
            .await
            .unwrap();
        assert_eq!(attached_file_1.len(), 0);
        let attached_file_2 = attached_store
            .open_attached_file("some_id", "other_file.jpg")
            .await
            .unwrap();
        assert_eq!(attached_file_2.len(), 11);
        // delete files for id
        attached_store
            .delete_attached_files("some_id")
            .await
            .unwrap();
        let after_delete = attached_store
            .open_attached_file("some_id", "file_name.jpg")
            .await;
        // files are gone
        assert!(after_delete.is_err());
    }

    async fn get_temp_store() -> FileUploadStore {
        let mem_db = get_memory_db("test", "temp_files")
            .await
            .expect("could not create get_memory_db");
        FileUploadStore::new(mem_db)
    }

    async fn get_attached_store() -> FileUploadStore {
        let mem_db = get_memory_db("test", "attached_files")
            .await
            .expect("could not create get_memory_db");
        FileUploadStore::new(mem_db)
    }
}
