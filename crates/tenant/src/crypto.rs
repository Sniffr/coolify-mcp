use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, AeadCore, KeyInit, OsRng, Payload},
};
use secrecy::{ExposeSecret, SecretString};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

use crate::{TenantError, UserId};

const NONCE_LEN: usize = 12;
const MIN_KEY_MATERIAL_BYTES: usize = 32;

pub(crate) struct EncryptionKey {
    key: [u8; 32],
}

impl Drop for EncryptionKey {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

impl EncryptionKey {
    pub(crate) fn from_material(material: &str) -> Result<Self, TenantError> {
        if material.len() < MIN_KEY_MATERIAL_BYTES {
            return Err(TenantError::InvalidKey);
        }

        let digest = Sha256::digest(material.as_bytes());
        let mut key = [0_u8; 32];
        key.copy_from_slice(&digest);
        Ok(Self { key })
    }

    pub(crate) fn encrypt(
        &self,
        user_id: UserId,
        token: &SecretString,
    ) -> Result<Vec<u8>, TenantError> {
        let cipher = Aes256Gcm::new_from_slice(&self.key).map_err(|_| TenantError::Crypto)?;
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let mut plaintext = token.expose_secret().as_bytes().to_vec();
        let associated_data = Self::associated_data(user_id);
        let ciphertext = match cipher.encrypt(
            &nonce,
            Payload {
                msg: plaintext.as_ref(),
                aad: associated_data.as_bytes(),
            },
        ) {
            Ok(ciphertext) => ciphertext,
            Err(_) => {
                plaintext.zeroize();
                return Err(TenantError::Crypto);
            }
        };
        plaintext.zeroize();

        let mut stored = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        stored.extend_from_slice(nonce.as_slice());
        stored.extend_from_slice(&ciphertext);
        Ok(stored)
    }

    pub(crate) fn decrypt(
        &self,
        user_id: UserId,
        stored: &[u8],
    ) -> Result<SecretString, TenantError> {
        if stored.len() <= NONCE_LEN {
            return Err(TenantError::Decryption);
        }

        let cipher = Aes256Gcm::new_from_slice(&self.key).map_err(|_| TenantError::Decryption)?;
        let nonce = Nonce::from_slice(&stored[..NONCE_LEN]);
        let associated_data = Self::associated_data(user_id);
        let mut plaintext = cipher
            .decrypt(
                nonce,
                Payload {
                    msg: &stored[NONCE_LEN..],
                    aad: associated_data.as_bytes(),
                },
            )
            .map_err(|_| TenantError::Decryption)?;
        let text = match std::str::from_utf8(&plaintext) {
            Ok(text) => text,
            Err(_) => {
                plaintext.zeroize();
                return Err(TenantError::Decryption);
            }
        };
        let token = SecretString::new(text.to_owned().into());
        plaintext.zeroize();
        Ok(token)
    }

    pub(crate) fn encrypt_blob(
        &self,
        label: &str,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, TenantError> {
        let cipher = Aes256Gcm::new_from_slice(&self.key).map_err(|_| TenantError::Crypto)?;
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext,
                    aad: label.as_bytes(),
                },
            )
            .map_err(|_| TenantError::Crypto)?;
        let mut stored = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        stored.extend_from_slice(nonce.as_slice());
        stored.extend_from_slice(&ciphertext);
        Ok(stored)
    }

    pub(crate) fn decrypt_blob(&self, label: &str, stored: &[u8]) -> Result<Vec<u8>, TenantError> {
        if stored.len() <= NONCE_LEN {
            return Err(TenantError::Decryption);
        }
        let cipher = Aes256Gcm::new_from_slice(&self.key).map_err(|_| TenantError::Decryption)?;
        cipher
            .decrypt(
                Nonce::from_slice(&stored[..NONCE_LEN]),
                Payload {
                    msg: &stored[NONCE_LEN..],
                    aad: label.as_bytes(),
                },
            )
            .map_err(|_| TenantError::Decryption)
    }

    pub(crate) fn associated_data(user_id: UserId) -> String {
        format!("{user_id}:coolify-default")
    }
}
