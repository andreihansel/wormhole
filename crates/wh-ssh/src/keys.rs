//! Keychain operations: generate, import and inspect SSH keys.

use russh::keys::ssh_key::private::{KeypairData, RsaKeypair};
use russh::keys::ssh_key::LineEnding;
use russh::keys::{decode_secret_key, Algorithm, Certificate, HashAlg, PrivateKey, PublicKey};
use thiserror::Error;
use zeroize::Zeroizing;

const MIN_RSA_BITS: usize = 2048;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum KeyError {
    #[error("this key is encrypted; a passphrase is required")]
    NeedsPassphrase,
    #[error("incorrect passphrase")]
    BadPassphrase,
    #[error("unrecognized or unsupported key format")]
    Unsupported,
    #[error("certificate does not belong to this key")]
    CertificateMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyType {
    Ed25519,
    Rsa { bits: usize },
}

/// A decoded key ready for the vault. `private_openssh` is unencrypted
/// OpenSSH format; the vault encrypts it at rest.
#[derive(Debug)]
pub struct KeyMaterial {
    pub public_openssh: String,
    pub fingerprint: String,
    pub private_openssh: Zeroizing<String>,
}

pub fn generate(kind: KeyType, comment: &str) -> Result<KeyMaterial, KeyError> {
    let mut rng = rand::rng();
    let mut key = match kind {
        KeyType::Ed25519 => PrivateKey::random(&mut rng, Algorithm::Ed25519),
        KeyType::Rsa { bits } if bits >= MIN_RSA_BITS => RsaKeypair::random(&mut rng, bits)
            .and_then(|kp| PrivateKey::try_from(KeypairData::from(kp))),
        KeyType::Rsa { .. } => return Err(KeyError::Unsupported),
    }
    .map_err(|_| KeyError::Unsupported)?;
    key.set_comment(comment);
    material(&key)
}

/// Accepts OpenSSH, PKCS#1 (`BEGIN RSA PRIVATE KEY`) and PKCS#8 keys.
pub fn import_private(text: &str, passphrase: Option<&str>) -> Result<KeyMaterial, KeyError> {
    let key = decode_secret_key(text, passphrase).map_err(|e| match e {
        russh::keys::Error::KeyIsEncrypted => KeyError::NeedsPassphrase,
        _ if passphrase.is_some() => KeyError::BadPassphrase,
        _ => KeyError::Unsupported,
    })?;
    material(&key)
}

/// Validates an OpenSSH certificate and checks it certifies `public_openssh`.
pub fn check_certificate(public_openssh: &str, cert_openssh: &str) -> Result<(), KeyError> {
    let public = PublicKey::from_openssh(public_openssh.trim()).map_err(|_| KeyError::Unsupported)?;
    let cert = Certificate::from_openssh(cert_openssh.trim()).map_err(|_| KeyError::Unsupported)?;
    if cert.public_key() == public.key_data() {
        Ok(())
    } else {
        Err(KeyError::CertificateMismatch)
    }
}

/// Parses a stored (unencrypted OpenSSH) private key for authentication.
pub fn load_stored(private_openssh: &str) -> Result<PrivateKey, KeyError> {
    PrivateKey::from_openssh(private_openssh).map_err(|_| KeyError::Unsupported)
}

fn material(key: &PrivateKey) -> Result<KeyMaterial, KeyError> {
    Ok(KeyMaterial {
        public_openssh: key.public_key().to_openssh().map_err(|_| KeyError::Unsupported)?,
        fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
        private_openssh: key.to_openssh(LineEnding::LF).map_err(|_| KeyError::Unsupported)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/keys/");
        std::fs::read_to_string(format!("{path}{name}")).unwrap()
    }

    /// `ssh-keygen -lf` output for a fixture, e.g. "SHA256:abc...".
    fn keygen_fingerprint(name: &str) -> String {
        let line = fixture("fingerprints.txt").lines().find(|l| l.contains(name)).unwrap().to_string();
        line.split_whitespace().nth(1).unwrap().to_string()
    }

    #[test]
    fn generate_ed25519_key_has_openssh_formats() {
        let k = generate(KeyType::Ed25519, "me@laptop").unwrap();
        assert!(k.public_openssh.starts_with("ssh-ed25519 AAAA"));
        assert!(k.public_openssh.ends_with(" me@laptop"));
        assert!(k.private_openssh.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"));
        assert!(k.fingerprint.starts_with("SHA256:"));
        // Round-trips through import.
        assert_eq!(import_private(&k.private_openssh, None).unwrap().fingerprint, k.fingerprint);
    }

    #[test]
    fn generate_rsa() {
        let k = generate(KeyType::Rsa { bits: 2048 }, "rsa").unwrap();
        assert!(k.public_openssh.starts_with("ssh-rsa AAAA"));
    }

    #[test]
    fn rejects_weak_rsa() {
        assert_eq!(generate(KeyType::Rsa { bits: 1024 }, "x").unwrap_err(), KeyError::Unsupported);
    }

    #[test]
    fn import_openssh_private_key_fingerprint_matches_ssh_keygen() {
        let k = import_private(&fixture("id_ed25519"), None).unwrap();
        assert_eq!(k.fingerprint, keygen_fingerprint("wh-test (ED25519)"));
        assert_eq!(k.public_openssh.split_whitespace().nth(1), fixture("id_ed25519.pub").split_whitespace().nth(1));
    }

    #[test]
    fn import_encrypted_key_requires_passphrase() {
        let text = fixture("id_ed25519_enc");
        assert_eq!(import_private(&text, None).unwrap_err(), KeyError::NeedsPassphrase);
        assert_eq!(import_private(&text, Some("nope")).unwrap_err(), KeyError::BadPassphrase);
        let k = import_private(&text, Some("fixture-pass")).unwrap();
        assert_eq!(k.fingerprint, keygen_fingerprint("wh-test-enc"));
        // Stored form is decrypted (the vault provides encryption at rest).
        assert!(import_private(&k.private_openssh, None).is_ok());
    }

    #[test]
    fn import_pem_rsa() {
        let k = import_private(&fixture("id_rsa_pem"), None).unwrap();
        assert_eq!(k.fingerprint, keygen_fingerprint("wh-test-rsa"));
    }

    #[test]
    fn stored_key_loads_for_auth() {
        let k = import_private(&fixture("id_ed25519_enc"), Some("fixture-pass")).unwrap();
        let loaded = load_stored(&k.private_openssh).unwrap();
        assert_eq!(loaded.fingerprint(HashAlg::Sha256).to_string(), k.fingerprint);
        assert_eq!(load_stored("nope").unwrap_err(), KeyError::Unsupported);
    }

    #[test]
    fn import_garbage_is_unsupported() {
        assert_eq!(import_private("hello", None).unwrap_err(), KeyError::Unsupported);
    }

    #[test]
    fn certificate_pairs_with_its_key_only() {
        let cert = fixture("id_ed25519-cert.pub");
        assert_eq!(check_certificate(&fixture("id_ed25519.pub"), &cert), Ok(()));
        assert_eq!(
            check_certificate(&fixture("id_ed25519_enc.pub"), &cert),
            Err(KeyError::CertificateMismatch)
        );
        assert_eq!(check_certificate(&fixture("id_ed25519.pub"), "junk"), Err(KeyError::Unsupported));
    }
}
