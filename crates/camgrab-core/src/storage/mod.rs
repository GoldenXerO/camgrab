use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// Result of a storage operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageResult {
    pub key: String,
    pub size_bytes: u64,
    pub timestamp: DateTime<Utc>,
    pub backend_name: String,
}

/// Entry in storage list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageEntry {
    pub key: String,
    pub size_bytes: u64,
    pub last_modified: DateTime<Utc>,
}

/// Errors that can occur during storage operations
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("S3 error: {0}")]
    S3Error(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),
}

/// Trait for storage backends
#[async_trait::async_trait]
pub trait StorageBackend: Send + Sync {
    /// Store data with the given key
    async fn store(&self, key: &str, data: &[u8]) -> Result<StorageResult, StorageError>;

    /// Retrieve data by key
    async fn retrieve(&self, key: &str) -> Result<Vec<u8>, StorageError>;

    /// Delete data by key
    async fn delete(&self, key: &str) -> Result<(), StorageError>;

    /// List all entries with the given prefix
    async fn list(&self, prefix: &str) -> Result<Vec<StorageEntry>, StorageError>;

    /// Get the name of this storage backend
    fn name(&self) -> &str;
}

/// Local filesystem storage backend
pub struct LocalStorage {
    base_path: PathBuf,
}

impl LocalStorage {
    pub async fn new(base_path: PathBuf) -> Result<Self, StorageError> {
        // Create base directory if it doesn't exist
        fs::create_dir_all(&base_path).await?;

        Ok(Self { base_path })
    }

    fn get_full_path(&self, key: &str) -> PathBuf {
        self.base_path.join(key)
    }
}

#[async_trait::async_trait]
impl StorageBackend for LocalStorage {
    async fn store(&self, key: &str, data: &[u8]) -> Result<StorageResult, StorageError> {
        let full_path = self.get_full_path(key);

        // Create parent directories if needed
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Atomic write: write to temp file, then rename
        let temp_path = full_path.with_extension("tmp");

        let mut file = fs::File::create(&temp_path).await?;
        file.write_all(data).await?;
        file.sync_all().await?;
        drop(file);

        fs::rename(&temp_path, &full_path).await?;

        let metadata = fs::metadata(&full_path).await?;

        Ok(StorageResult {
            key: key.to_string(),
            size_bytes: metadata.len(),
            timestamp: Utc::now(),
            backend_name: self.name().to_string(),
        })
    }

    async fn retrieve(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        let full_path = self.get_full_path(key);

        if !full_path.exists() {
            return Err(StorageError::NotFound(key.to_string()));
        }

        let data = fs::read(&full_path).await?;
        Ok(data)
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let full_path = self.get_full_path(key);

        if !full_path.exists() {
            return Err(StorageError::NotFound(key.to_string()));
        }

        fs::remove_file(&full_path).await?;
        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<StorageEntry>, StorageError> {
        let prefix_path = self.base_path.join(prefix);

        if !prefix_path.exists() {
            return Ok(Vec::new());
        }

        let mut entries = Vec::new();
        let mut read_dir = fs::read_dir(&prefix_path).await?;

        while let Some(entry) = read_dir.next_entry().await? {
            let metadata = entry.metadata().await?;

            if metadata.is_file() {
                let key = entry
                    .path()
                    .strip_prefix(&self.base_path)
                    .map_err(|e| {
                        StorageError::IoError(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            e.to_string(),
                        ))
                    })?
                    .to_string_lossy()
                    .to_string();

                let modified = metadata.modified()?;
                let datetime: DateTime<Utc> = modified.into();

                entries.push(StorageEntry {
                    key,
                    size_bytes: metadata.len(),
                    last_modified: datetime,
                });
            }
        }

        entries.sort_by(|a, b| b.last_modified.cmp(&a.last_modified));

        Ok(entries)
    }

    fn name(&self) -> &str {
        "local"
    }
}

/// S3 storage backend
pub struct S3Storage {
    bucket: Box<s3::Bucket>,
    prefix: String,
}

