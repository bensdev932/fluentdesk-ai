//! Consolidated learner voice, CEFR, script, and routing preferences (`T038`).

pub use crate::domain::voice_prefs::{
    validate_english_voice_id, SpeakingSpeed, DEFAULT_ENGLISH_VOICE_ID,
};
use crate::domain::{CefrLevel, CorrectionMode, ProviderId, ProviderPreference, SessionError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

fn default_english_voice_id() -> String {
    DEFAULT_ENGLISH_VOICE_ID.to_string()
}

/// Consolidated non-secret learner voice and routing preferences stored in SQLite.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearnerPreferences {
    pub correction_mode: CorrectionMode,
    pub speaking_speed: SpeakingSpeed,
    pub push_to_talk_enabled: bool,
    pub cloud_opt_in: bool,
    pub raw_audio_recording_opt_in: bool,
    pub preferred_provider: ProviderPreference,
    pub model_overrides: HashMap<ProviderId, String>,
    #[serde(default)]
    pub cefr_level: CefrLevel,
    #[serde(default = "default_english_voice_id")]
    pub english_voice_id: String,
    #[serde(default)]
    pub pre_conversation_script_enabled: bool,
}

impl Default for LearnerPreferences {
    fn default() -> Self {
        Self {
            correction_mode: CorrectionMode::default(),
            speaking_speed: SpeakingSpeed::default(),
            push_to_talk_enabled: false,
            cloud_opt_in: false,
            raw_audio_recording_opt_in: false,
            preferred_provider: ProviderPreference::default(),
            model_overrides: HashMap::new(),
            cefr_level: CefrLevel::default(),
            english_voice_id: default_english_voice_id(),
            pre_conversation_script_enabled: false,
        }
    }
}

/// Partial update payload applied atomically to [`LearnerPreferences`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearnerPreferencesPatch {
    pub correction_mode: Option<CorrectionMode>,
    pub speaking_speed: Option<f32>,
    pub push_to_talk_enabled: Option<bool>,
    pub cloud_opt_in: Option<bool>,
    pub raw_audio_recording_opt_in: Option<bool>,
    pub preferred_provider: Option<ProviderPreference>,
    #[serde(default)]
    pub model_overrides: HashMap<ProviderId, String>,
    #[serde(default)]
    pub cefr_level: Option<CefrLevel>,
    #[serde(default)]
    pub english_voice_id: Option<String>,
    #[serde(default)]
    pub pre_conversation_script_enabled: Option<bool>,
}

impl LearnerPreferences {
    /// Applies a partial patch, enforcing speaking-speed, English voice, and cloud opt-in invariants.
    pub fn apply_patch(&mut self, patch: LearnerPreferencesPatch) -> Result<(), SessionError> {
        let next_speed = patch
            .speaking_speed
            .map(SpeakingSpeed::try_new)
            .transpose()?;
        let next_voice = patch
            .english_voice_id
            .as_deref()
            .map(validate_english_voice_id)
            .transpose()?;
        self.apply_scalar_fields(next_speed, next_voice, &patch);
        merge_model_overrides(&mut self.model_overrides, patch.model_overrides);
        self.enforce_cloud_consent_invariant();
        Ok(())
    }

    fn apply_scalar_fields(
        &mut self,
        next_speed: Option<SpeakingSpeed>,
        next_voice: Option<String>,
        patch: &LearnerPreferencesPatch,
    ) {
        if let Some(speed) = next_speed {
            self.speaking_speed = speed;
        }
        if let Some(voice) = next_voice {
            self.english_voice_id = voice;
        }
        if let Some(mode) = patch.correction_mode {
            self.correction_mode = mode;
        }
        if let Some(ptt) = patch.push_to_talk_enabled {
            self.push_to_talk_enabled = ptt;
        }
        if let Some(cloud) = patch.cloud_opt_in {
            self.cloud_opt_in = cloud;
        }
        if let Some(raw) = patch.raw_audio_recording_opt_in {
            self.raw_audio_recording_opt_in = raw;
        }
        if let Some(pref) = patch.preferred_provider {
            self.preferred_provider = pref;
        }
        if let Some(cefr) = patch.cefr_level {
            self.cefr_level = cefr;
        }
        if let Some(script) = patch.pre_conversation_script_enabled {
            self.pre_conversation_script_enabled = script;
        }
        if let Some(script) = patch.pre_conversation_script_enabled {
            self.pre_conversation_script_enabled = script;
        }
    }

    /// Resets any pinned cloud provider back to `Auto` whenever cloud opt-in is disabled.
    pub fn enforce_cloud_consent_invariant(&mut self) {
        if !self.cloud_opt_in && self.preferred_provider.is_cloud() {
            self.preferred_provider = ProviderPreference::Auto;
        }
    }
}

fn merge_model_overrides(
    target: &mut HashMap<ProviderId, String>,
    updates: HashMap<ProviderId, String>,
) {
    for (id, raw_model) in updates {
        let trimmed = raw_model.trim();
        if trimmed.is_empty() {
            target.remove(&id);
        } else {
            target.insert(id, trimmed.to_string());
        }
    }
}
