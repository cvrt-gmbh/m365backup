use aes_gcm::aead::generic_array::GenericArray;
use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm};
use anyhow::Result;
use argon2::Argon2;
use serde::{Deserialize, Serialize};

const ARGON2_SALT_LEN: usize = 16;
const AES_KEY_LEN: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyConfig {
    pub salt: Vec<u8>,
    pub nonce: Vec<u8>,
    pub encrypted_master_key: Vec<u8>,
}

pub struct CryptoEngine {
    cipher: Aes256Gcm,
}

impl CryptoEngine {
    pub fn new(master_key: &[u8; AES_KEY_LEN]) -> Self {
        let cipher = Aes256Gcm::new(GenericArray::from_slice(master_key));
        Self { cipher }
    }

    pub fn from_passphrase(passphrase: &str, config: &KeyConfig) -> Result<Self> {
        let master_key = decrypt_master_key(passphrase, config)?;
        Ok(Self::new(&master_key))
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = self
            .cipher
            .encrypt(&nonce, plaintext)
            .map_err(|e| anyhow::anyhow!("encryption failed: {e}"))?;
        let mut out = nonce.to_vec();
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < 12 {
            anyhow::bail!("ciphertext too short");
        }
        let (nonce_bytes, ciphertext) = data.split_at(12);
        let nonce = GenericArray::from_slice(nonce_bytes);
        self.cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| anyhow::anyhow!("decryption failed: {e}"))
    }
}

pub fn create_key_config(passphrase: &str) -> Result<(KeyConfig, [u8; AES_KEY_LEN])> {
    let mut salt = vec![0u8; ARGON2_SALT_LEN];
    use rand::RngCore;
    OsRng.fill_bytes(&mut salt);

    let mut master_key = [0u8; AES_KEY_LEN];
    OsRng.fill_bytes(&mut master_key);

    let mut derived = [0u8; AES_KEY_LEN];
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), &salt, &mut derived)
        .map_err(|e| anyhow::anyhow!("key derivation failed: {e}"))?;

    let wrapping_cipher = Aes256Gcm::new(GenericArray::from_slice(&derived));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let encrypted_master_key = wrapping_cipher
        .encrypt(&nonce, master_key.as_slice())
        .map_err(|e| anyhow::anyhow!("key wrapping failed: {e}"))?;

    let config = KeyConfig {
        salt,
        nonce: nonce.to_vec(),
        encrypted_master_key,
    };
    Ok((config, master_key))
}

fn decrypt_master_key(passphrase: &str, config: &KeyConfig) -> Result<[u8; AES_KEY_LEN]> {
    let mut derived = [0u8; AES_KEY_LEN];
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), &config.salt, &mut derived)
        .map_err(|e| anyhow::anyhow!("key derivation failed: {e}"))?;

    let wrapping_cipher = Aes256Gcm::new(GenericArray::from_slice(&derived));
    let nonce = GenericArray::from_slice(&config.nonce);
    let master_key_bytes = wrapping_cipher
        .decrypt(nonce, config.encrypted_master_key.as_slice())
        .map_err(|_| anyhow::anyhow!("wrong passphrase or corrupted key config"))?;

    let master_key: [u8; AES_KEY_LEN] = master_key_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid master key length"))?;
    Ok(master_key)
}

pub fn hash_blake3(data: &[u8]) -> [u8; 32] {
    blake3::hash(data).into()
}

pub fn hash_blake3_hex(data: &[u8]) -> String {
    blake3::hash(data).to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_encryption() {
        let (config, master_key) = create_key_config("test-passphrase").unwrap();
        let engine = CryptoEngine::new(&master_key);
        let plaintext = b"hello world";
        let encrypted = engine.encrypt(plaintext).unwrap();
        let decrypted = engine.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);

        let engine2 = CryptoEngine::from_passphrase("test-passphrase", &config).unwrap();
        let decrypted2 = engine2.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted2, plaintext);
    }

    #[test]
    fn wrong_passphrase_fails() {
        let (config, _) = create_key_config("correct").unwrap();
        let result = CryptoEngine::from_passphrase("wrong", &config);
        assert!(result.is_err());
    }
}
