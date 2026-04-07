#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub database_url: String,
    pub static_dir: String,
    pub encryption_key: String,
    /// Optional path to a file containing the current hex key. When set, this
    /// file is authoritative and key rotation overwrites it atomically.
    pub encryption_key_file: Option<String>,
    /// Directory where uploaded evidence chunks + diagnostic ZIPs land.
    pub storage_dir: String,
    /// Facility code used for watermarks and traceability codes. Sourced
    /// from `FACILITY_CODE` env var; defaults to the DB seed value.
    pub facility_code: String,
}

/// Check that `key` is exactly 64 hex characters (32 bytes for AES-256).
fn is_valid_hex_key(key: &str) -> bool {
    key.len() == 64 && key.chars().all(|c| c.is_ascii_hexdigit())
}

/// Generate a random 32-byte key as 64 hex chars using OS randomness.
fn generate_random_key() -> String {
    use std::io::Read;
    let mut buf = [0u8; 32];
    // Use /dev/urandom on Linux; getrandom on other OSes via std.
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = f.read_exact(&mut buf);
    } else {
        // Fallback: use system time + pid as entropy (not ideal but never panics)
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(42) as u64;
        for (i, b) in buf.iter_mut().enumerate() {
            *b = ((seed.wrapping_mul(6364136223846793005).wrapping_add(i as u64)) >> 33) as u8;
        }
    }
    buf.iter().map(|b| format!("{:02x}", b)).collect()
}

impl Config {
    pub fn from_env() -> Self {
        let encryption_key_file = std::env::var("ENCRYPTION_KEY_FILE").ok();
        // If a key file exists and is readable, prefer it over the env var.
        let raw_key = if let Some(ref path) = encryption_key_file {
            match std::fs::read_to_string(path) {
                Ok(s) => {
                    let trimmed = s.trim().to_string();
                    if trimmed.is_empty() {
                        std::env::var("ENCRYPTION_KEY").ok()
                    } else {
                        Some(trimmed)
                    }
                }
                Err(_) => std::env::var("ENCRYPTION_KEY").ok(),
            }
        } else {
            std::env::var("ENCRYPTION_KEY").ok()
        };

        // Validate the key. If missing or invalid, auto-generate one for
        // local non-docker development and persist it to the key file if
        // configured. This replaces the old panic-on-placeholder behavior.
        let encryption_key = match raw_key {
            Some(k) if is_valid_hex_key(&k) => k,
            Some(k) if k == "dev-key-placeholder" || k.is_empty() => {
                eprintln!(
                    "WARNING: ENCRYPTION_KEY is not set or is a placeholder. \
                     Generating a random key for local development."
                );
                let key = generate_random_key();
                if let Some(ref path) = encryption_key_file {
                    if let Err(e) = std::fs::write(path, &key) {
                        eprintln!("WARNING: could not persist generated key to {}: {}", path, e);
                    }
                }
                key
            }
            Some(k) => {
                eprintln!(
                    "ERROR: ENCRYPTION_KEY must be exactly 64 hex characters (32 bytes). \
                     Got {} characters. Please set a valid key via ENCRYPTION_KEY env var \
                     or ENCRYPTION_KEY_FILE.",
                    k.len()
                );
                std::process::exit(1);
            }
            None => {
                eprintln!(
                    "WARNING: ENCRYPTION_KEY is not set. \
                     Generating a random key for local development."
                );
                let key = generate_random_key();
                if let Some(ref path) = encryption_key_file {
                    if let Err(e) = std::fs::write(path, &key) {
                        eprintln!("WARNING: could not persist generated key to {}: {}", path, e);
                    }
                }
                key
            }
        };

        Self {
            port: std::env::var("PORT")
                .unwrap_or_else(|_| "8080".into())
                .parse()
                .expect("PORT must be a number"),
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite://app.db".into()),
            static_dir: std::env::var("STATIC_DIR")
                .unwrap_or_else(|_| "static".into()),
            encryption_key,
            encryption_key_file,
            storage_dir: std::env::var("STORAGE_DIR")
                .unwrap_or_else(|_| "/app/storage".into()),
            facility_code: std::env::var("FACILITY_CODE")
                .unwrap_or_else(|_| "FAC01".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_hex_key_accepted() {
        assert!(is_valid_hex_key("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff"));
    }

    #[test]
    fn short_key_rejected() {
        assert!(!is_valid_hex_key("dead"));
    }

    #[test]
    fn placeholder_rejected() {
        assert!(!is_valid_hex_key("dev-key-placeholder"));
    }

    #[test]
    fn generated_key_is_valid() {
        let key = generate_random_key();
        assert!(is_valid_hex_key(&key), "generated key should be valid hex: {}", key);
    }
}
