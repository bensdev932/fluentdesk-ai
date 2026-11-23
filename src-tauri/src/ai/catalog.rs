//! Supported local and free-tier cloud AI provider metadata and curated model presets.

pub use crate::domain::{ProviderId, ProviderPreference};

pub const GROQ_DEFAULT_URL: &str = "https://api.groq.com/openai/v1/chat/completions";
pub const GEMINI_DEFAULT_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models";
pub const OPENROUTER_DEFAULT_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
pub const FREELMAPI_DEFAULT_URL: &str = "http://127.0.0.1:3001/v1/chat/completions";

pub const GROQ_FREE_MODELS: &[&str] = &["openai/gpt-oss-20b", "openai/gpt-oss-120b"];
pub const GEMINI_FREE_MODELS: &[&str] = &["gemini-3.5-flash-lite", "gemini-3.5-flash"];
pub const OPENROUTER_FREE_MODELS: &[&str] = &[
    "openrouter/free",
    "nvidia/nemotron-3.5-lightning:free",
    "nvidia/nemotron-3-ultra-550b-a55b:free",
];
pub const FREELMAPI_FREE_MODELS: &[&str] = &["auto:fast", "auto:balanced", "auto:smart", "auto"];

impl ProviderId {
    pub const ALL: [Self; 5] = [
        Self::Ollama,
        Self::Groq,
        Self::Gemini,
        Self::Openrouter,
        Self::Freellmapi,
    ];
    pub const CLOUD_ORDER: [Self; 4] =
        [Self::Groq, Self::Gemini, Self::Openrouter, Self::Freellmapi];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ollama => "ollama",
            Self::Groq => "groq",
            Self::Gemini => "gemini",
            Self::Openrouter => "openrouter",
            Self::Freellmapi => "freellmapi",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Ollama => "Ollama (Local)",
            Self::Groq => "Groq (Free Cloud)",
            Self::Gemini => "Gemini (Free Cloud)",
            Self::Openrouter => "OpenRouter (Free Cloud)",
            Self::Freellmapi => "FreeLLMAPI (Local Gateway)",
        }
    }

    pub fn is_cloud(self) -> bool {
        !matches!(self, Self::Ollama)
    }

    pub fn default_model(self) -> &'static str {
        match self {
            Self::Ollama => "llama3.2:3b",
            Self::Groq => GROQ_FREE_MODELS[0],
            Self::Gemini => GEMINI_FREE_MODELS[0],
            Self::Openrouter => OPENROUTER_FREE_MODELS[0],
            Self::Freellmapi => FREELMAPI_FREE_MODELS[0],
        }
    }

    pub fn free_models(self) -> &'static [&'static str] {
        match self {
            Self::Ollama => &[],
            Self::Groq => GROQ_FREE_MODELS,
            Self::Gemini => GEMINI_FREE_MODELS,
            Self::Openrouter => OPENROUTER_FREE_MODELS,
            Self::Freellmapi => FREELMAPI_FREE_MODELS,
        }
    }

    pub fn env_var_name(self) -> Option<&'static str> {
        match self {
            Self::Ollama => None,
            Self::Groq => Some("GROQ_API_KEY"),
            Self::Gemini => Some("GEMINI_API_KEY"),
            Self::Openrouter => Some("OPENROUTER_API_KEY"),
            Self::Freellmapi => Some("FREELMAPI_API_KEY"),
        }
    }

    pub fn resolve_endpoint_url(self) -> String {
        match self {
            Self::Ollama => String::new(),
            Self::Groq => env_url("FLUENTDESK_GROQ_BASE_URL", GROQ_DEFAULT_URL),
            Self::Gemini => env_url("FLUENTDESK_GEMINI_BASE_URL", GEMINI_DEFAULT_URL),
            Self::Openrouter => env_url("FLUENTDESK_OPENROUTER_BASE_URL", OPENROUTER_DEFAULT_URL),
            Self::Freellmapi => env_url("FLUENTDESK_FREELMAPI_BASE_URL", FREELMAPI_DEFAULT_URL),
        }
    }
}

fn env_url(var: &str, default_url: &str) -> String {
    std::env::var(var)
        .ok()
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_url.to_string())
}
