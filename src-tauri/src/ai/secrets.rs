//! Credential resolution with optional DPAPI disk persistence and env-var fallback.
//! Keys are wrapped in `RedactedSecret`, encrypted via DPAPI when persisted, and never logged.

use crate::ai::catalog::ProviderId;
use crate::ai::vault::{default_storage_dir, CredentialVault};
use crate::telemetry::{redact_sensitive_log_text, RedactedSecret};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KeySource {
    Env,
    Runtime,
    None,
}

#[derive(Clone, Default)]
pub struct RuntimeSecretStore {
    keys: Arc<RwLock<HashMap<ProviderId, RedactedSecret>>>,
    vault: Option<CredentialVault>,
    persist_lock: Arc<Mutex<()>>,
}

impl RuntimeSecretStore {
    /// Creates an isolated in-memory store (used by unit tests).
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a DPAPI-backed store rooted at `dir`, loading any previously saved keys.
    pub fn persistent(dir: PathBuf) -> Self {
        let vault = CredentialVault::with_storage_dir(dir);
        let initial = match vault.load_secrets() {
            Ok(map) => map,
            Err(err) => {
                tracing::warn!(error = %redact_sensitive_log_text(&err), "Failed loading DPAPI vault; starting empty");
                HashMap::new()
            }
        };
        Self {
            keys: Arc::new(RwLock::new(initial)),
            vault: Some(vault),
            persist_lock: Arc::new(Mutex::new(())),
        }
    }

    /// Creates a DPAPI-backed store under the default OS user app-data directory (`%APPDATA%\FluentDeskAI`).
    pub fn default_persistent() -> Self {
        Self::persistent(default_storage_dir())
    }

    /// Sets or clears a runtime API key (persists to DPAPI vault when enabled; empty clears).
    pub fn set_key(&self, provider: ProviderId, raw: Option<&str>) {
        if provider.is_cloud() && self.mutate_key(provider, raw) {
            self.persist_snapshot();
        }
    }

    /// Clears ONLY the saved runtime/DPAPI key for one cloud provider (env fallback remains intact).
    pub fn clear_key(&self, provider: ProviderId) {
        self.set_key(provider, None);
    }

    /// Resolves a cloud API key from runtime/DPAPI storage first, then process env vars.
    pub fn resolve(&self, provider: ProviderId) -> Option<(RedactedSecret, KeySource)> {
        self.resolve_with_env(provider, |name| std::env::var(name).ok())
    }

    /// Returns whether a key is available and from which source (never reveals the key).
    pub fn key_source(&self, provider: ProviderId) -> KeySource {
        self.resolve(provider)
            .map(|(_, src)| src)
            .unwrap_or(KeySource::None)
    }

    pub(crate) fn resolve_with_env(
        &self,
        provider: ProviderId,
        env_lookup: impl FnOnce(&str) -> Option<String>,
    ) -> Option<(RedactedSecret, KeySource)> {
        if !provider.is_cloud() {
            return None;
        }
        if let Some(secret) = self.keys.read().expect("secret lock").get(&provider) {
            return Some((secret.clone(), KeySource::Runtime));
        }
        let env_name = provider.env_var_name()?;
        let val = env_lookup(env_name)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())?;
        Some((RedactedSecret::new(val), KeySource::Env))
    }

    fn mutate_key(&self, provider: ProviderId, raw: Option<&str>) -> bool {
        let mut guard = self.keys.write().expect("secret lock");
        match raw.map(str::trim).filter(|s| !s.is_empty()) {
            Some(clean) => {
                guard.insert(provider, RedactedSecret::new(clean.to_string()));
                true
            }
            None => guard.remove(&provider).is_some(),
        }
    }

    fn persist_snapshot(&self) {
        let Some(vault) = &self.vault else {
            return;
        };
        let _guard = self.persist_lock.lock().expect("persist lock");
        let snapshot = self.keys.read().expect("secret lock").clone();
        if let Err(err) = vault.save_secrets(&snapshot) {
            tracing::warn!(error = %redact_sensitive_log_text(&err), "Failed persisting DPAPI credential vault");
        }
    }
}
