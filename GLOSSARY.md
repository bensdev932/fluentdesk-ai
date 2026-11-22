# FluentDesk AI

Voice-first English speaking practice studio where a learner converses aloud with an AI tutor across multi-turn sessions using local speech recognition, language generation, and speech synthesis.

## Language

**Speaking Session**:
A bounded spoken practice conversation between a learner and the tutor with a single lifecycle state (`Idle`, `Initializing`, `Listening`, `Processing`, `Thinking`, `Speaking`, `Paused`, `Error`, `Ended`).
_Avoid_: Voice loop, call, chat room

**Utterance**:
A contiguous segment of learner speech captured between turn start and endpointing.
_Avoid_: Recording, audio clip, voice message

**Turn Detector**:
The endpointing mechanism that segments a stream of captured audio into a completed Utterance, either automatically via voice activity silence or manually via push-to-talk release.
_Avoid_: Chunker, audio splitter, mic listener

**Conversation Turn**:
A single chronological exchange entry recorded within a Speaking Session, authored by either the `User` (from a validated transcript) or the `Tutor` (spoken reply text).
_Avoid_: Message, chat bubble, log line

**Tutor Reply**:
The structured pedagogical response produced for a learner turn, consisting of a spoken reply, an optional grammar coaching correction, and an optional follow-up prompt.
_Avoid_: Completion, AI output, answer

**Correction Mode**:
The learner-selected intensity of grammar coaching (`Natural`, `Gentle`, `Detailed`, `Off`) applied when generating a Tutor Reply.
_Avoid_: Difficulty level, grading mode

**Tutor Router**:
The single deep module that enforces cloud privacy consent, resolves provider credentials, dispatches speaking turns across local and cloud language models, and handles rate-limit failover.
_Avoid_: AI gateway manager, LLM service wrapper

**Learner Preferences**:
The learner's persistent non-secret practice configuration governing Correction Mode, default CEFR level, speaking pace, English voice selection, push-to-talk behavior, privacy consent, and preferred AI provider.
_Avoid_: Router prefs file, user settings wrapper

**Roleplay Scenario**:
A structured English speaking context (`Free Conversation`, `Ordering at a Coffee Shop`, `Job Interview`, `Declining Politely`, `Hotel Check-In`, `Restaurant Dining`, `Doctor or Pharmacy Visit`, `Team Standup and Workplace Small Talk`) defining the Tutor's persona, the learner's role, target English vocabulary, and Communicative Goals for a Speaking Session.
_Avoid_: Prompt preset, conversation template, scenario config

**Communicative Goal**:
A concrete conversational objective within a Roleplay Scenario (such as requesting a substitution or politely declining an invitation) tracked across Conversation Turns until satisfied by the learner.
_Avoid_: Subtask, quest item, prompt checklist

**Pedagogical Scaffold**:
The bounded conversational progression governing a Speaking Session across phases (`Opener`, `FirstQuestion`, `TopicExchange`, `Closer`, `Completed`), turn budget, target CEFR level, active Roleplay Scenario, and tracked Communicative Goals.
_Avoid_: Session state bag, prompt metadata, turn counter

**Session Review**:
An asynchronous post-session pedagogical evaluation summarizing completed Communicative Goals, fluency observations, recurring grammar focuses, and CEFR-aligned feedback generated after a Speaking Session ends, always labeled as an AI Estimate rather than an official proficiency score.
_Avoid_: Exam grade, acoustic score, analytics report

**Learner Fact**:
A brief, text-only, user-deletable memory item distilled from a completed Speaking Session (such as an occupation, goal, or learning preference) used to personalize future Roleplay Scenarios without storing raw audio.
_Avoid_: User embedding, hidden profile, telemetry record

**Learner Progress**:
The cumulative English fluency snapshot distilled from completed Speaking Sessions and Session Reviews, tracking practice minutes, satisfied Communicative Goals, target English vocabulary used, and consecutive daily practice streak.
_Avoid_: Treasure chest, leaderboard score, raw session history list
