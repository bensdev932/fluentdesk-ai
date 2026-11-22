//! Isolated tests for DPAPI secrets-only `CredentialVault` and `RuntimeSecretStore`.

use crate::ai::catalog::ProviderId;
use crate::ai::secrets::{KeySource, RuntimeSecretStore};
use crate::ai::vault::default_storage_dir;
use crate::telemetry::RedactedSecret;

#[test]
fn default_storage_dir_resolves_appdata_fluentdesk_folder() {
    let dir = default_storage_dir();
    assert_eq!(
        dir.file_name().and_then(|s| s.to_str()),
        Some("FluentDeskAI")
    );
    #[cfg(windows)]
    if let Some(appdata) = std::env::var_os("APPDATA").filter(|v| !v.is_empty()) {
        assert_eq!(dir, std::path::PathBuf::from(appdata).join("FluentDeskAI"));
    }
}

#[test]
fn in_memory_secret_store_overrides_env_and_clears_only_runtime() {
    let store = RuntimeSecretStore::new();
    let env = |name: &str| (name == "GROQ_API_KEY").then(|| "gsk_env_fallback".to_string());
    assert_eq!(
        store.resolve_with_env(ProviderId::Groq, env),
        Some((RedactedSecret::new("gsk_env_fallback"), KeySource::Env))
    );
    store.set_key(ProviderId::Groq, Some("  gsk_runtime_override  "));
    assert_eq!(
        store.resolve_with_env(ProviderId::Groq, env),
        Some((
            RedactedSecret::new("gsk_runtime_override"),
            KeySource::Runtime
        ))
    );
    store.clear_key(ProviderId::Groq);
    assert_eq!(
        store.resolve_with_env(ProviderId::Groq, env),
        Some((RedactedSecret::new("gsk_env_fallback"), KeySource::Env))
    );
    assert_eq!(store.resolve_with_env(ProviderId::Groq, |_| None), None);
}

#[cfg(windows)]
mod windows_dpapi_tests {
    use crate::ai::catalog::ProviderId;
    use crate::ai::secrets::{KeySource, RuntimeSecretStore};
    use crate::ai::vault::CredentialVault;
    use crate::telemetry::RedactedSecret;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_DIR_SEQ: AtomicU64 = AtomicU64::new(0);

    struct TempVaultDir(PathBuf);

    impl TempVaultDir {
        fn new(tag: &str) -> Self {
            let seq = TEST_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
            let dir =
                std::env::temp_dir().join(format!("fluentdesk_{tag}_{}_{seq}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            Self(dir)
        }
    }

    impl Drop for TempVaultDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn dpapi_vault_encrypts_keys_without_plaintext_or_prefs_json_on_disk() {
        let tmp = TempVaultDir::new("dpapi_vault");
        let vault = CredentialVault::with_storage_dir(tmp.0.clone());
        assert!(vault.load_secrets().unwrap().is_empty());

        let keys = HashMap::from([
            (
                ProviderId::Groq,
                RedactedSecret::new("gsk_dpapi_secret_123"),
            ),
            (ProviderId::Gemini, RedactedSecret::new("AIzaDpapiTest456")),
            (ProviderId::Ollama, RedactedSecret::new("ignored_local")),
        ]);
        vault.save_secrets(&keys).expect("dpapi save");
        assert!(!tmp.0.join("router-prefs.json").exists());

        let disk = std::fs::read_to_string(vault.credentials_path()).expect("read dpapi file");
        assert!(disk.starts_with("DPAPIv1:"));
        assert!(!disk.contains("gsk_dpapi_secret_123"));
        assert!(!disk.contains("AIzaDpapiTest456"));
        assert!(!disk.contains("groq") && !disk.contains("gemini"));

        let reloaded = CredentialVault::with_storage_dir(tmp.0.clone())
            .load_secrets()
            .expect("dpapi load");
        assert_eq!(
            reloaded.get(&ProviderId::Groq).map(|s| s.expose_secret()),
            Some("gsk_dpapi_secret_123")
        );
        assert_eq!(
            reloaded.get(&ProviderId::Gemini).map(|s| s.expose_secret()),
            Some("AIzaDpapiTest456")
        );
        assert!(!reloaded.contains_key(&ProviderId::Ollama));

        std::fs::write(vault.credentials_path(), "DPAPIv1:not-valid-ciphertext").unwrap();
        assert!(vault.load_secrets().is_err());
    }

    #[test]
    fn persistent_secret_store_survives_restart_and_clear_preserves_env_fallback() {
        let tmp = TempVaultDir::new("persistent_store");
        let store = RuntimeSecretStore::persistent(tmp.0.clone());
        store.set_key(ProviderId::Groq, Some("gsk_persisted_999"));
        store.set_key(ProviderId::Openrouter, Some("sk-or-persisted-777"));
        assert!(!tmp.0.join("router-prefs.json").exists());
        drop(store);

        let restarted = RuntimeSecretStore::persistent(tmp.0.clone());
        let env = |name: &str| (name == "GROQ_API_KEY").then(|| "gsk_env_backup".to_string());
        assert_eq!(
            restarted.resolve_with_env(ProviderId::Groq, env),
            Some((RedactedSecret::new("gsk_persisted_999"), KeySource::Runtime))
        );
        assert_eq!(
            restarted.resolve_with_env(ProviderId::Openrouter, env),
            Some((
                RedactedSecret::new("sk-or-persisted-777"),
                KeySource::Runtime
            ))
        );

        restarted.clear_key(ProviderId::Groq);
        assert_eq!(
            restarted.resolve_with_env(ProviderId::Groq, env),
            Some((RedactedSecret::new("gsk_env_backup"), KeySource::Env))
        );
        drop(restarted);

        let after_clear = RuntimeSecretStore::persistent(tmp.0.clone());
        assert_eq!(
            after_clear.resolve_with_env(ProviderId::Groq, |_| None),
            None
        );
        assert_eq!(
            after_clear.resolve_with_env(ProviderId::Groq, env),
            Some((RedactedSecret::new("gsk_env_backup"), KeySource::Env))
        );
        assert_eq!(
            after_clear.resolve_with_env(ProviderId::Openrouter, env),
            Some((
                RedactedSecret::new("sk-or-persisted-777"),
                KeySource::Runtime
            ))
        );
    }
}
