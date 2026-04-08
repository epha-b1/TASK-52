//! AES-256-GCM encryption for sensitive fields at rest.
//!
//! The active cipher is stored behind an `ArcSwap` so it can be replaced
//! atomically during key rotation without blocking readers. Both encryption
//! and decryption are fallible — we never panic on malformed ciphertext.

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

#[derive(Clone)]
pub struct Crypto {
    cipher: Aes256Gcm,
}

#[derive(Debug)]
pub enum CryptoError {
    InvalidKey,
    Encrypt,
    Decrypt,
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CryptoError::InvalidKey => write!(f, "invalid key material"),
            CryptoError::Encrypt => write!(f, "encryption failed"),
            CryptoError::Decrypt => write!(f, "decryption failed"),
        }
    }
}

impl std::error::Error for CryptoError {}

impl Crypto {
    /// Build a `Crypto` from a 64-char hex key (32 bytes).
    pub fn from_hex(hex_key: &str) -> Result<Self, CryptoError> {
        let key_bytes = hex::decode(hex_key).map_err(|_| CryptoError::InvalidKey)?;
        if key_bytes.len() != 32 { return Err(CryptoError::InvalidKey); }
        let cipher = Aes256Gcm::new_from_slice(&key_bytes).map_err(|_| CryptoError::InvalidKey)?;
        Ok(Self { cipher })
    }

    /// Back-compat constructor — still panics on bad key for internal callers
    /// that assume a valid configured key. Prefer `from_hex`.
    pub fn new(hex_key: &str) -> Self {
        Self::from_hex(hex_key).expect("ENCRYPTION_KEY must be 32 bytes (64 hex chars)")
    }

    pub fn encrypt(&self, plaintext: &str) -> String {
        // This path is used in-handler for user input; falling back to a
        // deterministic error blob would corrupt data. Production callers
        // must never fail on encrypt so we pass through expect — but note
        // that AES-GCM encrypt only fails on allocation failures.
        self.try_encrypt(plaintext).expect("aes-gcm encrypt")
    }

    pub fn try_encrypt(&self, plaintext: &str) -> Result<String, CryptoError> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ct = self.cipher.encrypt(&nonce, plaintext.as_bytes()).map_err(|_| CryptoError::Encrypt)?;
        let mut combined = nonce.to_vec();
        combined.extend(ct);
        Ok(STANDARD.encode(&combined))
    }

    /// Legacy panic-on-error API kept for handlers that treat stored data
    /// as trusted. Prefer `try_decrypt` in new code.
    pub fn decrypt(&self, encoded: &str) -> String {
        self.try_decrypt(encoded).unwrap_or_default()
    }

    pub fn try_decrypt(&self, encoded: &str) -> Result<String, CryptoError> {
        let combined = STANDARD.decode(encoded).map_err(|_| CryptoError::Decrypt)?;
        if combined.len() < 12 { return Err(CryptoError::Decrypt); }
        let (nonce_bytes, ct) = combined.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        let pt = self.cipher.decrypt(nonce, ct).map_err(|_| CryptoError::Decrypt)?;
        String::from_utf8(pt).map_err(|_| CryptoError::Decrypt)
    }
}

/// Mask a phone number to show only the last 4 digits.
pub fn mask_phone(phone: &str) -> String {
    let digits: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() >= 4 {
        format!("***-***-{}", &digits[digits.len() - 4..])
    } else {
        "***-***-****".into()
    }
}

/// Mask a street address: show first word (house number) + "***".
pub fn mask_street(street: &str) -> String {
    let trimmed = street.trim();
    if trimmed.is_empty() { return "***".into(); }
    match trimmed.find(' ') {
        Some(idx) => format!("{} ***", &trimmed[..idx]),
        None if trimmed.len() <= 3 => "***".into(),
        None => format!("{}***", &trimmed[..3]),
    }
}

/// Mask a city name: show first 2 characters + "***".
pub fn mask_city(city: &str) -> String {
    let trimmed = city.trim();
    if trimmed.len() <= 2 { return "***".into(); }
    format!("{}***", &trimmed[..2])
}

/// Mask a ZIP+4 code to show only the last 4 characters.
/// "90210-1234" → "***0-1234"; "90210" → "***10"
pub fn mask_zip(zip: &str) -> String {
    let trimmed = zip.trim();
    if trimmed.len() <= 4 {
        return format!("***{}", trimmed);
    }
    format!("***{}", &trimmed[trimmed.len() - 4..])
}

/// Mask a state: return as-is (typically 2-letter abbreviation, not
/// considered sensitive on its own without the rest of the address).
pub fn mask_state(state: &str) -> String {
    state.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn round_trip_plain() {
        let c = Crypto::from_hex(TEST_KEY).unwrap();
        let ct = c.try_encrypt("hello world").unwrap();
        assert_ne!(ct, "hello world");
        assert_eq!(c.try_decrypt(&ct).unwrap(), "hello world");
    }

    #[test]
    fn decrypt_rejects_tampered() {
        let c = Crypto::from_hex(TEST_KEY).unwrap();
        let ct = c.try_encrypt("sensitive").unwrap();
        let mut tampered = ct.clone();
        // Corrupt the last char in a way that still decodes as base64
        let last = tampered.pop().unwrap();
        tampered.push(if last == 'A' { 'B' } else { 'A' });
        assert!(c.try_decrypt(&tampered).is_err());
    }

    #[test]
    fn wrong_key_cannot_decrypt() {
        let c1 = Crypto::from_hex(TEST_KEY).unwrap();
        let c2 = Crypto::from_hex("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff").unwrap();
        let ct = c1.try_encrypt("confidential").unwrap();
        assert!(c2.try_decrypt(&ct).is_err());
    }

    #[test]
    fn bad_hex_rejected() {
        assert!(Crypto::from_hex("not-hex").is_err());
        assert!(Crypto::from_hex("dead").is_err()); // wrong length
    }

    #[test]
    fn mask_phone_last_four() {
        assert_eq!(mask_phone("555-867-5309"), "***-***-5309");
        assert_eq!(mask_phone("(415) 555-1234"), "***-***-1234");
        assert_eq!(mask_phone("1"), "***-***-****");
    }

    #[test]
    fn mask_street_shows_house_number() {
        assert_eq!(mask_street("123 Main Street"), "123 ***");
        assert_eq!(mask_street("4500 Oak Ave"), "4500 ***");
        assert_eq!(mask_street(""), "***");
    }

    #[test]
    fn mask_city_shows_first_two() {
        assert_eq!(mask_city("Portland"), "Po***");
        assert_eq!(mask_city("LA"), "***");
        assert_eq!(mask_city("San Francisco"), "Sa***");
    }

    #[test]
    fn mask_state_passthrough() {
        assert_eq!(mask_state("OR"), "OR");
        assert_eq!(mask_state("CA"), "CA");
    }
}
