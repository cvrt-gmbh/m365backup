pub mod local;
pub mod s3;

use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait Backend: Send + Sync {
    async fn read(&self, path: &str) -> Result<Vec<u8>>;
    async fn write(&self, path: &str, data: &[u8]) -> Result<()>;
    async fn exists(&self, path: &str) -> Result<bool>;
    async fn list(&self, prefix: &str) -> Result<Vec<String>>;
    async fn delete(&self, path: &str) -> Result<()>;
}
