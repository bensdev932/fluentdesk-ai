use super::*;
use crate::domain::{AudioChunk, AudioFormat};
use std::io;
use std::sync::{Arc, Mutex};
use tracing_subscriber::fmt::MakeWriter;

#[derive(Clone, Default)]
struct BufferWriter {
    buf: Arc<Mutex<Vec<u8>>>,
}

impl BufferWriter {
    fn contents(&self) -> String {
        let bytes = self.buf.lock().expect("lock log buffer");
        String::from_utf8_lossy(&bytes).to_string()
    }
}

impl io::Write for BufferWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buf
            .lock()
            .expect("lock log buffer")
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for BufferWriter {
    type Writer = BufferWriter;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[test]
fn redacted_secret_never_leaks_in_debug_or_display_or_tracing() {
    let secret = RedactedSecret::new("sk-live-super-secret-99887766");
    let debug_str = format!("{secret:?}");
    let display_str = format!("{secret}");
    assert_eq!(debug_str, "[REDACTED_SECRET]");
    assert_eq!(display_str, "[REDACTED_SECRET]");

    let writer = BufferWriter::default();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(writer.clone())
        .finish();
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(api_key = ?secret, "Initializing optional provider");
    });
    let logged = writer.contents();
    assert!(logged.contains("[REDACTED_SECRET]"));
    assert!(!logged.contains("sk-live-super-secret-99887766"));
}

#[test]
fn raw_audio_chunk_samples_are_never_logged_in_tracing_events() {
    let samples = vec![0.424242_f32, -0.737373_f32, 0.919191_f32];
    let chunk =
        AudioChunk::new_bounded(AudioFormat::WHISPER_16KHZ_MONO_F32, samples, 100, 20).unwrap();
    let writer = BufferWriter::default();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(writer.clone())
        .finish();
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(?chunk, "Captured utterance audio chunk");
    });
    let logged = writer.contents();
    assert!(logged.contains("<REDACTED_RAW_AUDIO>"));
    assert!(!logged.contains("0.424242"));
}

#[test]
fn free_form_log_scrubber_redacts_bearer_groq_gemini_and_url_keys() {
    let raw = concat!(
        "Authorization: Bearer jwt_tok gsk_groqsecret123456 ",
        "x-goog-api-key: AIzaSyDummySecretKey99 ",
        "https://host/v1?key=AIzaSyUrlSecret&alt=json ",
        "{\"api_key\":\"sk-proj-ABCDEF123\"} api_key=my_secret_value_123"
    );
    let scrubbed = redact_sensitive_log_text(raw);
    for needle in [
        "jwt_tok",
        "gsk_groqsecret123456",
        "AIzaSyDummySecretKey99",
        "AIzaSyUrlSecret",
        "sk-proj-ABCDEF123",
        "my_secret_value_123",
    ] {
        assert!(
            !scrubbed.contains(needle),
            "Secret {needle} leaked in: {scrubbed}"
        );
    }
    assert!(scrubbed.contains("[REDACTED_SECRET]"));
}
