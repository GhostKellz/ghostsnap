use crate::{Error, Result};
use argon2::{Argon2, PasswordHasher};
use argon2::password_hash::{rand_core::OsRng, SaltString};
use chacha20poly1305::{
    aead::{Aead, AeadCore, KeyInit, OsRng as AeadOsRng},
    ChaCha20Poly1305, Key, Nonce,
};
use rand::RngCore;

pub struct MasterKey {
    key: Vec<u8>,
}

impl MasterKey {
    pub fn derive_from_password(password: &str, salt: &[u8], params: &crate::KdfParams) -> Result<Self> {
        let argon2 = Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            argon2::Params::new(
                params.memory,
                params.iterations,
                params.parallelism,
                None,
            ).map_err(|e| Error::Encryption(e.to_string()))?,
        );
        
        let salt_str = SaltString::encode_b64(salt)
            .map_err(|e| Error::Encryption(e.to_string()))?;
        
        let password_hash = argon2
            .hash_password(password.as_bytes(), &salt_str)
            .map_err(|e| Error::Encryption(e.to_string()))?;
        
        let hash = password_hash.hash.unwrap();
        Ok(Self {
            key: hash.as_bytes().to_vec(),
        })
    }
    
    pub fn generate() -> Self {
        let mut key = vec![0u8; 32];
        OsRng.fill_bytes(&mut key);
        Self { key }
    }
    
    pub fn as_bytes(&self) -> &[u8] {
        &self.key
    }
}

pub struct Encryptor {
    cipher: ChaCha20Poly1305,
}

impl Encryptor {
    pub fn new(key: &[u8]) -> Result<Self> {
        if key.len() != 32 {
            return Err(Error::Encryption("Key must be 32 bytes".to_string()));
        }
        
        let key = Key::from_slice(key);
        let cipher = ChaCha20Poly1305::new(key);
        Ok(Self { cipher })
    }
    
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let nonce = ChaCha20Poly1305::generate_nonce(&mut AeadOsRng);
        let ciphertext = self.cipher
            .encrypt(&nonce, plaintext)
            .map_err(|e| Error::Encryption(e.to_string()))?;
        
        let mut result = nonce.to_vec();
        result.extend_from_slice(&ciphertext);
        Ok(result)
    }
    
    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        if ciphertext.len() < 12 {
            return Err(Error::Encryption("Ciphertext too short".to_string()));
        }
        
        let (nonce_bytes, encrypted) = ciphertext.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        
        self.cipher
            .decrypt(nonce, encrypted)
            .map_err(|e| Error::Encryption(e.to_string()))
    }
}

pub fn hash_data(data: &[u8]) -> crate::ChunkID {
    crate::ChunkID::from(blake3::hash(data))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_encryption_roundtrip() {
        let key = MasterKey::generate();
        let encryptor = Encryptor::new(key.as_bytes()).unwrap();
        
        let plaintext = b"Hello, Ghostsnap!";
        let ciphertext = encryptor.encrypt(plaintext).unwrap();
        let decrypted = encryptor.decrypt(&ciphertext).unwrap();
        
        assert_eq!(plaintext.to_vec(), decrypted);
    }
}