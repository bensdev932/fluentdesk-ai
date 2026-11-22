//! Google Gemini `generateContent` adapter using `x-goog-api-key` header (never URL query keys).

use crate::ai::{
    cloud_error::ThinkingMode,
    cloud_http::{post_json, CloudFailure, CloudHttpRequest},
    cloud_http_retry::retry_gemini,
    cloud_stream_adapters::stream_gemini,
    cloud_ws_adapters::{collect_ws_reply, is_ws_url, stream_gemini_ws},
    provider::{AiProvider, TutorReplyStream},
};
use crate::domain::{
    ConversationRole, ConversationTurn, CorrectionMode, PedagogicalScaffold, ProviderHealth,
    SessionError, SessionErrorKind, Transcript, TutorReply,
};
use crate::modules::tutor::{
    feedback::parse_tutor_response_or_fallback,
    prompts::{assemble_voice_tutor_prompt, ChatMessage, TutorPrompt},
};
use crate::telemetry::RedactedSecret;
use async_trait::async_trait;
use serde_json::{json, Value};

const GEMINI_TIMEOUT_SECS: u64 = 12;

#[derive(Clone)]
pub struct GeminiProvider {
    base_url: String,
    model: String,
    api_key: RedactedSecret,
}

impl GeminiProvider {
    pub fn new(
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: RedactedSecret,
    ) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            model: model.into(),
            api_key,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn model_name(&self) -> &str {
        &self.model
    }

    pub(crate) fn api_key(&self) -> &RedactedSecret {
        &self.api_key
    }

    pub(crate) async fn generate_with_error(
        &self,
        history: &[ConversationTurn],
        user_transcript: &Transcript,
        correction_mode: CorrectionMode,
    ) -> Result<TutorReply, (SessionError, CloudFailure)> {
        if is_ws_url(&self.base_url) {
            let s = stream_gemini_ws(self, history, user_transcript, correction_mode, None).await;
            return collect_ws_reply(s).await;
        }
        let prompt = assemble_voice_tutor_prompt(history, user_transcript, correction_mode);
        let raw = retry_gemini(&self.base_url, &self.model, |tm| {
            post_json(self.build_request(":generateContent", &prompt, tm))
        })
        .await?;
        let text = extract_gemini_text(&raw).map_err(|e| (e, CloudFailure::None))?;
        parse_tutor_response_or_fallback(&text, correction_mode)
            .map_err(|e| (e, CloudFailure::None))
    }

    pub(crate) fn build_stream_request(
        &self,
        prompt: &TutorPrompt,
        thinking: ThinkingMode,
    ) -> CloudHttpRequest {
        self.build_request(":streamGenerateContent?alt=sse", prompt, thinking)
    }

    fn build_request(
        &self,
        suffix: &str,
        prompt: &TutorPrompt,
        thinking: ThinkingMode,
    ) -> CloudHttpRequest {
        CloudHttpRequest {
            url: format!("{}/{}{suffix}", self.base_url, self.model),
            secret_headers: vec![("x-goog-api-key".into(), self.api_key.clone())],
            extra_headers: Vec::new(),
            body: serialize_gemini_body(prompt, thinking),
            timeout_secs: GEMINI_TIMEOUT_SECS,
        }
    }
}

#[async_trait]
impl AiProvider for GeminiProvider {
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
        stream_gemini(self, history, user_transcript, correction_mode, scaffold)
            .await
            .map_err(|(err, _)| err)
    }
}

pub(crate) fn serialize_gemini_body(prompt: &TutorPrompt, thinking: ThinkingMode) -> String {
    let mut gen_cfg = json!({ "responseMimeType": "application/json" });
    thinking.apply_to(&mut gen_cfg);
    json!({
        "systemInstruction": { "parts": [{ "text": prompt.system_instruction() }] },
        "contents": gemini_contents(prompt),
        "generationConfig": gen_cfg,
    })
    .to_string()
}

fn gemini_contents(prompt: &TutorPrompt) -> Vec<Value> {
    if prompt.messages().is_empty() {
        vec![json!({ "role": "user", "parts": [{ "text": prompt.user_prompt() }] })]
    } else {
        prompt.messages().iter().map(map_gemini_turn).collect()
    }
}

fn map_gemini_turn(m: &ChatMessage) -> Value {
    let role = match m.role {
        ConversationRole::User => "user",
        ConversationRole::Tutor => "model",
    };
    json!({ "role": role, "parts": [{ "text": m.content }] })
}

pub(crate) fn extract_gemini_text(raw: &str) -> Result<String, SessionError> {
    let parsed: Value = serde_json::from_str(raw)
        .map_err(|e| gem_err("Unreadable response from Gemini API.", Some(e.to_string())))?;
    parsed
        .get("candidates")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .find_map(extract_candidate_text)
        .ok_or_else(|| gem_err("Gemini API returned an empty response.", None))
}

fn extract_candidate_text(candidate: &Value) -> Option<String> {
    let parts = candidate.pointer("/content/parts")?.as_array()?;
    let text: String = parts
        .iter()
        .filter(|p| p.get("thought").and_then(|v| v.as_bool()) != Some(true))
        .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
        .collect();
    (!text.trim().is_empty()).then_some(text)
}

fn gem_err(msg: &'static str, detail: Option<String>) -> SessionError {
    SessionError::new(SessionErrorKind::AiInferenceFailed, msg, true, detail)
}
