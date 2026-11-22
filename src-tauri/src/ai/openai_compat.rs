//! OpenAI-compatible chat completions adapter serving both Groq and OpenRouter free models.

use crate::ai::{
    catalog::ProviderId,
    cloud_error::{cloud_infer_err, CompatMode},
    cloud_http::{post_json, CloudFailure, CloudHttpRequest},
    cloud_http_retry::retry_openai_compat,
    cloud_stream_adapters::stream_openai_compat,
    cloud_stream_extract::extract_openai_delta,
    cloud_ws_adapters::{collect_ws_reply, is_ws_url, stream_openai_compat_ws},
    provider::{AiProvider, TutorReplyStream},
};
use crate::domain::{
    ConversationRole, ConversationTurn, CorrectionMode, PedagogicalScaffold, ProviderHealth,
    SessionError, Transcript, TutorReply,
};
use crate::modules::tutor::{
    feedback::parse_tutor_response_or_fallback,
    prompts::{assemble_voice_tutor_prompt, ChatMessage, TutorPrompt},
};
use crate::telemetry::RedactedSecret;
use async_trait::async_trait;
use serde_json::{json, Value};

const CLOUD_TIMEOUT_SECS: u64 = 20;

#[derive(Clone)]
pub struct OpenAiCompatProvider {
    provider_id: ProviderId,
    endpoint_url: String,
    model: String,
    api_key: RedactedSecret,
}

impl OpenAiCompatProvider {
    pub fn new(
        provider_id: ProviderId,
        endpoint_url: impl Into<String>,
        model: impl Into<String>,
        api_key: RedactedSecret,
    ) -> Self {
        Self {
            provider_id,
            endpoint_url: endpoint_url.into(),
            model: model.into(),
            api_key,
        }
    }

    pub fn model_name(&self) -> &str {
        &self.model
    }

    pub(crate) fn endpoint_url(&self) -> &str {
        &self.endpoint_url
    }

    pub(crate) fn provider_id(&self) -> ProviderId {
        self.provider_id
    }

    pub(crate) async fn generate_with_error(
        &self,
        history: &[ConversationTurn],
        user_transcript: &Transcript,
        correction_mode: CorrectionMode,
    ) -> Result<TutorReply, (SessionError, CloudFailure)> {
        if is_ws_url(&self.endpoint_url) {
            let s = stream_openai_compat_ws(self, history, user_transcript, correction_mode, None)
                .await;
            return collect_ws_reply(s).await;
        }
        let prompt = assemble_voice_tutor_prompt(history, user_transcript, correction_mode);
        let (raw, _) =
            retry_openai_compat(self.provider_id, &self.endpoint_url, &self.model, |cm| {
                post_json(self.build_request(&prompt, cm, false))
            })
            .await?;
        let text = extract_assistant_text(&raw).map_err(|e| (e, CloudFailure::None))?;
        parse_tutor_response_or_fallback(&text, correction_mode)
            .map_err(|e| (e, CloudFailure::None))
    }

    pub(crate) fn build_request(
        &self,
        prompt: &TutorPrompt,
        compat: CompatMode,
        stream: bool,
    ) -> CloudHttpRequest {
        let auth = RedactedSecret::new(format!("Bearer {}", self.api_key.expose_secret()));
        CloudHttpRequest {
            url: self.endpoint_url.clone(),
            secret_headers: vec![("Authorization".into(), auth)],
            extra_headers: openrouter_headers(self.provider_id),
            body: serialize_openai_body(self.provider_id, &self.model, prompt, compat, stream),
            timeout_secs: CLOUD_TIMEOUT_SECS,
        }
    }
}

#[async_trait]
impl AiProvider for OpenAiCompatProvider {
    async fn health_check(&self) -> ProviderHealth {
        if self.api_key.expose_secret().trim().is_empty() || self.model.trim().is_empty() {
            ProviderHealth::Misconfigured
        } else {
            ProviderHealth::Available
        }
    }

    async fn generate_tutor_reply(
        &self,
        history: &[ConversationTurn],
        user_transcript: &Transcript,
        correction_mode: CorrectionMode,
    ) -> Result<TutorReply, SessionError> {
        self.generate_with_error(history, user_transcript, correction_mode)
            .await
            .map_err(|(err, _)| err)
    }

    async fn stream_routed_reply(
        &self,
        history: &[ConversationTurn],
        user_transcript: &Transcript,
        correction_mode: CorrectionMode,
    ) -> Result<TutorReplyStream, SessionError> {
        self.stream_scaffolded_reply(history, user_transcript, correction_mode, None)
            .await
    }

    async fn stream_scaffolded_reply(
        &self,
        history: &[ConversationTurn],
        user_transcript: &Transcript,
        correction_mode: CorrectionMode,
        scaffold: Option<&PedagogicalScaffold>,
    ) -> Result<TutorReplyStream, SessionError> {
        stream_openai_compat(self, history, user_transcript, correction_mode, scaffold)
            .await
            .map_err(|(err, _)| err)
    }
}

fn openrouter_headers(pid: ProviderId) -> Vec<(String, String)> {
    if pid != ProviderId::Openrouter {
        return Vec::new();
    }
    vec![
        ("HTTP-Referer".into(), "http://localhost".into()),
        ("X-Title".into(), "FluentDesk AI".into()),
    ]
}

fn serialize_openai_body(
    pid: ProviderId,
    model: &str,
    prompt: &TutorPrompt,
    compat: CompatMode,
    stream: bool,
) -> String {
    let mut payload =
        json!({ "model": model, "messages": openai_messages(prompt), "temperature": 0.7 });
    compat.apply_to(&mut payload, pid, model);
    if stream {
        payload["stream"] = json!(true);
    }
    payload.to_string()
}

fn openai_messages(prompt: &TutorPrompt) -> Vec<Value> {
    let mut messages = vec![json!({ "role": "system", "content": prompt.system_instruction() })];
    if prompt.messages().is_empty() {
        messages.push(json!({ "role": "user", "content": prompt.user_prompt() }));
    } else {
        messages.extend(prompt.messages().iter().map(map_openai_turn));
    }
    messages
}

fn map_openai_turn(m: &ChatMessage) -> Value {
    let role = if matches!(m.role, ConversationRole::User) {
        "user"
    } else {
        "assistant"
    };
    json!({ "role": role, "content": m.content })
}

fn extract_assistant_text(raw: &str) -> Result<String, SessionError> {
    serde_json::from_str::<Value>(raw).map_err(|e| {
        cloud_infer_err(
            "Unreadable response from cloud AI provider.",
            Some(e.to_string()),
        )
    })?;
    extract_openai_delta(raw)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| cloud_infer_err("Cloud AI provider returned an empty response.", None))
}
