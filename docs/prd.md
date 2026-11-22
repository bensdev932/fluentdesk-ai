# PRD — FluentDesk AI
## Voice-First English Speaking Tutor

**Version:** 2.0  
**Product type:** AI English speaking desktop application  
**Desktop framework:** Tauri 2  
**Backend:** Rust  
**Frontend:** React + TypeScript  
**Database:** SQLite  
**AI strategy:** Local-first, with optional free cloud AI providers

### 1. Product vision

FluentDesk AI is an interactive English-speaking tutor that allows learners to practice speaking English with an AI teacher using their microphone and speakers or headphones.

The experience should resemble a real conversation with a patient English teacher.

The AI should listen, understand, respond verbally, ask follow-up questions, explain mistakes, and adapt to the learner's English proficiency.

**The primary interaction is voice-to-voice conversation.** Typing, multiple-choice quizzes, and fill-in-the-blank exercises are optional supporting features.

### 2. Target users

- Beginner English learners (A1–A2).
- Intermediate learners (B1–B2).
- Advanced learners (C1–C2).
- People who want to improve conversational English.
- Students preparing for job interviews.
- Professionals practicing business English.
- Learners who lack an English-speaking conversation partner.

### 3. Core product requirements

#### FR-001: Voice conversation

The user must be able to:

- Start a speaking session.
- Speak naturally into a microphone.
- Let the application detect when they finish speaking.
- Hear the AI tutor respond with spoken English.
- Continue without manually submitting every message.
- Pause or end a conversation.
- Interrupt the AI while it is speaking.
- See the recognized transcript.

A push-to-talk control should be available as an alternative to automatic listening.

#### FR-002: Intelligent AI tutor

The AI should:

- Understand spoken English.
- Remember conversation context.
- Ask relevant follow-up questions.
- Avoid repetitive questions.
- Adapt vocabulary to the user's ability.
- Speak naturally and conversationally.
- Explain difficult expressions when asked.
- Encourage the user to speak more.

#### FR-003: Grammar correction during conversation

The AI should recognize important mistakes and provide corrections without turning every conversation into a grammar lecture.

Correction settings:

1. Natural conversation: correct only important mistakes.
2. Gentle correction: provide short corrections after the user finishes.
3. Detailed correction: explain grammar and sentence structure.
4. No correction: prioritize fluency.

Example:

User: "Yesterday I go to the market."

AI: "Oh, you went to the market yesterday! What did you buy?"

Optional feedback:

- Original: Yesterday I go to the market.
- Better: Yesterday I went to the market.
- Explanation: Use the past tense of go, which is went.

#### FR-004: Free conversation mode

Conversation topics:

- Daily activities.
- Hobbies.
- Family and friends.
- Food and restaurants.
- Travel.
- Education.
- Work.
- Technology.
- News and general topics.
- Personal interests.

The learner can choose a topic or let the AI select one.

#### FR-005: Real-life roleplay

The AI should act as different people in realistic situations.

Examples:

- Customer and restaurant waiter.
- Hotel receptionist and guest.
- Interviewer and job applicant.
- Shop assistant and customer.
- Teacher and student.
- Business colleague and coworker.
- Airport staff and traveler.

The AI should remain in character and maintain the scenario until the learner ends it.

#### FR-006: Pronunciation practice

The user should be able to:

- Hear a model sentence.
- Repeat the sentence.
- View the speech transcription.
- Compare expected and recognized words.
- Practice difficult words.
- Repeat phrases at different speaking speeds.

Pronunciation scoring should only be provided when a genuine acoustic pronunciation assessment is available. Speech transcription accuracy alone must not be presented as a pronunciation score.

#### FR-007: Speaking evaluation

At the end of every speaking session, evaluate:

| Category | Evaluation |
|---|---|
| Fluency | Pauses and continuity |
| Grammar | Sentence correctness |
| Vocabulary | Word variety and appropriateness |
| Sentence structure | Clarity and complexity |
| Comprehension | Ability to respond appropriately |
| Pronunciation | Only when supported by an acoustic assessment |

Provide strengths, improvement areas, example corrections, and recommended speaking activities.

#### FR-008: Learning profile

Allow the user to select:

- English level: A1–C2.
- Preferred accent: American or British.
- AI teacher voice.
- Conversation speed.
- Correction style.
- Learning goals.

#### FR-009: Learning dashboard

Show:

- Total speaking time.
- Number of conversations.
- Speaking session history.
- Frequently corrected expressions.
- Vocabulary learned through conversations.
- Grammar improvement.
- Weekly practice streak.
- Progress by learning skill.

#### FR-010: AI provider configuration

Support:

- Ollama.
- llama.cpp.
- Compatible free cloud LLM APIs.
- Replaceable speech recognition engines.
- Replaceable speech synthesis engines.

Core voice functionality must not require a paid subscription.

### 4. Primary screens

1. Welcome and profile setup.
2. Speaking dashboard.
3. Live conversation room.
4. Roleplay selection.
5. Pronunciation practice.
6. Conversation history.
7. Speaking evaluation.
8. Vocabulary collected from conversations.
9. AI and voice settings.

### 5. MVP priorities

**Must have:**
- Local profile.
- Microphone capture.
- Voice activity detection.
- Speech-to-text.
- AI conversation engine.
- Text-to-speech.
- Automatic conversational turn-taking.
- Spoken responses.
- Session transcript.
- Basic grammar feedback.
- SQLite persistence.

**Should have:**
- Roleplay.
- Conversation topics.
- AI voice selection.
- Speaking session reports.
- Vocabulary collection.
- Offline operation.

**Later releases:**
- Advanced pronunciation assessment.
- Multiple AI personalities.
- Fully interruptible streaming speech.
- Advanced speech analytics.
- Cloud synchronization.

### 6. Acceptance criteria

The MVP is accepted when:

1. A user can start a conversation without typing.
2. The application recognizes spoken English.
3. The AI generates a contextually appropriate response.
4. The user hears the response through their speakers or headphones.
5. The next speaking turn starts automatically.
6. Conversation context is preserved.
7. The user can view their transcript.
8. The user can end and review a session.
9. Important grammar feedback is available.
10. The application can perform the complete voice workflow using local AI without a paid account.

### 7. Product success metrics

- Average speaking minutes per session.
- Completed voice conversation sessions.
- Successful speech recognition rate.
- Conversation turn completion rate.
- AI response relevance.
- User return rate.
- Measurable improvement in speaking performance.

### 8. Explicit exclusions

The product must not be designed primarily around:
- Fill-in-the-blank activities.
- Multiple-choice grammar quizzes.
- Text-only chatbot experiences.
- Memorization without conversational practice.