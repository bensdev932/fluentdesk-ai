//! Bounded LRU TTS audio phrase cache decorator implementing `TextToSpeech`.

use crate::domain::{AudioChunk, ProviderHealth, SessionError};
use crate::modules::speech::spoken_text::normalize_for_speech;
use crate::tts::phrase_lru::{CacheKey, PhraseLru};
use crate::tts::provider::{TextToSpeech, TtsAudioStream};
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

const DEFAULT_CACHE_CAPACITY: usize = 64;
const MAX_CACHED_TEXT_LEN: usize = 280;

/// Decorator caching short normalized TTS phrases across repeated tutor acknowledgments.
pub struct TtsPhraseCache {
    inner: Arc<dyn TextToSpeech>,
    lru: Arc<Mutex<PhraseLru>>,
}

impl TtsPhraseCache {
    pub fn new(inner: Arc<dyn TextToSpeech>) -> Self {
        Self::with_capacity(inner, DEFAULT_CACHE_CAPACITY)
    }

    pub fn with_capacity(inner: Arc<dyn TextToSpeech>, capacity: usize) -> Self {
        Self {
            inner,
            lru: Arc::new(Mutex::new(PhraseLru::new(capacity))),
        }
    }

    pub fn cached_len(&self) -> usize {
        self.lru.lock().expect("lock tts cache").entries.len()
    }
}

#[async_trait]
impl TextToSpeech for TtsPhraseCache {
    async fn health_check(&self) -> ProviderHealth {
        self.inner.health_check().await
    }

    async fn synthesize(
        &self,
        spoken_text: &str,
        speaking_speed: f32,
    ) -> Result<AudioChunk, SessionError> {
        let text = normalize_for_speech(spoken_text);
        let query = if text.is_empty() {
            spoken_text.trim().to_string()
        } else {
            text
        };
        let key = cache_key(&query, speaking_speed);
        if let Some(hit) = lookup_cache(&self.lru, &key) {
            return Ok(hit);
        }
        let chunk = self.inner.synthesize(&query, speaking_speed).await?;
        maybe_store(&self.lru, &query, key, chunk.clone());
        Ok(chunk)
    }

    async fn synthesize_stream(
        &self,
        spoken_text: &str,
        speaking_speed: f32,
    ) -> Result<TtsAudioStream, SessionError> {
        let text = normalize_for_speech(spoken_text);
        let query = if text.is_empty() {
            spoken_text.trim().to_string()
        } else {
            text
        };
        let key = cache_key(&query, speaking_speed);
        if let Some(hit) = lookup_cache(&self.lru, &key) {
            return Ok(single_chunk_stream(hit));
        }
        let rx = self.inner.synthesize_stream(&query, speaking_speed).await?;
        Ok(tee_stream_into_cache(rx, self.lru.clone(), query, key))
    }
}

fn cache_key(text: &str, speed: f32) -> CacheKey {
    (text.to_string(), (speed * 100.0).round() as u32)
}

fn lookup_cache(lru: &Mutex<PhraseLru>, key: &CacheKey) -> Option<AudioChunk> {
    lru.lock().expect("lock tts cache").get(key)
}

fn maybe_store(lru: &Mutex<PhraseLru>, query: &str, key: CacheKey, chunk: AudioChunk) {
    if query.len() <= MAX_CACHED_TEXT_LEN && !chunk.samples_f32.is_empty() {
        lru.lock().expect("lock tts cache").insert(key, chunk);
    }
}

fn single_chunk_stream(chunk: AudioChunk) -> TtsAudioStream {
    let (tx, rx) = mpsc::channel(1);
    let _ = tx.try_send(Ok(chunk));
    rx
}

fn tee_stream_into_cache(
    mut src: TtsAudioStream,
    lru: Arc<Mutex<PhraseLru>>,
    query: String,
    key: CacheKey,
) -> TtsAudioStream {
    let (tx, rx) = mpsc::channel(4);
    tokio::spawn(async move {
        let mut single: Option<AudioChunk> = None;
        let mut count = 0_usize;
        while let Some(item) = src.recv().await {
            if let Ok(ref chunk) = item {
                count += 1;
                single = if count == 1 {
                    Some(chunk.clone())
                } else {
                    None
                };
            }
            if tx.send(item).await.is_err() {
                return;
            }
        }
        if let Some(chunk) = single {
            maybe_store(&lru, &query, key, chunk);
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::TtsPhraseCache;
    use crate::domain::AudioFormat;
    use crate::tts::{mock::MockTtsProvider, provider::TextToSpeech};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct CountingTts(Arc<AtomicUsize>, MockTtsProvider);

    #[async_trait::async_trait]
    impl TextToSpeech for CountingTts {
        async fn health_check(&self) -> crate::domain::ProviderHealth {
            self.1.health_check().await
        }
        async fn synthesize(
            &self,
            t: &str,
            s: f32,
        ) -> Result<crate::domain::AudioChunk, crate::domain::SessionError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            self.1.synthesize(t, s).await
        }
    }

    #[tokio::test]
    async fn phrase_cache_hits_on_repeated_normalized_phrase_and_evicts_lru() {
        let calls = Arc::new(AtomicUsize::new(0));
        let inner = Arc::new(CountingTts(calls.clone(), MockTtsProvider::default()));
        let cache = TtsPhraseCache::with_capacity(inner, 2);

        let a1 = cache.synthesize("**Good job!** 👍", 1.0).await.unwrap();
        let a2 = cache.synthesize("Good job!", 1.0).await.unwrap();
        assert_eq!(a1.samples_f32.len(), a2.samples_f32.len());
        assert_eq!(a1.format, AudioFormat::WHISPER_16KHZ_MONO_F32);
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        cache.synthesize("Phrase two.", 1.0).await.unwrap();
        cache.synthesize("Phrase three.", 1.0).await.unwrap();
        assert_eq!(cache.cached_len(), 2);
        cache.synthesize("Good job!", 1.0).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 4);
    }
}
