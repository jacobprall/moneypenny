use serde::{Deserialize, Serialize};

// =========================================================================
// Encryption configuration
// =========================================================================

/// Encryption key source for database-at-rest encryption.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum KeySource {
    /// macOS Keychain
    Keychain { service: String, account: String },
    /// Linux kernel keyring or key file
    KeyFile { path: String },
    /// Windows Credential Manager
    CredentialManager { target: String },
    /// Passphrase-derived (WASM/portable)
    Passphrase { salt: String },
    /// No encryption (development only)
    None,
}

impl Default for KeySource {
    fn default() -> Self {
        KeySource::None
    }
}

/// Encryption configuration for the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionConfig {
    pub enabled: bool,
    pub key_source: KeySource,
    pub cipher: String,
}

impl Default for EncryptionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            key_source: KeySource::None,
            cipher: "aes-256-cbc".into(),
        }
    }
}

// =========================================================================
// Key management abstractions
// =========================================================================

/// Retrieve the encryption key from the configured source.
pub fn get_key(source: &KeySource) -> anyhow::Result<Option<Vec<u8>>> {
    match source {
        KeySource::None => Ok(None),
        KeySource::Keychain { service, account } => {
            get_key_from_keychain(service, account)
        }
        KeySource::KeyFile { path } => {
            get_key_from_file(path)
        }
        KeySource::CredentialManager { target } => {
            get_key_from_credential_manager(target)
        }
        KeySource::Passphrase { salt } => {
            get_key_from_passphrase(salt)
        }
    }
}

/// Store an encryption key to the configured source.
pub fn store_key(source: &KeySource, key: &[u8]) -> anyhow::Result<()> {
    match source {
        KeySource::None => Ok(()),
        KeySource::KeyFile { path } => {
            std::fs::write(path, key)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
            }
            Ok(())
        }
        _ => {
            anyhow::bail!("Key storage for this source type requires platform-specific integration")
        }
    }
}

/// Generate a random 256-bit key.
pub fn generate_key() -> Vec<u8> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::SystemTime;

    // Simple key generation (production would use a CSPRNG)
    let mut key = Vec::with_capacity(32);
    for i in 0..4 {
        let mut hasher = DefaultHasher::new();
        SystemTime::now().hash(&mut hasher);
        i.hash(&mut hasher);
        std::process::id().hash(&mut hasher);
        key.extend_from_slice(&hasher.finish().to_le_bytes());
    }
    key
}

fn get_key_from_keychain(_service: &str, _account: &str) -> anyhow::Result<Option<Vec<u8>>> {
    // Platform-specific: requires Security.framework on macOS
    anyhow::bail!("Keychain integration requires platform-specific implementation (macOS Security.framework)")
}

