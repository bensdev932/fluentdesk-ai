//! Maps low-level cloud HTTP errors and caches per-provider/model capability tiers.

use crate::ai::{
    catalog::ProviderId,
    cloud_http::{CloudFailure, CloudHttpError},
};
use crate::domain::{SessionError, SessionErrorKind};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ThinkingMode {
    BudgetZero,
    LevelMinimal,
    LevelLow,
    Omit,
}

impl ThinkingMode {
    pub(crate) fn default_for(model: &str) -> Self {
        if model.contains("2.5") {
            Self::BudgetZero
        } else {
            Self::LevelMinimal
        }
    }

    pub(crate) fn next_fallback(self) -> Option<Self> {
        match self {
            Self::BudgetZero => Some(Self::Omit),
            Self::LevelMinimal => Some(Self::LevelLow),
            Self::LevelLow => Some(Self::Omit),
            Self::Omit => None,
        }
    }

    pub(crate) fn apply_to(self, gen_cfg: &mut Value) {
        match self {
            Self::BudgetZero => gen_cfg["thinkingConfig"] = json!({ "thinkingBudget": 0 }),
            Self::LevelMinimal => gen_cfg["thinkingConfig"] = json!({ "thinkingLevel": "MINIMAL" }),
            Self::LevelLow => gen_cfg["thinkingConfig"] = json!({ "thinkingLevel": "LOW" }),
            Self::Omit => {}
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompatMode {
    Full,
    ReasoningNoJson,
    NoReasoning,
    Plain,
}

impl CompatMode {
    pub(crate) fn default_for(pid: ProviderId, model: &str) -> Self {
        match pid {
            ProviderId::Openrouter => Self::Full,
            ProviderId::Groq if is_gpt_oss(model) => Self::ReasoningNoJson,
            ProviderId::Groq if is_groq_reasoning_model(model) => Self::Full,
            _ => Self::NoReasoning,
        }
    }

    pub(crate) fn next_fallback(self) -> Option<Self> {
        match self {
            Self::Full => Some(Self::NoReasoning),
            Self::ReasoningNoJson | Self::NoReasoning => Some(Self::Plain),
            Self::Plain => None,
        }
    }

    pub(crate) fn without_json(self, pid: ProviderId, model: &str) -> Option<Self> {
        match self {
            Self::Full if pid == ProviderId::Groq && is_groq_reasoning_model(model) => {
                Some(Self::ReasoningNoJson)
            }
            Self::Full | Self::NoReasoning => Some(Self::Plain),
            Self::ReasoningNoJson | Self::Plain => None,
        }
    }

    pub(crate) fn apply_to(self, payload: &mut Value, pid: ProviderId, model: &str) {
        if matches!(self, Self::Full | Self::NoReasoning) {
            payload["response_format"] = json!({ "type": "json_object" });
        }
        if matches!(self, Self::Full | Self::ReasoningNoJson) {
            apply_reasoning_flags(payload, pid, model);
        }
    }
}

static THINKING_CACHE: OnceLock<Mutex<HashMap<String, ThinkingMode>>> = OnceLock::new();
static COMPAT_CACHE: OnceLock<Mutex<HashMap<(ProviderId, String), CompatMode>>> = OnceLock::new();

pub(crate) fn cached_thinking_mode(base_url: &str, model: &str) -> ThinkingMode {
    let map = THINKING_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    map.lock()
        .expect("lock")
        .get(&cache_key(base_url, model))
        .copied()
        .unwrap_or_else(|| ThinkingMode::default_for(model))
}

pub(crate) fn store_thinking_mode(base_url: &str, model: &str, mode: ThinkingMode) {
    let map = THINKING_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    map.lock()
        .expect("lock")
        .insert(cache_key(base_url, model), mode);
}

pub(crate) fn cached_compat_mode(pid: ProviderId, url: &str, model: &str) -> CompatMode {
    let map = COMPAT_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    map.lock()
        .expect("lock")
        .get(&(pid, cache_key(url, model)))
        .copied()
        .unwrap_or_else(|| CompatMode::default_for(pid, model))
}

pub(crate) fn store_compat_mode(pid: ProviderId, url: &str, model: &str, mode: CompatMode) {
    let map = COMPAT_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    map.lock()
        .expect("lock")
        .insert((pid, cache_key(url, model)), mode);
}

fn cache_key(url: &str, model: &str) -> String {
    format!("{url}\0{model}")
}

fn is_gpt_oss(model: &str) -> bool {
    model.to_ascii_lowercase().contains("gpt-oss")
}

fn is_groq_reasoning_model(model: &str) -> bool {
    let m = model.to_ascii_lowercase();
    is_gpt_oss(&m) || m.contains("deepseek") || m.contains("qwen") || m.contains("qwq")
}

fn apply_reasoning_flags(payload: &mut Value, pid: ProviderId, model: &str) {
    if pid == ProviderId::Openrouter {
        payload["reasoning"] = json!({ "effort": "low", "exclude": true });
    } else if pid == ProviderId::Groq && is_groq_reasoning_model(model) {
        apply_groq_reasoning(payload, model);
    }
}

fn apply_groq_reasoning(payload: &mut Value, model: &str) {
    payload["reasoning_effort"] = json!("low");
    if is_gpt_oss(model) {
        payload["include_reasoning"] = json!(false);
    } else {
        payload["reasoning_format"] = json!("hidden");
    }
}

pub(crate) fn map_cloud_err(
    provider: ProviderId,
    err: CloudHttpError,
) -> (SessionError, CloudFailure) {
    use CloudFailure::{AuthInvalid, Cooldown};
    let (lbl, d) = (provider.label(), err.message);
    let (avail, fail, msg) = match err.status {
        Some(429) => (true, Cooldown, format!("{lbl} rate limit (429).")),
        Some(401 | 403) => (true, AuthInvalid, format!("{lbl} rejected the API key.")),
        Some(s) if (500..=599).contains(&s) => (true, Cooldown, format!("{lbl} HTTP {s}.")),
        None if err.is_network => (true, Cooldown, format!("{lbl} unreachable ({d}).")),
        _ => (
            false,
            CloudFailure::None,
            format!("{lbl} request failed ({d})."),
        ),
    };
    let kind = if avail {
        SessionErrorKind::AiProviderUnavailable
    } else {
        SessionErrorKind::AiInferenceFailed
    };
    (SessionError::new(kind, msg, true, Some(d)), fail)
}

pub(crate) fn cloud_infer_err(msg: &'static str, detail: Option<String>) -> SessionError {
    SessionError::new(SessionErrorKind::AiInferenceFailed, msg, true, detail)
}
