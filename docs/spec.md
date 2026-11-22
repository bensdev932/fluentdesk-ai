# Feature Specification: Voice-First Conversation Loop

**Feature ID:** 001  
**Status:** Draft for review  
**Priority:** P0 — MVP  
**Product:** FluentDesk AI

## 1. Problem

English learners need a conversation partner to practice speaking, listening, and responding. Text-only chat and worksheet-style exercises do not provide the repeated spoken interaction this product is intended to teach.

## 2. Goal

Deliver a desktop experience in which a learner can start a session, speak English, receive a contextually relevant spoken response from an AI tutor, and continue through multiple turns without typing each message.

## 3. Non-goals

- Replacing a certified English teacher or providing formal language certification.
- Building a worksheet-first or fill-in-the-blank learning product.
- Requiring cloud inference or a paid AI account.
- Advanced phoneme-level pronunciation scoring in this feature.
- Multi-user calls or human-to-human conferencing.
- Cloud synchronization.

## 4. User stories

### US-001 — Start a speaking session

As a learner, I want to start a voice session so I can practice speaking English without typing.

**Acceptance scenarios**

1. **Given** the application is ready and a local speech/AI configuration is available, **when** I select Start Speaking, **then** the application requests/uses microphone permission, initializes the voice pipeline, and shows the Listening state only after capture is active.
2. **Given** microphone permission is denied or no input device is available, **when** I start a session, **then** the application explains the problem and offers a retry or device-selection path without entering a false Listening state.
3. **Given** the session is active, **when** I choose End Session, **then** microphone capture stops, pending work is cancelled safely, audio resources are released, and the session transcript remains available.

### US-002 — Speak and receive a spoken AI response

As a learner, I want the application to recognize my spoken English and answer aloud so I can practice a real conversation.

**Acceptance scenarios**

1. **Given** the session is Listening, **when** I speak a complete utterance and stop, **then** the system detects the end of the turn, transcribes the utterance, and displays the recognized text.
2. **Given** a valid transcript is available, **when** the local AI provider is healthy, **then** the tutor generates a response that is relevant to the transcript and previous conversation context.
3. **Given** the tutor response is ready, **when** local TTS succeeds, **then** the application plays the spoken response and displays the corresponding text.
4. **Given** playback completes, **when** the session is still active, **then** the application returns to Listening automatically without requiring the learner to click Send.
5. **Given** the speech segment contains only silence or noise and no usable transcript, **when** endpointing completes, **then** no empty conversation turn is sent to the LLM.

### US-003 — Use push-to-talk fallback

As a learner, I want a push-to-talk control so I can control when audio is captured in noisy environments.

**Acceptance scenarios**

1. **Given** the session is active, **when** I activate push-to-talk, **then** the system captures a bounded utterance until I release the control or a documented maximum duration is reached.
2. **Given** push-to-talk is released, **when** the captured audio is valid, **then** the normal transcription and response pipeline runs.
3. **Given** push-to-talk is not active, **when** the session is in push-to-talk mode, **then** the application does not capture a user utterance for submission.

### US-004 — Preserve conversational context

As a learner, I want the tutor to remember what I just said so the conversation feels coherent.

**Acceptance scenarios**

1. **Given** at least two completed turns exist, **when** I make a follow-up statement referring to earlier content, **then** the AI response uses relevant prior context.
2. **Given** the conversation approaches the configured context limit, **when** another turn is added, **then** the application applies a documented context-window strategy without silently dropping the most recent turns.
3. **Given** the session ends, **when** I reopen its history, **then** completed user transcripts and AI response text appear in the correct order.

### US-005 — Receive gentle grammar feedback

As a learner, I want useful corrections without having every conversation interrupted by grammar lectures.

**Acceptance scenarios**

1. **Given** correction mode is Natural Conversation, **when** I make a minor language error, **then** the tutor prioritizes continuing the conversation and may model a natural corrected phrase without a long explanation.
2. **Given** correction mode is Detailed, **when** a meaningful grammar error is detected, **then** the application displays the original phrase, a corrected phrase, and a concise explanation separately from the spoken reply.
3. **Given** a correction is produced, **when** the original meaning is ambiguous, **then** the tutor does not claim a single correction is certainly intended and may ask a clarifying question.

