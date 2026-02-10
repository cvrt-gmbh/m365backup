use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use async_trait::async_trait;

use super::Backend;

pub struct LocalBackend {
    root: PathBuf,
}

impl LocalBackend {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let root = path.as_ref().to_path_buf();
        Ok(Self { root })
    }

    pub fn init(path: impl AsRef<Path>) -> Result<Self> {
        let root = path.as_ref().to_path_buf();
        std::fs::create_dir_all(&root)
            .with_context(|| format!("failed to create directory: {}", root.display()))?;
        Ok(Self { root })
    }

    fn full_path(&self, path: &str) -> PathBuf {
        self.root.join(path)
    }
}

#[async_trait]
impl Backend for LocalBackend {
    async fn read(&self, path: &str) -> Result<Vec<u8>> {
        let full = self.full_path(path);
        tokio::fs::read(&full)
            .await
            .with_context(|| format!("failed to read: {}", full.display()))
    }

    async fn write(&self, path: &str, data: &[u8]) -> Result<()> {
        let full = self.full_path(path);
        if let Some(parent) = full.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&full, data)
            .await
            .with_context(|| format!("failed to write: {}", full.display()))
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let full = self.full_path(path);
        Ok(tokio::fs::try_exists(&full).await.unwrap_or(false))
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let dir = self.full_path(prefix);
        if !dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut entries = Vec::new();
        let mut read_dir = tokio::fs::read_dir(&dir).await?;
        while let Some(entry) = read_dir.next_entry().await? {
            if let Some(name) = entry.file_name().to_str() {
                entries.push(format!("{prefix}/{name}"));
            }
        }
        entries.sort();
        Ok(entries)
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let full = self.full_path(path);
        if full.is_file() {
            tokio::fs::remove_file(&full).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_backend_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::init(dir.path()).unwrap();

        backend.write("test/hello.txt", b"world").await.unwrap();
        assert!(backend.exists("test/hello.txt").await.unwrap());

        let data = backend.read("test/hello.txt").await.unwrap();
        assert_eq!(data, b"world");

        let list = backend.list("test").await.unwrap();
        assert_eq!(list, vec!["test/hello.txt"]);

        backend.delete("test/hello.txt").await.unwrap();
        assert!(!backend.exists("test/hello.txt").await.unwrap());
    }
}
