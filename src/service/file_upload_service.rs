use super::{Error, Result};
use crate::constants::{MAX_FILE_NAME_CHARACTERS, MAX_FILE_SIZE_BYTES, VALID_FILE_MIME_TYPES};
use crate::persistence::file_upload::FileUploadStoreApi;
use crate::web::data::UploadFilesResponse;
use crate::{persistence, util};
use async_trait::async_trait;
use log::error;
use std::sync::Arc;

#[async_trait]
pub trait FileUploadServiceApi: Send + Sync {
    /// validates the given uploaded file
    async fn validate_attached_file(&self, file: &dyn util::file::UploadFileHandler) -> Result<()>;

    /// uploads files
    async fn upload_files(
        &self,
        files: Vec<&dyn util::file::UploadFileHandler>,
    ) -> Result<UploadFilesResponse>;

    /// returns a temp upload file
    async fn get_temp_file(&self, file_upload_id: &str) -> Result<Option<(String, Vec<u8>)>>;
}

#[derive(Clone)]
pub struct FileUploadService {
    file_upload_store: Arc<dyn FileUploadStoreApi>,
}

impl FileUploadService {
    pub fn new(file_upload_store: Arc<dyn FileUploadStoreApi>) -> Self {
        Self { file_upload_store }
    }
}

#[async_trait]
impl FileUploadServiceApi for FileUploadService {
    async fn validate_attached_file(&self, file: &dyn util::file::UploadFileHandler) -> Result<()> {
        if file.len() > MAX_FILE_SIZE_BYTES as u64 {
            return Err(Error::Validation(format!(
                "Maximum file size is {} bytes",
                MAX_FILE_SIZE_BYTES
            )));
        }

        let name = match file.name() {
            Some(n) => n,
            None => {
                return Err(Error::Validation(String::from("File name needs to be set")));
            }
        };

        if name.is_empty() || name.len() > MAX_FILE_NAME_CHARACTERS {
            return Err(Error::Validation(format!(
                "File name needs to have between 1 and {} characters",
                MAX_FILE_NAME_CHARACTERS
            )));
        }

        let detected_type = match file.detect_content_type().await.map_err(|e| {
            error!("Could not detect content type for file {name}: {e}");
            Error::Validation(String::from("Could not detect content type for file"))
        })? {
            Some(t) => t,
            None => {
                return Err(Error::Validation(String::from(
                    "Unknown file type detected",
                )))
            }
        };

        if !VALID_FILE_MIME_TYPES.contains(&detected_type.as_str()) {
            return Err(Error::Validation(String::from(
                "Invalid file type detected",
            )));
        }
        Ok(())
    }

    async fn upload_files(
        &self,
        files: Vec<&dyn util::file::UploadFileHandler>,
    ) -> Result<UploadFilesResponse> {
        // create a new random id
        let file_upload_id = util::get_uuid_v4().to_string();
        // create a folder to store the files
        self.file_upload_store
            .create_temp_upload_folder(&file_upload_id)
            .await?;
        // sanitize and randomize file name and write file into the temporary folder
        for file in files {
            let file_name = util::file::generate_unique_filename(
                &util::file::sanitize_filename(
                    &file
                        .name()
                        .ok_or(Error::Validation(String::from("Invalid file name")))?,
                ),
                file.extension(),
            );
            let read_file = file.get_contents().await.map_err(persistence::Error::Io)?;
            self.file_upload_store
                .write_temp_upload_file(&file_upload_id, &file_name, &read_file)
                .await?;
        }
        Ok(UploadFilesResponse { file_upload_id })
    }

