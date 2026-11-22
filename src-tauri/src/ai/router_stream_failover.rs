//! First-item streaming validation and post-200 failure classification for `TutorRouter`.

use crate::ai::{
    catalog::ProviderId,
    cloud_http::CloudFailure,
    provider::{TutorReplyStream, TutorStreamItem},
    router_exec::TRANSIENT_COOLDOWN_SECS,
    router_types::CircuitBreakers,
};
use crate::domain::{SessionError, SessionErrorKind};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// Awaits the first emitted `TutorStreamItem` before committing to a streaming provider.
/// Any error or premature EOF before sentence 1 triggers circuit-breaker updates and failover.
pub(crate) async fn verify_first_stream_item(
    id: ProviderId,
    mut rx: TutorReplyStream,
) -> Result<(TutorStreamItem, TutorReplyStream), (SessionError, CloudFailure)> {
    match rx.recv().await {
        Some(Ok(first)) => Ok((first, rx)),
        Some(Err(err)) => {
            let failure = classify_stream_failure(id, &err);
            Err((err, failure))
        }
        None => Err((empty_stream_err(id), CloudFailure::Cooldown)),
    }
}

/// Re-emits the verified first item followed by remaining stream items, updating breakers on mid-stream errors.
pub(crate) fn wrap_verified_stream(
    id: ProviderId,
    first: TutorStreamItem,
    mut inner: TutorReplyStream,
    notes: Vec<String>,
    breakers: Arc<Mutex<CircuitBreakers>>,
) -> TutorReplyStream {
    let failover_note = (!notes.is_empty()).then(|| format!("Failover: {}", notes.join("; ")));
    let (tx, rx) = mpsc::channel(16);
    tokio::spawn(async move {
        let mut saw_final = matches!(first, TutorStreamItem::Final(_));
        if tx
            .send(Ok(apply_note(first, &failover_note)))
            .await
            .is_err()
        {
            return;
        }
        let mut reported_err = false;
        while let Some(item) = inner.recv().await {
            match &item {
                Ok(TutorStreamItem::Final(_)) => saw_final = true,
                Err(err) => {
                    reported_err = true;
                    record_breaker(&breakers, id, classify_stream_failure(id, err));
                }
                Ok(TutorStreamItem::Sentence(_)) => {}
            }
            let mapped = item.map(|it| apply_note(it, &failover_note));
            if tx.send(mapped).await.is_err() {
                return;
            }
        }
        if !saw_final && !reported_err && id.is_cloud() {
            record_breaker(&breakers, id, CloudFailure::Cooldown);
        }
    });
    rx
}

fn record_breaker(breakers: &Arc<Mutex<CircuitBreakers>>, id: ProviderId, failure: CloudFailure) {
    breakers
        .lock()
        .expect("lock")
        .record(id, failure, TRANSIENT_COOLDOWN_SECS);
}

fn apply_note(item: TutorStreamItem, note: &Option<String>) -> TutorStreamItem {
    match (item, note) {
        (TutorStreamItem::Final(mut r), Some(n)) => {
            r.failover_note = Some(n.clone());
            TutorStreamItem::Final(r)
        }
        (other, _) => other,
    }
}

fn classify_stream_failure(id: ProviderId, err: &SessionError) -> CloudFailure {
    if !id.is_cloud() {
        return CloudFailure::None;
    }
    if err.user_message.contains("rejected the API key") {
        return CloudFailure::AuthInvalid;
    }
    if err.kind == SessionErrorKind::AiProviderUnavailable
        || err
            .user_message
            .to_ascii_lowercase()
            .contains("empty response")
    {
        return CloudFailure::Cooldown;
    }
    CloudFailure::None
}

fn empty_stream_err(id: ProviderId) -> SessionError {
    SessionError::new(
        SessionErrorKind::AiProviderUnavailable,
        format!("{} stream closed without a reply.", id.label()),
        true,
        None,
    )
}