impl S3Storage {
    pub fn new(
        bucket_name: String,
        region: String,
        prefix: String,
        access_key: Option<String>,
        secret_key: Option<String>,
    ) -> Result<Self, StorageError> {
        let region = region
            .parse::<s3::Region>()
            .map_err(|e| StorageError::S3Error(format!("Invalid region: {e}")))?;

        let credentials = if let (Some(access), Some(secret)) = (access_key, secret_key) {
            s3::creds::Credentials::new(Some(&access), Some(&secret), None, None, None)
                .map_err(|e| StorageError::S3Error(format!("Invalid credentials: {e}")))?
        } else {
            s3::creds::Credentials::default().map_err(|e| {
                StorageError::S3Error(format!("Failed to get default credentials: {e}"))
            })?
        };

        let bucket = s3::Bucket::new(&bucket_name, region, credentials)
            .map_err(|e| StorageError::S3Error(format!("Failed to create bucket: {e}")))?
            .with_path_style();

        Ok(Self { bucket, prefix })
    }

    fn get_s3_key(&self, key: &str) -> String {
        if self.prefix.is_empty() {
            key.to_string()
        } else {
            format!("{}/{}", self.prefix.trim_end_matches('/'), key)
        }
    }
}

#[async_trait::async_trait]
impl StorageBackend for S3Storage {
    async fn store(&self, key: &str, data: &[u8]) -> Result<StorageResult, StorageError> {
        let s3_key = self.get_s3_key(key);

        let response = self
            .bucket
            .put_object(&s3_key, data)
            .await
            .map_err(|e| StorageError::S3Error(format!("Failed to put object: {e}")))?;

        if response.status_code() != 200 {
            return Err(StorageError::S3Error(format!(
                "S3 returned status code: {}",
                response.status_code()
            )));
        }

        Ok(StorageResult {
            key: key.to_string(),
            size_bytes: data.len() as u64,
            timestamp: Utc::now(),
            backend_name: self.name().to_string(),
        })
    }

    async fn retrieve(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        let s3_key = self.get_s3_key(key);

        let response = self
            .bucket
            .get_object(&s3_key)
            .await
            .map_err(|e| StorageError::S3Error(format!("Failed to get object: {e}")))?;

        if response.status_code() == 404 {
            return Err(StorageError::NotFound(key.to_string()));
        }

        if response.status_code() != 200 {
            return Err(StorageError::S3Error(format!(
                "S3 returned status code: {}",
                response.status_code()
            )));
        }

        Ok(response.bytes().to_vec())
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let s3_key = self.get_s3_key(key);

        let response = self
            .bucket
            .delete_object(&s3_key)
            .await
            .map_err(|e| StorageError::S3Error(format!("Failed to delete object: {e}")))?;

        if response.status_code() == 404 {
            return Err(StorageError::NotFound(key.to_string()));
        }

        if response.status_code() != 204 && response.status_code() != 200 {
            return Err(StorageError::S3Error(format!(
                "S3 returned status code: {}",
                response.status_code()
            )));
        }

        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<StorageEntry>, StorageError> {
        let s3_prefix = self.get_s3_key(prefix);

        let results = self
            .bucket
            .list(s3_prefix, None)
            .await
            .map_err(|e| StorageError::S3Error(format!("Failed to list objects: {e}")))?;

        let mut entries = Vec::new();

        for result in results {
            for object in result.contents {
                // Strip the prefix from the key
                let key = if self.prefix.is_empty() {
                    object.key.clone()
                } else {
                    object
                        .key
                        .strip_prefix(&format!("{}/", self.prefix.trim_end_matches('/')))
                        .unwrap_or(&object.key)
                        .to_string()
                };

                let last_modified = DateTime::parse_from_rfc3339(&object.last_modified)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());

                entries.push(StorageEntry {
                    key,
                    size_bytes: object.size,
                    last_modified,
                });
            }
        }

        entries.sort_by(|a, b| b.last_modified.cmp(&a.last_modified));

        Ok(entries)
    }

    fn name(&self) -> &str {
        "s3"
    }
}

/// Manager that coordinates multiple storage backends
pub struct StorageManager {
    backends: Vec<Box<dyn StorageBackend + Send + Sync>>,
}

impl StorageManager {
    pub fn new() -> Self {
        Self {
            backends: Vec::new(),
        }
    }

    /// Add a storage backend
    pub fn add_backend(&mut self, backend: Box<dyn StorageBackend + Send + Sync>) {
        self.backends.push(backend);
    }

    /// Generate a key for a snapshot with timestamp
    fn generate_snapshot_key(camera: &str, format: &str) -> String {
        let now = Utc::now();
        let date = now.format("%Y-%m-%d");
        let time = now.format("%H%M%S");
        format!("{camera}/{date}/snap_{time}.{format}")
    }

    /// Generate a key for a clip with timestamp
    fn generate_clip_key(camera: &str, format: &str) -> String {
        let now = Utc::now();
        let date = now.format("%Y-%m-%d");
        let time = now.format("%H%M%S");
        format!("{camera}/{date}/clip_{time}.{format}")
    }

