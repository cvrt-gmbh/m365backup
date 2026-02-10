use anyhow::{Context, Result};
use async_trait::async_trait;
use s3::bucket::Bucket;
use s3::creds::Credentials;
use s3::region::Region;

use super::Backend;

pub struct S3Backend {
    bucket: Box<Bucket>,
    prefix: String,
}

impl S3Backend {
    pub async fn new(
        bucket_name: &str,
        endpoint: &str,
        region: &str,
        access_key: &str,
        secret_key: &str,
        prefix: Option<&str>,
    ) -> Result<Self> {
        let region = Region::Custom {
            region: region.to_string(),
            endpoint: endpoint.to_string(),
        };
        let credentials = Credentials::new(Some(access_key), Some(secret_key), None, None, None)?;
        let bucket = Bucket::new(bucket_name, region, credentials)?.with_path_style();
        let prefix = prefix.unwrap_or("").to_string();
        Ok(Self { bucket, prefix })
    }

    fn full_path(&self, path: &str) -> String {
        if self.prefix.is_empty() {
            path.to_string()
        } else {
            format!("{}/{path}", self.prefix)
        }
    }
}

#[async_trait]
impl Backend for S3Backend {
    async fn read(&self, path: &str) -> Result<Vec<u8>> {
        let full = self.full_path(path);
        let response = self
            .bucket
            .get_object(&full)
            .await
            .with_context(|| format!("S3 GET failed: {full}"))?;
        Ok(response.to_vec())
    }

    async fn write(&self, path: &str, data: &[u8]) -> Result<()> {
        let full = self.full_path(path);
        self.bucket
            .put_object(&full, data)
            .await
            .with_context(|| format!("S3 PUT failed: {full}"))?;
        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let full = self.full_path(path);
        match self.bucket.head_object(&full).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let full = self.full_path(prefix);
        let results = self
            .bucket
            .list(full.clone(), Some("/".to_string()))
            .await
            .with_context(|| format!("S3 LIST failed: {full}"))?;
        let mut paths = Vec::new();
        for result in results {
            for obj in result.contents {
                if let Some(stripped) = obj.key.strip_prefix(&format!("{}/", self.prefix)) {
                    paths.push(stripped.to_string());
                } else {
                    paths.push(obj.key);
                }
            }
        }
        paths.sort();
        Ok(paths)
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let full = self.full_path(path);
        self.bucket
            .delete_object(&full)
            .await
            .with_context(|| format!("S3 DELETE failed: {full}"))?;
        Ok(())
    }
}
