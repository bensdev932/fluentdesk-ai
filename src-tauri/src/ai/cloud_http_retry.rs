//! Shared HTTP 400 capability fallback retry loop and mode caching for cloud adapters.

use crate::ai::catalog::ProviderId;
use crate::ai::cloud_error::{
    cached_compat_mode, cached_thinking_mode, map_cloud_err, store_compat_mode,
    store_thinking_mode, CompatMode, ThinkingMode,
};
use crate::ai::cloud_http::{CloudFailure, CloudHttpError};
use crate::domain::SessionError;
use std::future::Future;

pub(crate) async fn retry_gemini<R, F, Fut>(
    base_url: &str,
    model: &str,
    run: F,
) -> Result<R, (SessionError, CloudFailure)>
where
    F: FnMut(ThinkingMode) -> Fut,
    Fut: Future<Output = Result<R, CloudHttpError>>,
{
    let initial = cached_thinking_mode(base_url, model);
    retry_with_mode_fallback(
        ProviderId::Gemini,
        initial,
        ThinkingMode::next_fallback,
        run,
        |acc| store_thinking_mode(base_url, model, acc),
    )
    .await
    .map(|(val, _)| val)
}

pub(crate) async fn retry_openai_compat<R, F, Fut>(
    pid: ProviderId,
    url: &str,
    model: &str,
    run: F,
) -> Result<(R, CompatMode), (SessionError, CloudFailure)>
where
    F: FnMut(CompatMode) -> Fut,
    Fut: Future<Output = Result<R, CloudHttpError>>,
{
    let initial = cached_compat_mode(pid, url, model);
    retry_with_mode_fallback(pid, initial, CompatMode::next_fallback, run, |acc| {
        store_compat_mode(pid, url, model, acc)
    })
    .await
}

async fn retry_with_mode_fallback<M, R, F, Fut>(
    pid: ProviderId,
    mut mode: M,
    next_mode: fn(M) -> Option<M>,
    mut run: F,
    on_accept: impl FnOnce(M),
) -> Result<(R, M), (SessionError, CloudFailure)>
where
    M: Copy,
    F: FnMut(M) -> Fut,
    Fut: Future<Output = Result<R, CloudHttpError>>,
{
    loop {
        match run(mode).await {
            Ok(val) => {
                on_accept(mode);
                return Ok((val, mode));
            }
            Err(e) if e.status == Some(400) => {
                mode = next_mode(mode).ok_or_else(|| map_cloud_err(pid, e))?;
            }
            Err(e) => return Err(map_cloud_err(pid, e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ai::catalog::ProviderId;
    use crate::ai::openai_compat::OpenAiCompatProvider;
    use crate::ai::provider::AiProvider;
    use crate::ai::router_tests::{ok_json, stub, tx};
    use crate::{domain::CorrectionMode::Natural, telemetry::RedactedSecret};

    #[tokio::test]
    async fn groq_reasoning_models_suppress_reasoning_and_cache_fallback() {
        let e400 = "HTTP/1.1 400 Bad Request\r\nContent-Length: 3\r\n\r\nbad".to_string();
        let ok = ok_json(r#"{"choices":[{"message":{"content":"{\"spokenReply\":\"Nice!\"}"}}]}"#);
        let (port, seen) = stub(vec![e400, ok.clone(), ok]).await;
        let u = format!("http://127.0.0.1:{port}/chat");
        let k = RedactedSecret::new("gsk_test");
        let p = OpenAiCompatProvider::new(ProviderId::Groq, u, "openai/gpt-oss-20b", k);
        let r1 = p
            .generate_tutor_reply(&[], &tx("Hi"), Natural)
            .await
            .unwrap();
        let r2 = p.generate_tutor_reply(&[], &tx("Next"), Natural).await;
        assert!(r1.spoken_reply == "Nice!" && r2.is_ok());
        let r = seen.lock().unwrap();
        assert!(
            r.len() == 3
                && r[0].contains("\"reasoning_effort\":\"low\"")
                && r[0].contains("\"include_reasoning\":false")
                && !r[0].contains("json_object")
        );
        assert!(!r[1].contains("reasoning_effort") && !r[1].contains("json_object"));
        assert!(!r[2].contains("reasoning_effort") && !r[2].contains("json_object"));
    }
}