    /// Store a snapshot to all backends
    pub async fn store_snapshot(
        &self,
        camera: &str,
        data: &[u8],
        format: &str,
    ) -> Result<Vec<StorageResult>, StorageError> {
        let key = Self::generate_snapshot_key(camera, format);
        let mut results = Vec::new();

        for backend in &self.backends {
            let result = backend.store(&key, data).await?;
            results.push(result);
        }

        Ok(results)
    }

    /// Store a clip to all backends
    pub async fn store_clip(
        &self,
        camera: &str,
        data: &[u8],
        format: &str,
    ) -> Result<Vec<StorageResult>, StorageError> {
        let key = Self::generate_clip_key(camera, format);
        let mut results = Vec::new();

        for backend in &self.backends {
            let result = backend.store(&key, data).await?;
            results.push(result);
        }

        Ok(results)
    }

    /// Get number of backends
    pub fn backend_count(&self) -> usize {
        self.backends.len()
    }
}

impl Default for StorageManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_local_storage_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        let key = "test/file.txt";
        let data = b"Hello, World!";

        // Store
        let result = storage.store(key, data).await.unwrap();
        assert_eq!(result.key, key);
        assert_eq!(result.size_bytes, data.len() as u64);
        assert_eq!(result.backend_name, "local");

        // Retrieve
        let retrieved = storage.retrieve(key).await.unwrap();
        assert_eq!(retrieved, data);

        // List
        let entries = storage.list("test").await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].size_bytes, data.len() as u64);

        // Delete
        storage.delete(key).await.unwrap();

        // Verify deleted
        let result = storage.retrieve(key).await;
        assert!(matches!(result, Err(StorageError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_local_storage_nested_paths() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        let key = "camera1/2024-01-15/snap_143022.jpg";
        let data = b"image data";

        let result = storage.store(key, data).await.unwrap();
        assert_eq!(result.key, key);

        let retrieved = storage.retrieve(key).await.unwrap();
        assert_eq!(retrieved, data);
    }

    #[tokio::test]
    async fn test_local_storage_atomic_write() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        let key = "test/atomic.txt";
        let data = b"atomic data";

        storage.store(key, data).await.unwrap();

        // Verify no .tmp files remain
        let full_path = storage.get_full_path(key);
        let temp_path = full_path.with_extension("tmp");
        assert!(!temp_path.exists());
        assert!(full_path.exists());
    }

    #[tokio::test]
    async fn test_storage_manager_snapshot_key_generation() {
        let key = StorageManager::generate_snapshot_key("front-door", "jpg");
        assert!(key.contains("front-door"));
        assert!(key.contains("snap_"));
        assert!(key.ends_with(".jpg"));

        // Check date format YYYY-MM-DD
        let parts: Vec<&str> = key.split('/').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], "front-door");
        assert_eq!(parts[1].len(), 10); // YYYY-MM-DD
    }

    #[tokio::test]
    async fn test_storage_manager_clip_key_generation() {
        let key = StorageManager::generate_clip_key("back-yard", "mp4");
        assert!(key.contains("back-yard"));
        assert!(key.contains("clip_"));
        assert!(key.ends_with(".mp4"));
    }

    #[tokio::test]
    async fn test_storage_manager_multiple_backends() {
        let temp_dir1 = TempDir::new().unwrap();
        let temp_dir2 = TempDir::new().unwrap();

        let storage1 = LocalStorage::new(temp_dir1.path().to_path_buf())
            .await
            .unwrap();
        let storage2 = LocalStorage::new(temp_dir2.path().to_path_buf())
            .await
            .unwrap();

        let mut manager = StorageManager::new();
        manager.add_backend(Box::new(storage1));
        manager.add_backend(Box::new(storage2));

        assert_eq!(manager.backend_count(), 2);

        let data = b"test snapshot data";
        let results = manager
            .store_snapshot("test-camera", data, "jpg")
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].backend_name, "local");
        assert_eq!(results[1].backend_name, "local");
    }

    #[tokio::test]
    async fn test_local_storage_list_empty() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        let entries = storage.list("nonexistent").await.unwrap();
        assert_eq!(entries.len(), 0);
    }

    #[tokio::test]
    async fn test_local_storage_delete_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        let result = storage.delete("nonexistent/file.txt").await;
        assert!(matches!(result, Err(StorageError::NotFound(_))));
    }
}
