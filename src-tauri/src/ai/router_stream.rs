//! Streaming candidate failover and SSE/NDJSON routing helpers for `TutorRouter`.

use crate::ai::{
    catalog::{ProviderId, ProviderPreference},
    cloud_http::CloudFailure,
    cloud_stream_adapters::{stream_gemini, stream_openai_compat},
    gemini::GeminiProvider,
    openai_compat::OpenAiCompatProvider,
    provider::{AiProvider, TutorReplyStream, TutorStreamItem},
    router::TutorRouter,
    router_exec::{
        advance_cursor, build_candidate_list, consent_err, missing_key_err,
        resolve_ollama_provider, DispatchEnv, TRANSIENT_COOLDOWN_SECS,
    },
};
use crate::domain::{
    ConversationTurn, CorrectionMode, PedagogicalScaffold, SessionError, Transcript,
};
use std::sync::atomic::Ordering;
use tokio::sync::mpsc;

pub(crate) struct StreamTurnInput<'a> {
    pub history: &'a [ConversationTurn],
    pub transcript: &'a Transcript,
    pub mode: CorrectionMode,
    pub scaffold: Option<&'a PedagogicalScaffold>,
}

impl TutorRouter {
    /// Streams a speaking turn across routed local and cloud providers with pre-stream failover.
    pub async fn route_stream_turn(
        &self,
        history: &[ConversationTurn],
        transcript: &Transcript,
        mode: CorrectionMode,
        req_provider: Option<ProviderPreference>,
        req_model: Option<&str>,
    ) -> Result<TutorReplyStream, SessionError> {
        self.route_stream_scaffolded_turn(history, transcript, mode, None, req_provider, req_model)
            .await
    }

    pub async fn route_stream_scaffolded_turn(
        &self,
        history: &[ConversationTurn],
        transcript: &Transcript,
        mode: CorrectionMode,
        scaffold: Option<&PedagogicalScaffold>,
        req_provider: Option<ProviderPreference>,
        req_model: Option<&str>,
    ) -> Result<TutorReplyStream, SessionError> {
        let st = self.load_routing_state().await?;
        let effective = req_provider.unwrap_or(st.prefs.preferred_provider);
        let cursor = self.rr_cursor.load(Ordering::Relaxed);
        let cands = build_candidate_list(
            effective,
            st.prefs.cloud_opt_in,
            cursor,
            &self.secrets,
            |id| self.breakers.lock().expect("lock").is_healthy(id),
        )?;
        let env = DispatchEnv {
            ollama_base: &self.ollama_base,
            endpoints: &st.endpoint_overrides,
            overrides: &st.prefs.model_overrides,
            secrets: &self.secrets,
            breakers: &self.breakers,
            rr_cursor: &self.rr_cursor,
        };
        let turn = StreamTurnInput {
            history,
            transcript,
            mode,
            scaffold,
        };
        execute_stream_candidates(
            &cands,
            &env,
            &turn,
            req_model,
            effective == ProviderPreference::Auto,
        )
        .await
    }
}

async fn execute_stream_candidates(
    candidates: &[ProviderId],
    env: &DispatchEnv<'_>,
    turn: &StreamTurnInput<'_>,
    req_model: Option<&str>,
    is_auto: bool,
) -> Result<TutorReplyStream, SessionError> {
    let (mut notes, mut last_err) = (Vec::new(), None);
    for (idx, &id) in candidates.iter().enumerate() {
        if is_auto && id.is_cloud() {
            advance_cursor(env.rr_cursor, id);
        }
        let model = (idx == 0)
            .then_some(req_model)
            .flatten()
            .or_else(|| env.overrides.get(&id).map(String::as_str));
        match open_stream_provider(id, model, env, turn).await {
            Ok(rx) => return Ok(annotate_failover_stream(rx, notes)),
            Err((err, failure)) => {
                env.breakers
                    .lock()
                    .expect("lock")
                    .record(id, failure, TRANSIENT_COOLDOWN_SECS);
                notes.push(format!("{}: {}", id.as_str(), err.user_message));
                last_err = Some(err);
            }
        }
    }
    Err(last_err.unwrap_or_else(consent_err))
}

async fn open_stream_provider(
    id: ProviderId,
    model_override: Option<&str>,
    env: &DispatchEnv<'_>,
    turn: &StreamTurnInput<'_>,
) -> Result<TutorReplyStream, (SessionError, CloudFailure)> {
    let clean = model_override.map(str::trim).filter(|s| !s.is_empty());
    if id == ProviderId::Ollama {
        let p = resolve_ollama_provider(clean, env.ollama_base).await?;
        return p
            .stream_scaffolded_reply(turn.history, turn.transcript, turn.mode, turn.scaffold)
            .await
            .map_err(|e| (e, CloudFailure::None));
    }
    let model = clean.unwrap_or_else(|| id.default_model()).to_string();
    let Some((key, _)) = env.secrets.resolve(id) else {
        return Err((missing_key_err(id), CloudFailure::None));
    };
    let url = env
        .endpoints
        .get(&id)
        .cloned()
        .unwrap_or_else(|| id.resolve_endpoint_url());
    match id {
        ProviderId::Gemini => {
            stream_gemini(
                &GeminiProvider::new(url, &model, key),
                turn.history,
                turn.transcript,
                turn.mode,
                turn.scaffold,
            )
            .await
        }
        ProviderId::Groq | ProviderId::Openrouter => {
            stream_openai_compat(
                &OpenAiCompatProvider::new(id, url, &model, key),
                turn.history,
                turn.transcript,
                turn.mode,
                turn.scaffold,
            )
            .await
        }
        ProviderId::Ollama => unreachable!(),
    }
}

fn annotate_failover_stream(mut inner: TutorReplyStream, notes: Vec<String>) -> TutorReplyStream {
    if notes.is_empty() {
        return inner;
    }
    let failover_note = Some(format!("Failover: {}", notes.join("; ")));
    let (tx, rx) = mpsc::channel(16);
    tokio::spawn(async move {
        while let Some(item) = inner.recv().await {
            let mapped = match item {
                Ok(TutorStreamItem::Final(mut r)) => {
                    r.failover_note = failover_note.clone();
                    Ok(TutorStreamItem::Final(r))
                }
                other => other,
            };
            if tx.send(mapped).await.is_err() {
                break;
            }
        }
    });
    rx
}
