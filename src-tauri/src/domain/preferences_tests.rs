//! Unit tests for `LearnerPreferences`, `SpeakingSpeed`, and `RawAudioRetentionPolicy`.

use crate::domain::{
    CorrectionMode, LearnerPreferences, LearnerPreferencesPatch, ProviderId, ProviderPreference,
    RawAudioRetentionPolicy, SpeakingSpeed,
};
use std::collections::HashMap;

#[test]
fn learner_preferences_defaults_enforce_privacy_and_natural_mode() {
    let prefs = LearnerPreferences::default();
    assert_eq!(prefs.correction_mode, CorrectionMode::Natural);
    assert_eq!(prefs.speaking_speed.as_f32(), 1.0);
    assert!(!prefs.push_to_talk_enabled);
    assert!(!prefs.cloud_opt_in);
    assert!(!prefs.raw_audio_recording_opt_in);
    assert_eq!(prefs.preferred_provider, ProviderPreference::Auto);
    assert!(prefs.model_overrides.is_empty());

    let policy = RawAudioRetentionPolicy::from_preferences(&prefs);
    assert!(!policy.is_raw_audio_enabled());
    assert!(policy.require_explicit_recording_consent().is_err());
}

#[test]
fn speaking_speed_rejects_out_of_bounds_values_without_mutating_preferences() {
    let mut prefs = LearnerPreferences::default();
    for bad in [0.49_f32, 2.01, -1.0, f32::NAN, f32::INFINITY] {
        assert!(SpeakingSpeed::try_new(bad).is_err());
        let res = prefs.apply_patch(LearnerPreferencesPatch {
            speaking_speed: Some(bad),
            correction_mode: Some(CorrectionMode::Gentle),
            ..Default::default()
        });
        assert!(res.is_err());
        assert_eq!(prefs.speaking_speed.as_f32(), 1.0);
        assert_eq!(prefs.correction_mode, CorrectionMode::Natural);
    }

    prefs
        .apply_patch(LearnerPreferencesPatch {
            speaking_speed: Some(1.25),
            raw_audio_recording_opt_in: Some(true),
            ..Default::default()
        })
        .expect("valid speed");
    assert_eq!(prefs.speaking_speed.as_f32(), 1.25);
    assert!(RawAudioRetentionPolicy::from_preferences(&prefs)
        .require_explicit_recording_consent()
        .is_ok());
}

#[test]
fn disabling_cloud_opt_in_resets_pinned_cloud_provider_to_auto_and_keeps_ollama() {
    let mut prefs = LearnerPreferences::default();
    for cloud in [
        ProviderPreference::Groq,
        ProviderPreference::Gemini,
        ProviderPreference::Openrouter,
    ] {
        prefs
            .apply_patch(LearnerPreferencesPatch {
                cloud_opt_in: Some(true),
                preferred_provider: Some(cloud),
                model_overrides: HashMap::from([(
                    ProviderId::Gemini,
                    "  gemini-3.5-flash  ".into(),
                )]),
                ..Default::default()
            })
            .expect("enable cloud");
        assert_eq!(prefs.preferred_provider, cloud);
        assert_eq!(
            prefs
                .model_overrides
                .get(&ProviderId::Gemini)
                .map(String::as_str),
            Some("gemini-3.5-flash")
        );

        prefs
            .apply_patch(LearnerPreferencesPatch {
                cloud_opt_in: Some(false),
                ..Default::default()
            })
            .expect("disable cloud");
        assert!(!prefs.cloud_opt_in);
        assert_eq!(prefs.preferred_provider, ProviderPreference::Auto);
    }

    prefs
        .apply_patch(LearnerPreferencesPatch {
            cloud_opt_in: Some(true),
            preferred_provider: Some(ProviderPreference::Ollama),
            model_overrides: HashMap::from([(ProviderId::Gemini, "   ".into())]),
            ..Default::default()
        })
        .expect("pin ollama");
    assert!(!prefs.model_overrides.contains_key(&ProviderId::Gemini));
    prefs
        .apply_patch(LearnerPreferencesPatch {
            cloud_opt_in: Some(false),
            ..Default::default()
        })
        .expect("disable keeps ollama");
    assert_eq!(prefs.preferred_provider, ProviderPreference::Ollama);

    prefs
        .apply_patch(LearnerPreferencesPatch {
            preferred_provider: Some(ProviderPreference::Groq),
            ..Default::default()
        })
        .expect("cloud pinned while opted out stays auto");
    assert_eq!(prefs.preferred_provider, ProviderPreference::Auto);
}
