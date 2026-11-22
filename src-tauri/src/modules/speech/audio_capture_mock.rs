//! Deterministic mock implementation of `AudioInput` and live test feeder (`T006`, `T020`).

use crate::domain::{AudioChunk, AudioFormat, SessionError, SessionErrorKind};
use crate::modules::speech::audio_capture::{
    missing_device_error, AudioDeviceId, AudioInput, AudioInputDeviceInfo,
    DEFAULT_AUDIO_CHANNEL_CAPACITY,
};
use async_trait::async_trait;
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::{mpsc, Notify};

type SharedLiveTx = Arc<Mutex<Option<mpsc::Sender<AudioChunk>>>>;

/// Handle for asynchronously feeding `AudioChunk`s into a live `MockAudioInput` stream.
#[derive(Clone)]
pub struct LiveAudioFeeder {
    tx: SharedLiveTx,
    started: Arc<Notify>,
}

impl LiveAudioFeeder {
    pub async fn wait_started(&self) {
        loop {
            let notified = self.started.notified();
            if lock(&self.tx).is_some() {
                return;
            }
            notified.await;
        }
    }

    pub async fn push(&self, chunk: AudioChunk) -> bool {
        let sender = lock(&self.tx).clone();
        match sender {
            Some(tx) => tx.send(chunk).await.is_ok(),
            None => false,
        }
    }

    pub fn close(&self) {
        *lock(&self.tx) = None;
    }
}

/// Deterministic mock implementation of `AudioInput` (`T006`, `T020`).
#[derive(Clone, Default)]
pub struct MockAudioInput {
    devices: Arc<Mutex<Vec<AudioInputDeviceInfo>>>,
    scripted_chunks: Arc<Mutex<Vec<AudioChunk>>>,
    capturing: Arc<Mutex<bool>>,
    fail_with: Arc<Mutex<Option<SessionError>>>,
    requested_devices: Arc<Mutex<Vec<Option<String>>>>,
    live_tx: Option<SharedLiveTx>,
    live_started: Option<Arc<Notify>>,
}

impl MockAudioInput {
    pub fn new(devices: Vec<AudioInputDeviceInfo>, scripted_chunks: Vec<AudioChunk>) -> Self {
        Self {
            devices: Arc::new(Mutex::new(devices)),
            scripted_chunks: Arc::new(Mutex::new(scripted_chunks)),
            capturing: Arc::new(Mutex::new(false)),
            fail_with: Arc::new(Mutex::new(None)),
            requested_devices: Arc::new(Mutex::new(Vec::new())),
            live_tx: None,
            live_started: None,
        }
    }

    pub fn new_live(devices: Vec<AudioInputDeviceInfo>) -> (Self, LiveAudioFeeder) {
        let tx = Arc::new(Mutex::new(None));
        let started = Arc::new(Notify::new());
        let mock = Self {
            live_tx: Some(Arc::clone(&tx)),
            live_started: Some(Arc::clone(&started)),
            ..Self::new(devices, Vec::new())
        };
        (mock, LiveAudioFeeder { tx, started })
    }

    pub fn set_failure(&self, error: Option<SessionError>) {
        *lock(&self.fail_with) = error;
    }

    pub fn recorded_start_devices(&self) -> Vec<Option<String>> {
        lock(&self.requested_devices).clone()
    }

    pub fn simulate_device_disconnect(&self) -> SessionError {
        *lock(&self.capturing) = false;
        let err = audio_err("Microphone was disconnected during the session.", None);
        self.set_failure(Some(err.clone()));
        err
    }

    fn open_channel(&self) -> mpsc::Receiver<AudioChunk> {
        if let Some(live) = &self.live_tx {
            let (tx, rx) = mpsc::channel(DEFAULT_AUDIO_CHANNEL_CAPACITY);
            *lock(live) = Some(tx);
            if let Some(notify) = &self.live_started {
                notify.notify_waiters();
            }
            return rx;
        }
        spawn_scripted_channel(&self.scripted_chunks)
    }
}

#[async_trait]
impl AudioInput for MockAudioInput {
    async fn list_input_devices(&self) -> Result<Vec<AudioInputDeviceInfo>, SessionError> {
        check_failure(&self.fail_with)?;
        Ok(lock(&self.devices).clone())
    }

    async fn start_capture(
        &self,
        device_id: Option<&str>,
        format: AudioFormat,
    ) -> Result<mpsc::Receiver<AudioChunk>, SessionError> {
        check_failure(&self.fail_with)?;
        validate_requested_format_and_device(&self.devices, device_id, format)?;
        lock(&self.requested_devices).push(device_id.map(str::to_string));
        *lock(&self.capturing) = true;
        Ok(self.open_channel())
    }

    async fn stop_capture(&self) -> Result<(), SessionError> {
        *lock(&self.capturing) = false;
        if let Some(live) = &self.live_tx {
            *lock(live) = None;
        }
        Ok(())
    }

    fn is_capturing(&self) -> bool {
        *lock(&self.capturing)
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().expect("mock audio mutex poisoned")
}

fn check_failure(fail_with: &Mutex<Option<SessionError>>) -> Result<(), SessionError> {
    match lock(fail_with).clone() {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

fn validate_requested_format_and_device(
    devices: &Mutex<Vec<AudioInputDeviceInfo>>,
    device_id: Option<&str>,
    format: AudioFormat,
) -> Result<(), SessionError> {
    format.validate().map_err(|e| {
        audio_err(
            "Requested audio stream format is not supported.",
            Some(e.to_string()),
        )
    })?;
    let Some(req_id) = device_id else {
        return Ok(());
    };
    let candidate = AudioDeviceId::try_new(req_id)?;
    let list = lock(devices);
    if !list.iter().any(|d| d.id == candidate.as_str()) {
        return Err(missing_device_error(candidate.as_str()));
    }
    Ok(())
}

fn spawn_scripted_channel(scripted_chunks: &Mutex<Vec<AudioChunk>>) -> mpsc::Receiver<AudioChunk> {
    let chunks = lock(scripted_chunks).clone();
    let (tx, rx) = mpsc::channel(DEFAULT_AUDIO_CHANNEL_CAPACITY);
    for chunk in chunks {
        if tx.try_send(chunk).is_err() {
            break;
        }
    }
    rx
}

fn audio_err(msg: &'static str, detail: Option<String>) -> SessionError {
    SessionError::new(SessionErrorKind::AudioDeviceUnavailable, msg, true, detail)
}
