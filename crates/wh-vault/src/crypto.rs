//! Argon2id key derivation and XChaCha20-Poly1305 sealing.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use chacha20poly1305::aead::rand_core::RngCore;
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::Zeroizing;

pub const KEY_LEN: usize = 32;
pub const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
pub type Key = Zeroizing<[u8; KEY_LEN]>;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CryptoError {
    /// Wrong key, wrong AAD, or tampered ciphertext. Deliberately uninformative.
    #[error("decryption failed")]
    Decrypt,
    #[error("key derivation failed")]
    Kdf,
}

/// Argon2id cost parameters, stored alongside the salt so they can be raised later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    pub mem_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        // OWASP recommendation for Argon2id.
        Self { mem_kib: 64 * 1024, iterations: 3, parallelism: 1 }
    }
}

impl KdfParams {
    /// Upper bounds for parameters read from files, so a crafted vault or
    /// export cannot make us allocate unbounded memory or spin forever.
    pub fn is_sane(&self) -> bool {
        (8..=1024 * 1024).contains(&self.mem_kib)
            && (1..=10).contains(&self.iterations)
            && (1..=16).contains(&self.parallelism)
    }
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    OsRng.fill_bytes(&mut out);
    out
}

pub fn derive_key(password: &[u8], salt: &[u8; SALT_LEN], params: KdfParams) -> Result<Key, CryptoError> {
    let params = Params::new(params.mem_kib, params.iterations, params.parallelism, Some(KEY_LEN))
        .map_err(|_| CryptoError::Kdf)?;
    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password, salt, key.as_mut())
        .map_err(|_| CryptoError::Kdf)?;
    Ok(key)
}

/// Returns `nonce || ciphertext`. `aad` is authenticated but not encrypted.
pub fn seal(key: &Key, plaintext: &[u8], aad: &[u8]) -> Vec<u8> {
    let cipher = XChaCha20Poly1305::new(key.as_ref().into());
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ct = cipher
        .encrypt(&nonce, Payload { msg: plaintext, aad })
        .expect("XChaCha20-Poly1305 encryption cannot fail for in-memory buffers");
    let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    out
}

pub fn open(key: &Key, sealed: &[u8], aad: &[u8]) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    if sealed.len() < NONCE_LEN {
        return Err(CryptoError::Decrypt);
    }
    let (nonce, ct) = sealed.split_at(NONCE_LEN);
    XChaCha20Poly1305::new(key.as_ref().into())
        .decrypt(XNonce::from_slice(nonce), Payload { msg: ct, aad })
        .map(Zeroizing::new)
        .map_err(|_| CryptoError::Decrypt)
}

#[cfg(test)]
pub(crate) const TEST_KDF: KdfParams = KdfParams { mem_kib: 256, iterations: 1, parallelism: 1 };

#[cfg(test)]
mod tests {
    use super::*;

    fn key(pw: &str) -> Key {
        derive_key(pw.as_bytes(), &[7; SALT_LEN], TEST_KDF).unwrap()
    }

    #[test]
    fn derive_key_is_deterministic_per_password_and_salt() {
        assert_eq!(*key("hunter2"), *key("hunter2"));
        assert_ne!(*key("hunter2"), *key("hunter3"));
        let other_salt = derive_key(b"hunter2", &[8; SALT_LEN], TEST_KDF).unwrap();
        assert_ne!(*key("hunter2"), *other_salt);
    }

    #[test]
    fn seal_open_roundtrip() {
        let k = key("pw");
        let sealed = seal(&k, b"secret", b"aad");
        assert_eq!(open(&k, &sealed, b"aad").unwrap().as_slice(), b"secret");
    }

    #[test]
    fn sealing_twice_uses_fresh_nonces() {
        let k = key("pw");
        assert_ne!(seal(&k, b"same", b""), seal(&k, b"same", b""));
    }

    #[test]
    fn open_rejects_wrong_key_wrong_aad_and_tampering() {
        let k = key("pw");
        let mut sealed = seal(&k, b"secret", b"aad");
        assert_eq!(open(&key("nope"), &sealed, b"aad"), Err(CryptoError::Decrypt));
        assert_eq!(open(&k, &sealed, b"other"), Err(CryptoError::Decrypt));
        let last = sealed.len() - 1;
        sealed[last] ^= 1;
        assert_eq!(open(&k, &sealed, b"aad"), Err(CryptoError::Decrypt));
        assert_eq!(open(&k, &[1, 2, 3], b"aad"), Err(CryptoError::Decrypt), "short input must not panic");
    }

    #[test]
    fn default_and_test_params_are_sane() {
        assert!(KdfParams::default().is_sane());
        assert!(TEST_KDF.is_sane());
        assert!(!KdfParams { mem_kib: u32::MAX, ..TEST_KDF }.is_sane());
    }

    #[test]
    fn random_bytes_differ() {
        assert_ne!(random_bytes::<32>(), random_bytes::<32>());
    }
}
