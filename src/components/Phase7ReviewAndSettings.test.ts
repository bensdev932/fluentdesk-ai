import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { listPureEnglishVoices } from '../services/browserVoice';
import { createFakeSpeakingSessionPort } from '../test/voiceTestStubs';
import { CORRECTION_MODES } from '../types/domain';
import { formatCorrectionModeDescription } from './CorrectionModeSelector';
import { EnglishPracticeHub } from './EnglishPracticeHub';
import { EnglishVoiceBar } from './EnglishVoiceBar';
import { LearnerFactsCard } from './LearnerFactsCard';
import { LearnerProgressStrip } from './LearnerProgressStrip';
import { ScenarioGoalBar } from './ScenarioGoalBar';
import { SessionReviewCard } from './SessionReviewCard';
import { TutorReplyCard } from './TutorReplyCard';
import { VoiceOrbStage } from './VoiceOrbStage';
import {
  computeUsedVocabulary,
  createInitialSpeakingRoomState,
  reduceVoiceLoopEvent,
} from './speakingRoomController';

describe('Pure-English Roleplay Hub, In-Stage Session Review, and Preferences', () => {
  it('supports switching all four CorrectionModes via unified port', async () => {
    const port = createFakeSpeakingSessionPort();
    const started = await port.startSpeakingSession({ correctionMode: 'Natural', pushToTalk: false });
    expect(started.correctionMode).toBe('Natural');

    for (const mode of CORRECTION_MODES) {
      const updated = await port.updateCorrectionMode(mode);
      expect(updated).toBe(mode);
      expect(formatCorrectionModeDescription(mode).length).toBeGreaterThan(0);
    }
    const status = await port.getSessionStatus(started.sessionId);
    expect(status.correctionMode).toBe('Off');
  });

  it('populates spoken English opener on session start and in-stage AI Estimate review on session end', () => {
    const initial = createInitialSpeakingRoomState();
    const started = reduceVoiceLoopEvent(initial, {
      type: 'session-state-changed',
      sessionId: 's-1',
      state: 'Listening',
      openerReply: {
        spokenReply: 'Good morning! Welcome to Sunrise Cafe. What can I get started for you today?',
        followUpPrompt: 'What drink or size would you like to order?',
      },
      scaffold: {
        phase: 'Opener',
        turnCount: 0,
        maxTurns: 6,
        cefrLevel: 'A2',
        topic: 'Ordering at a Coffee Shop',
        scenarioId: 'cafe_order',
        targetVocabulary: ['oat milk', 'iced latte', 'muffin'],
      },
    });
    expect(started.latestReply?.spokenReply).toContain('Sunrise Cafe');
    expect(started.turns).toHaveLength(1);
    expect(started.turns[0].role).toBe('Tutor');
    expect(started.targetVocabulary).toContain('oat milk');

    const withUserTurn = reduceVoiceLoopEvent(started, {
      type: 'user-transcript-ready',
      sessionId: 's-1',
      turn: {
        id: 'u-1',
        sessionId: 's-1',
        role: 'User',
        text: 'Could I have an iced latte with oat milk please?',
        timestampUnixMs: 1000,
      },
    });
    expect(
      computeUsedVocabulary(withUserTurn.targetVocabulary, withUserTurn.turns),
    ).toEqual(['oat milk', 'iced latte']);

    const ended = reduceVoiceLoopEvent(withUserTurn, {
      type: 'session-state-changed',
      sessionId: 's-1',
      state: 'Ended',
      sessionReview: {
        sessionId: 's-1',
        scenarioId: 'cafe_order',
        estimateLabel: 'AI Estimate',
        summary: 'Ordering at a Coffee Shop: 2/3 goals met. AI Estimate.',
        cefrSignal: 'A2',
        completedGoalIds: ['order_drink'],
        grammarFocuses: ['Use could I'],
        vocabularyUsed: ['oat milk', 'iced latte'],
        turnCount: 1,
        learnerWordCount: 10,
        createdAtUnixMs: 2000,
      },
    });
    expect(ended.sessionReview?.estimateLabel).toBe('AI Estimate');

    const endedHtml = renderToStaticMarkup(
      createElement(VoiceOrbStage, {
        state: 'Ended',
        review: ended.sessionReview,
        onBargeIn: () => undefined,
        onRestart: () => undefined,
        onReturnToHub: () => undefined,
      }),
    );
    expect(endedHtml).toContain('AI Estimate');
    expect(endedHtml).toContain('Practice Again');
    expect(endedHtml).toContain('Back to Practice Hub');
  });

  it('renders pure-English Hub, Progress Strip, English-only voices, and Replay/Slow controls', () => {
    const voices = listPureEnglishVoices();
    expect(voices.length).toBeGreaterThanOrEqual(3);
    expect(voices.every((v) => v.lang.toLowerCase().startsWith('en'))).toBe(true);

    const voiceHtml = renderToStaticMarkup(
      createElement(EnglishVoiceBar, {
        speakingSpeed: 1.0,
        englishVoiceId: 'en-US',
        voices,
        onChangeSpeed: () => undefined,
        onChangeVoice: () => undefined,
      }),
    );
    expect(voiceHtml).toContain('English (US)');
    expect(voiceHtml).toContain('0.75x');

    const progressHtml = renderToStaticMarkup(
      createElement(LearnerProgressStrip, {
        progress: {
          totalSessions: 4,
          practiceMinutes: 18,
          completedGoalsCount: 9,
          vocabularyUsed: ['oat milk', 'deadline'],
          dayStreak: 3,
        },
      }),
    );
    expect(progressHtml).toContain('Minutes Practiced');
    expect(progressHtml).toContain('18');
    expect(progressHtml).toContain('Day Streak');

    const hubHtml = renderToStaticMarkup(
      createElement(EnglishPracticeHub, {
        scenarios: [
          {
            id: 'restaurant_dining',
            title: 'Restaurant Dining',
            summary: 'Order a meal in English.',
            tutorPersona: 'Friendly server',
            learnerRole: 'Diner',
            defaultCefr: 'A2',
            defaultMaxTurns: 6,
            targetVocabulary: ['recommend', 'special'],
            goals: [{ id: 'g1', description: 'Order main dish' }],
          },
        ],
        selectedScenarioId: 'restaurant_dining',
        onSelectScenario: () => undefined,
        onStartScenario: () => undefined,
      }),
    );
    expect(hubHtml).toContain('Restaurant Dining');
    expect(hubHtml).toContain('Practice This Scenario');

    const goalHtml = renderToStaticMarkup(
      createElement(ScenarioGoalBar, {
        goals: [{ id: 'g1', description: 'Order main dish' }],
        completedGoalIds: ['g1'],
        targetVocabulary: ['recommend', 'special'],
        usedVocabulary: ['recommend'],
      }),
    );
    expect(goalHtml).toContain('✓ recommend');

    const replyHtml = renderToStaticMarkup(
      createElement(TutorReplyCard, {
        reply: { spokenReply: 'Welcome to Sunrise Cafe!' },
        playbackWarning: null,
        onReplay: () => undefined,
        onReplaySlow: () => undefined,
      }),
    );
    expect(replyHtml).toContain('Replay');
    expect(replyHtml).toContain('Slow (0.75x)');

    const revHtml = renderToStaticMarkup(
      createElement(SessionReviewCard, {
        review: {
          sessionId: 's1', scenarioId: 'cafe_order', estimateLabel: 'AI Estimate',
          summary: 'Ordering at a Coffee Shop: 2/3 goals met. AI Estimate.', cefrSignal: 'A2',
          completedGoalIds: ['order_drink'], grammarFocuses: ['Use could I'],
          vocabularyUsed: ['oat milk'], turnCount: 2, learnerWordCount: 18, createdAtUnixMs: 1000,
        },
      }),
    );
    expect(revHtml).toContain('AI Estimate');
    const memHtml = renderToStaticMarkup(
      createElement(LearnerFactsCard, {
        facts: [{ id: 'f1', factText: 'My name is Maya', createdAtUnixMs: 1000 }],
        onDeleteFact: () => undefined, onClearFacts: () => undefined,
      }),
    );
    expect(memHtml).toContain('My name is Maya');
  });
});
