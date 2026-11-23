//! AI provider identity and routing preference value objects.

use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Concrete AI provider identifier supported by the Tutor Router.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderId {
    Ollama,
    Groq,
    Gemini,
    Openrouter,
    Freellmapi,
}

impl FromStr for ProviderId {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "ollama" => Ok(Self::Ollama),
            "groq" => Ok(Self::Groq),
            "gemini" => Ok(Self::Gemini),
            "openrouter" => Ok(Self::Openrouter),
            "freellmapi" => Ok(Self::Freellmapi),
            _ => Err(()),
        }
    }
}

/// Caller routing preference (`auto` failover or a pinned concrete provider).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderPreference {
    #[default]
    Auto,
    Ollama,
    Groq,
    Gemini,
    Openrouter,
    Freellmapi,
}

impl ProviderPreference {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Ollama => "ollama",
            Self::Groq => "groq",
            Self::Gemini => "gemini",
            Self::Openrouter => "openrouter",
            Self::Freellmapi => "freellmapi",
        }
    }

    pub fn as_pinned(self) -> Option<ProviderId> {
        match self {
            Self::Auto => None,
            Self::Ollama => Some(ProviderId::Ollama),
            Self::Groq => Some(ProviderId::Groq),
            Self::Gemini => Some(ProviderId::Gemini),
            Self::Openrouter => Some(ProviderId::Openrouter),
            Self::Freellmapi => Some(ProviderId::Freellmapi),
        }
    }

    pub fn is_cloud(self) -> bool {
        matches!(
            self,
            Self::Groq | Self::Gemini | Self::Openrouter | Self::Freellmapi
        )
    }
}

impl FromStr for ProviderPreference {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "ollama" => Ok(Self::Ollama),
            "groq" => Ok(Self::Groq),
            "gemini" => Ok(Self::Gemini),
            "openrouter" => Ok(Self::Openrouter),
            "freellmapi" => Ok(Self::Freellmapi),
            _ => Err(()),
        }
    }
}
