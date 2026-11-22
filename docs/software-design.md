# SDD — FluentDesk AI
## Voice-First Software Design Document

**Version:** 2.0

### 1. Technology stack

| Layer | Technology | Purpose |
|---|---|---|
| Desktop | Tauri 2 | Native desktop shell |
| Backend | Rust | Application logic |
| Frontend | React + TypeScript | User interface |
| Audio input | CPAL | Microphone capture |
| Voice detection | WebRTC VAD | Detect speech and silence |
| Speech recognition | whisper.cpp | Speech-to-text |
| LLM runtime | Ollama | Local AI conversation |
| Alternative runtime | llama.cpp | Local GGUF inference |
| Speech synthesis | Kokoro | Natural AI voice |
| Database | SQLite | Learning history |
| Database access | SQLx | Rust database layer |
| Async runtime | Tokio | Background processing |
| Serialization | Serde | Data structures |
| Errors | thiserror | Application error types |
| Logging | tracing | Diagnostics |

### 2. Architecture overview

```text
+-----------------------------------------+
|              React Frontend             |
|                                         |
| Dashboard | Speaking Room | History     |
| Roleplay  | Evaluation    | Settings    |
+--------------------+--------------------+
                     |
                 Tauri IPC
                     |
+--------------------v--------------------+
|                Rust Core                |
|                                         |
| Conversation Manager                    |
| Audio Manager                           |
| Speech Recognition Manager              |
| AI Provider Manager                     |
| Speech Synthesis Manager                |
| Learning Evaluation Manager             |
| Session Manager                         |
| Database Manager                         |
+--------------------+--------------------+
                     |
             Speech/AI adapters
                     |
      +--------------+---------------+
      |              |               |
    CPAL          Whisper.cpp      Ollama
 Microphone          STT             LLM
      |              |               |
      +--------------+---------------+
                     |
                  Kokoro
                    TTS
                     |
               Audio playback
                     |
                  SQLite
```

### 3. Voice conversation lifecycle

The core workflow:

```text
IDLE
 |
 v
START SESSION
 |
 v
INITIALIZE MICROPHONE
 |
 v
LISTENING
 |
 v
DETECT SPEECH
 |
 v
CAPTURE AUDIO
 |
 v
DETECT END OF SPEECH
 |
 v
TRANSCRIBE AUDIO
 |
 v
PROCESS CONVERSATION
 |
 v
GENERATE AI RESPONSE
 |
 v
SYNTHESIZE SPEECH
 |
 v
PLAY AI AUDIO
 |
 v
RETURN TO LISTENING
```

The initial MVP should use automatic turn detection with a push-to-talk fallback.

Full-duplex, highly responsive interruption handling can follow as a second-stage improvement.

### 4. Rust module structure

```text
src-tauri/
|
+-- src/
|   +-- main.rs
|   +-- lib.rs
|
|   +-- commands/
|   |   +-- conversation.rs
|   |   +-- audio.rs
|   |   +-- settings.rs
|   |   +-- dashboard.rs
|
|   +-- modules/
|   |   +-- conversation/
|   |   |   +-- manager.rs
|   |   |   +-- context.rs
|   |   |   +-- turn.rs
|   |   |
|   |   +-- speech/
|   |   |   +-- audio_capture.rs
|   |   |   +-- vad.rs
|   |   |   +-- transcription.rs
|   |   |   +-- synthesis.rs
|   |   |   +-- playback.rs
|   |   |
|   |   +-- tutor/
|   |   |   +-- prompts.rs
|   |   |   +-- feedback.rs
|   |   |   +-- evaluation.rs
|   |   |
|   |   +-- learning/
|   |       +-- vocabulary.rs
|   |       +-- progress.rs
|   |
|   +-- ai/
|   |   +-- provider.rs
|   |   +-- ollama.rs
|   |   +-- llama_cpp.rs
|   |
|   +-- stt/
|   |   +-- provider.rs
|   |   +-- whisper.rs
|   |
|   +-- tts/
|   |   +-- provider.rs
|   |   +-- kokoro.rs
|   |
|   +-- database/
|       +-- connection.rs
|       +-- migrations/
|       +-- repository.rs
```

### 5. Core abstractions

Use replaceable provider interfaces.

#### AI provider

```rust
#[async_trait::async_trait]
pub trait AiProvider: Send + Sync {
    async fn generate_response(
        &self,
        request: ConversationRequest,
    ) -> Result<AiResponse, AiError>;

    async fn health_check(&self)
        -> Result<(), AiError>;
}
```

The implementation should support streaming response chunks in addition to complete responses.

#### Speech-to-text provider

```rust
#[async_trait::async_trait]
pub trait SpeechToText: Send + Sync {
    async fn transcribe(
        &self,
        audio: AudioBuffer,
    ) -> Result<Transcript, SpeechError>;
}
```

#### Text-to-speech provider

```rust
#[async_trait::async_trait]
pub trait TextToSpeech: Send + Sync {
    async fn synthesize(
        &self,
        text: String,
        voice: String,
    ) -> Result<AudioBuffer, SpeechError>;
}
```

#### Audio input

Implement an audio capture service that:

