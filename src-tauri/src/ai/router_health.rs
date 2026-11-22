//! Health evaluation and safe descriptor views for `TutorRouter` (keys never leave backend).

use crate::ai::router_health_view::{ollama_overall_health, try_cloud_overall_health};
use crate::ai::{
    catalog::{ProviderId, ProviderPreference},
    ollama::OllamaConfig,
    ollama_discovery::check_local_ollama_health,
    router_types::{ProviderDescriptorView, ProvidersSnapshot, RouterHealthView},
    secrets::{KeySource, RuntimeSecretStore},
};
use crate::domain::ProviderHealth;
use std::collections::HashMap;

pub(crate) async fn evaluate_overall_health(
    ollama_base: &OllamaConfig,
    secrets: &RuntimeSecretStore,
    cloud_opt_in: bool,
    preferred: ProviderPreference,
    overrides: &HashMap<ProviderId, String>,
    status_fn: impl Fn(ProviderId) -> (u64, bool),
) -> RouterHealthView {
    let cloud_views: Vec<ProviderDescriptorView> = ProviderId::CLOUD_ORDER
        .into_iter()
        .map(|id| describe_cloud_view(id, cloud_opt_in, overrides, secrets, status_fn(id)))
        .collect();
    if let Some(view) = try_cloud_overall_health(cloud_opt_in, preferred, &cloud_views) {
        return view;
    }
    let ollama_check = check_local_ollama_health(ollama_base).await;
    ollama_overall_health(&describe_ollama_view(overrides, secrets, &ollama_check))
}

pub(crate) async fn build_providers_snapshot(
    ollama_base: &OllamaConfig,
    secrets: &RuntimeSecretStore,
    cloud_opt_in: bool,
    preferred: ProviderPreference,
    overrides: &HashMap<ProviderId, String>,
    status_fn: impl Fn(ProviderId) -> (u64, bool),
) -> ProvidersSnapshot {
    let ollama_check = check_local_ollama_health(ollama_base).await;
    let providers =
        describe_all_providers(cloud_opt_in, overrides, secrets, &ollama_check, status_fn);
    ProvidersSnapshot {
        cloud_opt_in,
        preferred_provider: preferred,
        providers,
    }
}

fn describe_all_providers(
    cloud_opt_in: bool,
    overrides: &HashMap<ProviderId, String>,
    secrets: &RuntimeSecretStore,
    ollama_check: &Result<String, (ProviderHealth, String)>,
    status_fn: impl Fn(ProviderId) -> (u64, bool),
) -> Vec<ProviderDescriptorView> {
    ProviderId::ALL
        .into_iter()
        .map(|id| {
            let (cooldown_secs, auth_invalid) = status_fn(id);
            describe_provider(
                id,
                cloud_opt_in,
                overrides,
                secrets,
                cooldown_secs,
                auth_invalid,
                ollama_check,
            )
        })
        .collect()
}

fn describe_provider(
    id: ProviderId,
    cloud_opt_in: bool,
    overrides: &HashMap<ProviderId, String>,
    secrets: &RuntimeSecretStore,
    cooldown_secs: u64,
    auth_invalid: bool,
    ollama_check: &Result<String, (ProviderHealth, String)>,
) -> ProviderDescriptorView {
    match id {
        ProviderId::Ollama => describe_ollama_view(overrides, secrets, ollama_check),
        _ => describe_cloud_view(
            id,
            cloud_opt_in,
            overrides,
            secrets,
            (cooldown_secs, auth_invalid),
        ),
    }
}

fn describe_ollama_view(
    overrides: &HashMap<ProviderId, String>,
    secrets: &RuntimeSecretStore,
    ollama_check: &Result<String, (ProviderHealth, String)>,
) -> ProviderDescriptorView {
    let id = ProviderId::Ollama;
    let override_model = overrides.get(&id).cloned();
    let fields = describe_ollama(id, override_model, ollama_check);
    build_descriptor_view(id, secrets.key_source(id), 0, false, fields)
}

fn describe_cloud_view(
    id: ProviderId,
    cloud_opt_in: bool,
    overrides: &HashMap<ProviderId, String>,
    secrets: &RuntimeSecretStore,
    status: (u64, bool),
) -> ProviderDescriptorView {
    let key_source = secrets.key_source(id);
    let override_model = overrides.get(&id).cloned();
    let fields = describe_cloud(
        id,
        cloud_opt_in,
        key_source,
        status.0,
        status.1,
        override_model,
    );
    build_descriptor_view(id, key_source, status.0, status.1, fields)
}

fn build_descriptor_view(
    id: ProviderId,
    key_source: KeySource,
    cooldown_secs: u64,
    auth_invalid: bool,
    fields: (bool, bool, String, Option<String>),
) -> ProviderDescriptorView {
    ProviderDescriptorView {
        id,
        label: id.label(),
        is_cloud: id.is_cloud(),
        configured: fields.0,
        key_source,
        default_model: id.default_model(),
        free_models: id.free_models().to_vec(),
        selected_model: fields.2,
        cooldown_secs,
        auth_invalid: id.is_cloud() && auth_invalid,
        available: fields.1,
        status_detail: fields.3,
    }
}

fn describe_ollama(
    id: ProviderId,
    override_model: Option<String>,
    ollama_check: &Result<String, (ProviderHealth, String)>,
) -> (bool, bool, String, Option<String>) {
    match ollama_check {
        Ok(m) => (
            true,
            true,
            override_model.unwrap_or_else(|| m.clone()),
            None,
        ),
        Err((_, e)) => (
            true,
            false,
            override_model.unwrap_or_else(|| id.default_model().to_string()),
            Some(e.clone()),
        ),
    }
}

fn describe_cloud(
    id: ProviderId,
    cloud_opt_in: bool,
    key_source: KeySource,
    cooldown_secs: u64,
    auth_invalid: bool,
    override_model: Option<String>,
) -> (bool, bool, String, Option<String>) {
    let has_key = key_source != KeySource::None;
    let avail = cloud_opt_in && has_key && cooldown_secs == 0 && !auth_invalid;
    let model = override_model.unwrap_or_else(|| id.default_model().to_string());
    let detail = if auth_invalid {
        Some(format!("{} API key was rejected (401/403).", id.label()))
    } else if cooldown_secs > 0 {
        Some(format!("{} in cooldown ({cooldown_secs}s).", id.label()))
    } else {
        None
    };
    (has_key, avail, model, detail)
}
