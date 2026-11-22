//! Typed DTOs, snapshots, and state helpers for the deepened `TutorRouter` interface.

use crate::ai::{
    catalog::{ProviderId, ProviderPreference},
    cloud_http::CloudFailure,
    secrets::{KeySource, RuntimeSecretStore},
};
use crate::domain::{LearnerPreferences, LearnerPreferencesPatch, TutorReply};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutedTutorReply {
    pub reply: TutorReply,
    pub provider: ProviderId,
    pub model: String,
    pub is_cloud: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failover_note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDescriptorView {
    pub id: ProviderId,
    pub label: &'static str,
    pub is_cloud: bool,
    pub configured: bool,
    pub key_source: KeySource,
    pub default_model: &'static str,
    pub free_models: Vec<&'static str>,
    pub selected_model: String,
    pub cooldown_secs: u64,
    pub auth_invalid: bool,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvidersSnapshot {
    pub cloud_opt_in: bool,
    pub preferred_provider: ProviderPreference,
    pub providers: Vec<ProviderDescriptorView>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouterConfigPatch {
    pub cloud_opt_in: Option<bool>,
    pub preferred_provider: Option<ProviderPreference>,
    #[serde(default)]
    pub model_overrides: HashMap<ProviderId, String>,
    #[serde(default)]
    pub api_keys: HashMap<ProviderId, String>,
    #[serde(default)]
    pub clear_keys: Vec<ProviderId>,
    #[serde(default)]
    pub reset_cooldowns: bool,
}

impl RouterConfigPatch {
    pub fn to_preferences_patch(&self) -> LearnerPreferencesPatch {
        LearnerPreferencesPatch {
            cloud_opt_in: self.cloud_opt_in,
            preferred_provider: self.preferred_provider,
            model_overrides: self.model_overrides.clone(),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouterHealthView {
    pub status: &'static str,
    pub provider: ProviderId,
    pub is_cloud: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Default)]
pub(crate) struct RouterState {
    pub prefs: LearnerPreferences,
    pub endpoint_overrides: HashMap<ProviderId, String>,
}

impl RouterState {
    pub fn from_preferences(prefs: LearnerPreferences) -> Self {
        Self {
            prefs,
            endpoint_overrides: HashMap::new(),
        }
    }

    pub fn sync_from_preferences(&mut self, prefs: &LearnerPreferences) {
        self.prefs = prefs.clone();
    }

    pub fn apply_keys(patch: &RouterConfigPatch, secrets: &RuntimeSecretStore) {
        patch.clear_keys.iter().for_each(|&p| secrets.clear_key(p));
        patch
            .api_keys
            .iter()
            .for_each(|(&p, k)| secrets.set_key(p, Some(k)));
    }

    pub fn apply_routing_patch(&mut self, patch: LearnerPreferencesPatch) {
        let _ = self.prefs.apply_patch(patch);
    }
}

#[derive(Default)]
pub(crate) struct CircuitBreakers {
    cooldowns: HashMap<ProviderId, Instant>,
    auth_invalid: HashSet<ProviderId>,
}

impl CircuitBreakers {
    pub fn status(&mut self, id: ProviderId) -> (u64, bool) {
        (self.cooldown_secs(id), self.auth_invalid.contains(&id))
    }

    pub fn is_healthy(&mut self, id: ProviderId) -> bool {
        !self.auth_invalid.contains(&id) && self.cooldown_secs(id) == 0
    }

    pub fn cooldown_secs(&mut self, id: ProviderId) -> u64 {
        let Some(&until) = self.cooldowns.get(&id) else {
            return 0;
        };
        let now = Instant::now();
        if until > now {
            return until.duration_since(now).as_secs().max(1);
        }
        self.cooldowns.remove(&id);
        0
    }

    pub fn mark_cooldown(&mut self, id: ProviderId, secs: u64) {
        self.cooldowns
            .insert(id, Instant::now() + Duration::from_secs(secs));
    }

    pub fn mark_auth_invalid(&mut self, id: ProviderId) {
        self.auth_invalid.insert(id);
    }

    pub fn record(&mut self, id: ProviderId, failure: CloudFailure, cooldown_secs: u64) {
        match failure {
            CloudFailure::Cooldown => self.mark_cooldown(id, cooldown_secs),
            CloudFailure::AuthInvalid => self.mark_auth_invalid(id),
            CloudFailure::None => {}
        }
    }

    pub fn apply_patch_resets(&mut self, patch: &RouterConfigPatch) {
        if patch.reset_cooldowns {
            self.cooldowns.clear();
            self.auth_invalid.clear();
            return;
        }
        for &id in &patch.clear_keys {
            self.clear_provider(id);
        }
        for &id in patch.api_keys.keys() {
            self.clear_provider(id);
        }
    }

    fn clear_provider(&mut self, id: ProviderId) {
        self.cooldowns.remove(&id);
        self.auth_invalid.remove(&id);
    }
}