- Enumerates available microphones.
- Requests and checks microphone access.
- Captures mono PCM audio.
- Normalizes the sample format for the selected STT engine.
- Detects speech and silence.
- Handles microphone disconnection.
- Releases the audio device when the session ends.

### 6. Conversation manager

The conversation manager coordinates every speaking turn.

Responsibilities:

1. Receive the recognized transcript.
2. Load previous conversation context.
3. Load the learner's proficiency and preferences.
4. Build the tutor prompt.
5. Send the request to the selected LLM.
6. Parse the response.
7. Send the spoken answer to TTS.
8. Stream or play synthesized audio.
9. Store the turn in SQLite.
10. Return to listening.

### 7. Tutor response format

Use a structured response contract:

```json
{
  "spoken_reply": "That sounds interesting! What happened next?",
  "correction": {
    "original": "I go yesterday",
    "corrected": "I went yesterday",
    "explanation": "Use the past tense."
  },
  "vocabulary": [],
  "follow_up_question": "What did you do there?"
}
```

The `spoken_reply` must sound natural when spoken aloud.

Do not read technical correction metadata to the learner. Display optional feedback separately.

If the structured response cannot be parsed, safely fall back to the natural text response.

### 8. Database structure

Required tables:

- users
- conversations
- conversation_turns
- session_metrics
- vocabulary
- learning_progress
- settings

Suggested conversation turns:

```sql
CREATE TABLE conversation_turns (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    role TEXT NOT NULL,
    transcript TEXT,
    response_text TEXT,
    duration_ms INTEGER,
    created_at TEXT NOT NULL,
    FOREIGN KEY (conversation_id)
        REFERENCES conversations(id)
);
```

Do not retain raw microphone audio by default. Store transcripts and synthesized content metadata unless the learner explicitly enables audio recording.

### 9. Frontend structure

```text
src/
|
+-- components/
|   +-- AudioWaveform.tsx
|   +-- MicrophoneButton.tsx
|   +-- TutorAvatar.tsx
|   +-- SpeakingStatus.tsx
|   +-- TranscriptPanel.tsx
|   +-- CorrectionCard.tsx
|
+-- pages/
|   +-- Dashboard.tsx
|   +-- SpeakingRoom.tsx
|   +-- Roleplay.tsx
|   +-- Pronunciation.tsx
|   +-- SessionReview.tsx
|   +-- Settings.tsx
|
+-- services/
|   +-- audio.ts
|   +-- conversation.ts
|   +-- settings.ts
```

### 10. Tauri commands

| Command | Responsibility |
|---|---|
| start_speaking_session | Start voice session |
| stop_speaking_session | End voice session |
| pause_speaking_session | Pause microphone capture |
| resume_speaking_session | Resume capture |
| set_microphone | Select input device |
| get_audio_devices | List available devices |
| set_voice | Select AI voice |
| set_conversation_mode | Set correction style |
| get_transcript | Retrieve transcript |
| get_session_review | Retrieve evaluation |
| get_ai_status | Check AI runtime |
| configure_ai | Select LLM provider |

Use Tauri channels/events for status updates and response streaming. Avoid repeatedly sending large audio buffers as base64 strings through IPC.

### 11. AI tutor prompt

```text
You are a friendly, patient, natural English-speaking tutor.

Your main purpose is to help the learner become confident
speaking English.

Rules:

1. Always prioritize natural conversation.
2. Ask one relevant question at a time.
3. Give short, natural verbal responses.
4. Remember what the learner has said.
5. Adapt your vocabulary to their English level.
6. Do not overwhelm the learner with grammar explanations.
7. Correct important mistakes naturally.
8. Encourage the learner to speak in complete sentences.
9. Never shame or criticize mistakes.
10. Keep roleplay scenarios realistic.
11. Avoid repetitive questions.
12. Encourage the learner to explain, describe, and express opinions.

When the learner makes a mistake:
- Respond naturally first.
- Offer a short correction when appropriate.
- Explain only when useful or requested.
- Continue the conversation with a follow-up question.

Your objective is to make the learner speak more English,
not to make them complete worksheets.
```

### 12. Privacy and security

- Microphone capture must be visibly indicated.
- Users must be able to stop listening immediately.
- Do not retain microphone recordings by default.
- Cloud AI must be optional and consent-based.
- Secure stored API credentials.
- Keep local endpoints bound to localhost.
- Do not log raw speech content unnecessarily.
- Sanitize AI-generated content displayed in the frontend.
- Do not execute shell commands generated by AI.

### 13. Performance engineering

Initial targets to measure:

| Operation | Target |
|---|---|
| UI interaction | Under 100 ms |
| Local database operations | Under 200 ms |
| Speech endpoint detection | Tunable, initially 500–900 ms of silence |
| First audible AI reply | Target under 3 seconds after end of speech |
| Continuous conversation | No application restart between turns |

These are engineering targets, not hardware-independent guarantees. The model, microphone, audio buffering, CPU/GPU and TTS engine will affect latency.

### 14. Offline operation

After model installation, the complete speaking pipeline must be usable offline:

- Microphone capture.
- Voice activity detection.
- Speech transcription.
- Local LLM responses.
- Voice synthesis.
- Session history.
- Learning progress.

Cloud services must be clearly identified as online-only options.