# TDD — FluentDesk AI
## Voice-First Testing Design Document

**Version:** 2.0  
**Testing approach:** Test-driven development

### 1. Testing objectives

Validate that:

- Microphone input works correctly.
- Spoken English is accurately transcribed.
- The AI understands conversational context.
- The AI generates relevant responses.
- The generated responses can be spoken aloud.
- Conversations continue automatically.
- The application handles audio and AI failures.
- Learning history persists across restarts.
- The voice experience works offline.

### 2. Testing layers

**Unit tests**
- Voice activity detection.
- Audio format conversion.
- Conversation state transitions.
- Tutor prompt construction.
- Response parsing.
- Grammar feedback extraction.
- Conversation context management.

**Integration tests**
- Microphone to speech recognition.
- Speech recognition to LLM.
- LLM to TTS.
- TTS to audio playback.
- Conversation manager to SQLite.
- Local AI provider communication.

**End-to-end tests**
- Start application.
- Start speaking session.
- Speak a sentence.
- Verify transcription.
- Receive an AI response.
- Hear the response.
- Speak again.
- Complete session.
- Review transcript and evaluation.

### 3. Voice test cases

| ID | Scenario | Expected result |
|---|---|---|
| VC-001 | Start speaking session | Microphone activated |
| VC-002 | Speak English sentence | Speech captured |
| VC-003 | Stop speaking | End of speech detected |
| VC-004 | Transcribe spoken sentence | Transcript returned |
| VC-005 | Send transcript to AI | Contextual response generated |
| VC-006 | Generate AI voice | Audio produced |
| VC-007 | Play AI voice | User hears response |
| VC-008 | Start another turn | Listening resumes |
| VC-009 | Pause session | Microphone stops capturing |
| VC-010 | End session | Resources released |
| VC-011 | Disconnect microphone | Error handled gracefully |
| VC-012 | Silence detected | No unnecessary AI request |
| VC-013 | AI runtime unavailable | Recoverable error displayed |
| VC-014 | Offline mode | Local voice pipeline still works |
| VC-015 | Switch correction mode | Feedback behavior changes |

### 4. Conversation quality tests

Example test:

User says:

```text
Yesterday I go to the supermarket
and buy some foods.
```

Expected:

- Recognize the complete sentence.
- Understand that it describes a past event.
- Respond naturally.
- Identify important grammar issues.
- Keep the conversation moving.

Possible tutor response:

```text
Oh, you went to the supermarket
and bought some food!

What did you buy there?
```

Feedback:

```text
go → went
buy → bought
some foods → some food
```

The evaluator should check:

- Preservation of intended meaning.
- Grammatical correctness.
- Natural conversational tone.
- Relevance of follow-up questions.
- Appropriate correction frequency.
- Correct adaptation to proficiency level.

### 5. Audio testing

Create test audio fixtures containing:

- Clear English speech.
- Slow English speech.
- Fast English speech.
- Different English accents.
- Background noise.
- Pauses between words.
- Long sentences.
- Short sentences.
- Silence.
- Multiple speakers.

Measure:

- Word Error Rate (WER).
- Speech endpoint detection delay.
- Audio processing errors.
- Transcription latency.
- Dropped audio frames.

### 6. AI response quality testing

Evaluate:

| Metric | Purpose |
|---|---|
| Relevance | Does the AI understand the user? |
| Context retention | Does it remember previous turns? |
| Naturalness | Does the response sound conversational? |
| Grammar accuracy | Are corrections accurate? |
| Level adaptation | Is the English appropriate? |
| Follow-up quality | Does the conversation continue naturally? |

Use a combination of deterministic tests and human-reviewed evaluation samples. LLM-generated evaluations should not be treated as absolute ground truth.

### 7. TTS testing

- Verify audio is generated.
- Verify audio format and sample rate.
- Verify that long replies are handled.
- Verify selected voice configuration.
- Verify playback starts and finishes.
- Verify that playback can be interrupted.
- Verify that playback failure does not terminate the application.

### 8. Conversation lifecycle testing

Expected state machine:

```text
IDLE
  |
  v
INITIALIZING
  |
  v
LISTENING
  |
  v
PROCESSING_AUDIO
  |
  v
THINKING
  |
  v
SPEAKING
  |
  +------> LISTENING
  |
  v
ENDED
```

Test invalid state transitions and cancellation during every stage.

### 9. Performance testing

| Test | Initial target |
|---|---|
| UI responsiveness | Under 100 ms |
| Audio endpoint detection | 500–900 ms silence target |
| First AI audio response | Under 3 seconds target |
| Local DB reads | Under 200 ms |
| 30-minute speaking session | No unbounded memory growth |
| 100 conversation turns | No data corruption |

Measure performance on each target hardware profile.

### 10. Privacy and security testing

- Microphone permission handling.
- Immediate stop-listening behavior.
- No unintended audio retention.
- No API credentials in logs.
- No unauthorized cloud transmission.
- Secure local database operations.
- Safe rendering of AI text.
- Proper cleanup when sessions end.

### 11. Automated CI

```text
Code Push
   |
   v
Rust Formatting
   |
   v
Clippy
   |
   v
Unit Tests
   |
   v
Audio Processing Tests
   |
   v
AI Mock Provider Tests
   |
   v
Database Integration Tests
   |
   v
Frontend Tests
   |
   v
Desktop Build
   |
   v
Release
```

Example commands:

```bash
cargo fmt --all -- --check

cargo clippy \
  --all-targets \
  --all-features \
  -- -D warnings

cargo test --workspace

npm run test

npm run build
```

Real hardware microphone tests should run separately from normal CI using recorded audio fixtures and mocked devices.

### 12. Definition of done

The MVP is complete when a learner can have a natural, uninterrupted sequence of spoken conversations with the AI tutor without needing to type messages or complete worksheets.

The system must recognize speech, understand context, generate a relevant response, speak that response, return to listening, and preserve the session transcript.