    async fn get_temp_file(&self, file_upload_id: &str) -> Result<Option<(String, Vec<u8>)>> {
        let mut files = self
            .file_upload_store
            .read_temp_upload_files(file_upload_id)
            .await
            .map_err(|_| crate::service::Error::NoFileForFileUploadId)?;
        // return the first file in the folder
        if !files.is_empty() {
            let (file_name, file_bytes) = files.remove(0);
            return Ok(Some((file_name, file_bytes)));
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use persistence::file_upload::MockFileUploadStoreApi;
    use std::sync::Arc;
    use util::file::MockUploadFileHandler;

    fn get_service(mock_storage: MockFileUploadStoreApi) -> FileUploadService {
        FileUploadService::new(Arc::new(mock_storage))
    }

    #[tokio::test]
    async fn upload_files_baseline() {
        let file_bytes = String::from("hello world").as_bytes().to_vec();
        let mut storage = MockFileUploadStoreApi::new();
        storage
            .expect_write_temp_upload_file()
            .returning(|_, _, _| Ok(()));
        storage
            .expect_create_temp_upload_folder()
            .returning(|_| Ok(()));
        let mut file = MockUploadFileHandler::new();
        file.expect_name()
            .returning(|| Some(String::from("invoice")));
        file.expect_extension()
            .returning(|| Some(String::from("pdf")));
        file.expect_get_contents()
            .returning(move || Ok(file_bytes.clone()));
        let service = get_service(storage);

        let res = service.upload_files(vec![&file]).await;
        assert!(res.is_ok());
        assert_eq!(
            res.unwrap().file_upload_id,
            "00000000-0000-0000-0000-000000000000".to_owned()
        );
    }

    #[tokio::test]
    async fn upload_files_baseline_fails_on_folder_creation() {
        let file_bytes = String::from("hello world").as_bytes().to_vec();
        let mut storage = MockFileUploadStoreApi::new();
        storage.expect_create_temp_upload_folder().returning(|_| {
            Err(persistence::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "test error",
            )))
        });
        let mut file = MockUploadFileHandler::new();
        file.expect_name()
            .returning(|| Some(String::from("invoice")));
        file.expect_extension()
            .returning(|| Some(String::from("pdf")));
        file.expect_get_contents()
            .returning(move || Ok(file_bytes.clone()));
        let service = get_service(storage);

        let res = service.upload_files(vec![&file]).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn upload_files_baseline_fails_on_file_creation() {
        let mut storage = MockFileUploadStoreApi::new();
        storage
            .expect_create_temp_upload_folder()
            .returning(|_| Ok(()));
        let mut file = MockUploadFileHandler::new();
        file.expect_name()
            .returning(|| Some(String::from("invoice")));
        file.expect_extension()
            .returning(|| Some(String::from("pdf")));
        file.expect_get_contents()
            .returning(|| Err(std::io::Error::new(std::io::ErrorKind::Other, "test error")));
        let service = get_service(storage);

        let res = service.upload_files(vec![&file]).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn upload_files_baseline_fails_on_file_name_errors() {
        let mut storage = MockFileUploadStoreApi::new();
        storage
            .expect_create_temp_upload_folder()
            .returning(|_| Ok(()));
        let mut file = MockUploadFileHandler::new();
        file.expect_name().returning(|| None);
        let service = get_service(storage);

        let res = service.upload_files(vec![&file]).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn upload_files_baseline_fails_on_file_read_errors() {
        let file_bytes = String::from("hello world").as_bytes().to_vec();
        let mut storage = MockFileUploadStoreApi::new();
        storage
            .expect_write_temp_upload_file()
            .returning(|_, _, _| {
                Err(persistence::Error::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "test error",
                )))
            });
        storage
            .expect_create_temp_upload_folder()
            .returning(|_| Ok(()));
        let mut file = MockUploadFileHandler::new();
        file.expect_name()
            .returning(|| Some(String::from("invoice")));
        file.expect_extension()
            .returning(|| Some(String::from("pdf")));
        file.expect_get_contents()
            .returning(move || Ok(file_bytes.clone()));
        let service = get_service(storage);

        let res = service.upload_files(vec![&file]).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn validate_attached_file_checks_file_size() {
        let mut file = MockUploadFileHandler::new();
        file.expect_len()
            .returning(move || MAX_FILE_SIZE_BYTES as u64 * 2);

        let service = get_service(MockFileUploadStoreApi::new());
        let res = service.validate_attached_file(&file).await;

        assert!(res.is_err());
    }

    #[tokio::test]
    async fn validate_attached_file_checks_file_name() {
        let mut file = MockUploadFileHandler::new();
        file.expect_len().returning(move || 100);
        file.expect_name().returning(move || None);

        let service = get_service(MockFileUploadStoreApi::new());
        let res = service.validate_attached_file(&file).await;

        assert!(res.is_err());
    }

    #[tokio::test]
    async fn validate_attached_file_checks_file_name_empty() {
        let mut file = MockUploadFileHandler::new();
        file.expect_len().returning(move || 100);
        file.expect_name().returning(move || Some(String::from("")));

        let service = get_service(MockFileUploadStoreApi::new());
        let res = service.validate_attached_file(&file).await;

        assert!(res.is_err());
    }

    #[tokio::test]
    async fn validate_attached_file_checks_file_name_length() {
        let mut file = MockUploadFileHandler::new();
        file.expect_len().returning(move || 100);
        file.expect_name()
            .returning(move || Some("abc".repeat(100)));

        let service = get_service(MockFileUploadStoreApi::new());
        let res = service.validate_attached_file(&file).await;

        assert!(res.is_err());
    }

    #[tokio::test]
    async fn validate_attached_file_checks_file_type_error() {
        let mut file = MockUploadFileHandler::new();
        file.expect_len().returning(move || 100);
        file.expect_name()
            .returning(move || Some(String::from("goodname")));
        file.expect_detect_content_type()
            .returning(move || Err(std::io::Error::new(std::io::ErrorKind::Other, "test error")));

        let service = get_service(MockFileUploadStoreApi::new());
        let res = service.validate_attached_file(&file).await;

        assert!(res.is_err());
    }

    #[tokio::test]
    async fn validate_attached_file_checks_file_type_invalid() {
        let mut file = MockUploadFileHandler::new();
        file.expect_len().returning(move || 100);
        file.expect_name()
            .returning(move || Some(String::from("goodname")));
        file.expect_detect_content_type()
            .returning(move || Ok(None));

        let service = get_service(MockFileUploadStoreApi::new());
        let res = service.validate_attached_file(&file).await;

        assert!(res.is_err());
    }

    #[tokio::test]
    async fn validate_attached_file_checks_file_type_not_in_list() {
        let mut file = MockUploadFileHandler::new();
        file.expect_len().returning(move || 100);
        file.expect_name()
            .returning(move || Some(String::from("goodname")));
        file.expect_detect_content_type()
            .returning(move || Ok(Some(String::from("invalidfile"))));

        let service = get_service(MockFileUploadStoreApi::new());
        let res = service.validate_attached_file(&file).await;

        assert!(res.is_err());
    }

    #[tokio::test]
    async fn validate_attached_file_checks_valid() {
        let mut file = MockUploadFileHandler::new();
        file.expect_len().returning(move || 100);
        file.expect_name()
            .returning(move || Some(String::from("goodname")));
        file.expect_detect_content_type()
            .returning(move || Ok(Some(String::from("application/pdf"))));

        let service = get_service(MockFileUploadStoreApi::new());
        let res = service.validate_attached_file(&file).await;

        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn get_temp_file_baseline() {
        let mut storage = MockFileUploadStoreApi::new();
        storage.expect_read_temp_upload_files().returning(|_| {
            Ok(vec![(
                "some_file".to_string(),
                "hello_world".as_bytes().to_vec(),
            )])
        });
        let service = get_service(storage);

        let res = service.get_temp_file("1234").await;
        assert!(res.is_ok());
        assert_eq!(
            res.unwrap(),
            Some(("some_file".to_string(), "hello_world".as_bytes().to_vec()))
        );
    }

    #[tokio::test]
    async fn get_temp_file_no_file() {
        let mut storage = MockFileUploadStoreApi::new();
        storage
            .expect_read_temp_upload_files()
            .returning(|_| Ok(vec![]));
        let service = get_service(storage);

        let res = service.get_temp_file("1234").await;
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), None);
    }

    #[tokio::test]
    async fn get_temp_file_err() {
        let mut storage = MockFileUploadStoreApi::new();
        storage.expect_read_temp_upload_files().returning(|_| {
            Err(persistence::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "test error",
            )))
        });
        let service = get_service(storage);

        let res = service.get_temp_file("1234").await;
        assert!(res.is_err());
    }
}