fn get_key_from_file(path: &str) -> anyhow::Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(data) => Ok(Some(data)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn get_key_from_credential_manager(_target: &str) -> anyhow::Result<Option<Vec<u8>>> {
    anyhow::bail!("Windows Credential Manager integration requires platform-specific implementation")
}

fn get_key_from_passphrase(salt: &str) -> anyhow::Result<Option<Vec<u8>>> {
    // Simple passphrase derivation (production would use PBKDF2/Argon2)
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    salt.hash(&mut hasher);
    "moneypenny-encryption".hash(&mut hasher);
    let h1 = hasher.finish().to_le_bytes();

    let mut hasher2 = DefaultHasher::new();
    h1.hash(&mut hasher2);
    salt.hash(&mut hasher2);
    let h2 = hasher2.finish().to_le_bytes();

    let mut hasher3 = DefaultHasher::new();
    h2.hash(&mut hasher3);
    salt.hash(&mut hasher3);
    let h3 = hasher3.finish().to_le_bytes();

    let mut hasher4 = DefaultHasher::new();
    h3.hash(&mut hasher4);
    salt.hash(&mut hasher4);
    let h4 = hasher4.finish().to_le_bytes();

    let mut key = Vec::with_capacity(32);
    key.extend_from_slice(&h1);
    key.extend_from_slice(&h2);
    key.extend_from_slice(&h3);
    key.extend_from_slice(&h4);
    Ok(Some(key))
}

/// Apply encryption pragmas to an open SQLite connection (for SEE/SQLCipher).
pub fn apply_encryption(conn: &rusqlite::Connection, key: &[u8]) -> anyhow::Result<()> {
    let hex_key: String = key.iter().map(|b| format!("{b:02x}")).collect();
    conn.execute_batch(&format!("PRAGMA key = 'x\"{hex_key}\"';"))?;
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Key source
    // ========================================================================

    #[test]
    fn key_source_none_returns_none() {
        let key = get_key(&KeySource::None).unwrap();
        assert!(key.is_none());
    }

    #[test]
    fn key_source_passphrase_returns_key() {
        let key = get_key(&KeySource::Passphrase { salt: "test-salt".into() }).unwrap();
        assert!(key.is_some());
        assert_eq!(key.unwrap().len(), 32);
    }

    #[test]
    fn key_source_passphrase_deterministic() {
        let k1 = get_key(&KeySource::Passphrase { salt: "same".into() }).unwrap().unwrap();
        let k2 = get_key(&KeySource::Passphrase { salt: "same".into() }).unwrap().unwrap();
        assert_eq!(k1, k2);
    }

    #[test]
    fn key_source_different_salts_different_keys() {
        let k1 = get_key(&KeySource::Passphrase { salt: "salt1".into() }).unwrap().unwrap();
        let k2 = get_key(&KeySource::Passphrase { salt: "salt2".into() }).unwrap().unwrap();
        assert_ne!(k1, k2);
    }

    #[test]
    fn key_source_file_not_found() {
        let key = get_key(&KeySource::KeyFile { path: "/tmp/mp_nonexistent_key_12345".into() }).unwrap();
        assert!(key.is_none());
    }

    #[test]
    fn key_source_file_roundtrip() {
        let path = std::env::temp_dir().join("mp_test_key_roundtrip");
        let original = generate_key();
        store_key(&KeySource::KeyFile { path: path.to_string_lossy().into() }, &original).unwrap();

        let loaded = get_key(&KeySource::KeyFile { path: path.to_string_lossy().into() }).unwrap().unwrap();
        assert_eq!(original, loaded);

        let _ = std::fs::remove_file(&path);
    }

    // ========================================================================
    // Key generation
    // ========================================================================

    #[test]
    fn generate_key_is_32_bytes() {
        let key = generate_key();
        assert_eq!(key.len(), 32);
    }

    #[test]
    fn generate_key_is_unique() {
        let k1 = generate_key();
        std::thread::sleep(std::time::Duration::from_millis(1));
        let k2 = generate_key();
        // Not guaranteed but extremely likely to differ
        // If this fails it's a cosmic coincidence, not a bug
        assert_ne!(k1, k2);
    }

    // ========================================================================
    // Config
    // ========================================================================

    #[test]
    fn encryption_config_default_disabled() {
        let config = EncryptionConfig::default();
        assert!(!config.enabled);
        assert!(matches!(config.key_source, KeySource::None));
    }

    #[test]
    fn encryption_config_serializes() {
        let config = EncryptionConfig {
            enabled: true,
            key_source: KeySource::Passphrase { salt: "mysalt".into() },
            cipher: "aes-256-cbc".into(),
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("passphrase"));
        assert!(json.contains("mysalt"));
    }

    #[test]
    fn key_source_keychain_not_implemented() {
        let result = get_key(&KeySource::Keychain {
            service: "test".into(),
            account: "test".into(),
        });
        assert!(result.is_err());
    }

    #[test]
    fn key_source_credential_manager_not_implemented() {
        let result = get_key(&KeySource::CredentialManager { target: "test".into() });
        assert!(result.is_err());
    }

    #[test]
    fn store_key_none_is_noop() {
        store_key(&KeySource::None, &[0u8; 32]).unwrap();
    }
}