### US-006 — Handle provider failures

As a learner, I want understandable recovery behavior when local models or audio services fail.

**Acceptance scenarios**

1. **Given** the STT engine is unavailable, **when** I finish speaking, **then** the application shows a recoverable error and does not fabricate a transcript.
2. **Given** the LLM runtime is unavailable, **when** a transcript is ready, **then** the application preserves the transcript and offers retry or provider settings.
3. **Given** TTS fails, **when** the AI text response exists, **then** the text remains visible and the application offers retry or text-only playback fallback without losing the turn.
4. **Given** the microphone disconnects during a session, **when** capture fails, **then** the session transitions to an error/paused state and releases resources safely.

### US-007 — Review a session

As a learner, I want to review what was said so I can learn from the conversation.

**Acceptance scenarios**

1. **Given** a session contains completed turns, **when** I open the session review, **then** user transcripts and AI replies are shown in chronological order.
2. **Given** the session review is open, **when** grammar feedback exists, **then** it is presented as optional learning feedback, not as a replacement for the conversation transcript.
3. **Given** I have not enabled audio recording, **when** I review or reopen a session, **then** raw microphone audio is not available because it was not retained.

## 5. Functional requirements

- **FR-001:** The system MUST support starting and ending a voice session.
- **FR-002:** The system MUST show truthful microphone and pipeline states: Idle, Initializing, Listening, Processing, Thinking, Speaking, Paused, and Error.
- **FR-003:** The system MUST capture spoken audio and segment a user turn using configurable endpointing or push-to-talk.
- **FR-004:** The system MUST transcribe supported audio into text using the selected STT adapter.
- **FR-005:** The system MUST prevent empty/invalid transcripts from being sent as user turns.
- **FR-006:** The system MUST send the transcript and relevant conversation context to the configured AI provider.
- **FR-007:** The AI response MUST be available as text and, when TTS is available, spoken audio.
- **FR-008:** The system MUST automatically return to Listening after successful playback while the session remains active.
- **FR-009:** The system MUST persist completed transcripts and response text in chronological order.
- **FR-010:** The system MUST let the learner stop the microphone and end the session immediately.
- **FR-011:** The system MUST provide recoverable errors for microphone, STT, LLM, and TTS failures.
- **FR-012:** The system MUST NOT retain raw audio by default.
- **FR-013:** The system MUST work with local inference services after models are installed and configured.
- **FR-014:** Cloud inference MUST be optional and require explicit opt-in before transmitting learner content.
- **FR-015:** The system MUST preserve the voice-first interaction; text input MUST NOT be required for ordinary conversation turns.
- **FR-016:** The system MUST provide a push-to-talk fallback.
- **FR-017:** The system MUST allow correction intensity to be configured at least as Natural, Gentle, Detailed, and Off.

## 6. Quality requirements

- The UI MUST never indicate Listening unless microphone capture is active.
- Stopping a session MUST release microphone and playback resources.
- Audio buffers and conversation context MUST have explicit bounds.
- Completed transcripts MUST not be lost when a later provider stage fails.
- AI-generated correction data MUST be validated before being displayed.
- Performance MUST be measured by pipeline stage on defined hardware; no universal response-latency guarantee is imposed by this specification.

## 7. Assumptions and decisions for MVP

- One local learner profile is sufficient for the first MVP.
- The first release supports one active speaking session at a time.
- The default execution path is local STT + local LLM + local TTS.
- The first slice may use end-of-utterance turn-taking; barge-in (interrupting AI playback by speaking) is deferred unless separately specified.
- English is the initial practice language; additional target languages are out of scope.
- The system stores transcripts and session metadata but not raw audio by default.

## 8. Open questions to resolve before implementation

1. Which target operating system is first: Windows 11, Windows 10, Linux, macOS, or cross-platform from the start?
2. What is the minimum supported hardware profile (RAM, CPU, optional GPU)?
3. Should MVP begin with push-to-talk only, or include automatic voice activity detection from the first release?
4. Which local TTS engine and voice license are acceptable for the intended distribution?
5. Should barge-in be part of MVP or a later release?
6. What are the acceptable model download sizes and first-response latency on the minimum hardware profile?
