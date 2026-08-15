use super::*;

#[test]
fn test_security_level_comparisons() {
    let standard = SandboxPolicy::default();
    let strict = SandboxPolicy::strict();
    let paranoid = SandboxPolicy::paranoid();

    assert_eq!(standard.level, SecurityLevel::Standard);
    assert_eq!(strict.level, SecurityLevel::Strict);
    assert_eq!(paranoid.level, SecurityLevel::Paranoid);

    assert!(strict.enforce_https);
    assert!(!strict.allow_webgl);
    assert!(!paranoid.allow_javascript);
}

#[test]
fn test_pqc_mlkem_flow() {
    let pqc = MlkemEngine;
    let (pk, sk) = pqc.generate_keypair();
    
    let (ss1, ct) = pqc.encapsulate(&pk);
    let ss2 = pqc.decapsulate(&ct, &sk);
    
    assert_eq!(ss1, ss2);
}

#[test]
fn test_software_vault_encryption() {
    let dir = std::env::temp_dir();
    let vault_path = dir.join("sentinel_vault_test");
    std::fs::create_dir_all(&vault_path).unwrap();
    
    let key = [0u8; 32]; // Test key
    let vault = SoftwareVault::new(vault_path.clone(), key);
    
    let secret_name = "test_secret";
    let secret_value = b"Top Secret Data".to_vec();
    
    vault.store_secret(secret_name, secret_value.clone()).unwrap();
    
    // Verify file exists and is encrypted (not plain text)
    let encrypted_data = std::fs::read(vault_path.join(secret_name)).unwrap();
    assert_ne!(encrypted_data, secret_value);
    
    // Decrypt and verify
    let decrypted_value = vault.get_secret(secret_name).unwrap();
    assert_eq!(decrypted_value, secret_value);
    
    // Cleanup
    let _ = std::fs::remove_dir_all(&vault_path);
}

#[test]
fn test_security_manager_initialization() {
    let mut manager = SecurityManager::new();
    assert_eq!(manager.pqc, MlkemEngine);
    
    let dir = std::env::temp_dir();
    manager.use_software_vault(dir.join("sw_vault"), [1u8; 32]);
    
    manager.harden_system().unwrap();
}

#[cfg(test)]
mod prop_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_vault_roundtrip(key in prop::array::uniform32(0u8..), data in prop::collection::vec(0u8.., 1..1024)) {
            let dir = std::env::temp_dir();
            let vault_path = dir.join(format!("vault_prop_{}", hex::encode(&key[..4])));
            std::fs::create_dir_all(&vault_path).unwrap();
            
            let vault = SoftwareVault::new(vault_path.clone(), key);
            let secret_name = "prop_secret";
            
            vault.store_secret(secret_name, data.clone()).unwrap();
            let decrypted = vault.get_secret(secret_name).unwrap();
            
            prop_assert_eq!(decrypted, data);
            
            let _ = std::fs::remove_dir_all(&vault_path);
        }
    }
}
