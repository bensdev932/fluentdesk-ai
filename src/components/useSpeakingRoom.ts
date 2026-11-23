import { useEffect, useState } from 'react';
import { listPureEnglishVoices, subscribeEnglishVoices, type EnglishVoiceOption } from '../services/browserVoice';
import { DEFAULT_ROLEPLAY_SCENARIOS } from '../services/scenariosFallback';
import type { SpeakingSessionPort } from '../services/tauriService';
import type {
  CefrLevel,
  CorrectionMode,
  LearnerFact,
  LearnerProgress,
  RoleplayScenario,
} from '../types/domain';
import {
  computeUsedVocabulary,
  createInitialSpeakingRoomState,
  getSpeakingControlPermissions,
  reduceVoiceLoopEvent,
  type SpeakingControlPermissions,
  type SpeakingRoomViewModel,
} from './speakingRoomController';

const INITIAL_PROGRESS: LearnerProgress = {
  totalSessions: 0,
  practiceMinutes: 0,
  completedGoalsCount: 0,
  vocabularyUsed: [],
  dayStreak: 0,
};

export interface SpeakingRoomHandle {
  vm: SpeakingRoomViewModel;
  perms: SpeakingControlPermissions;
  scenarios: RoleplayScenario[];
  learnerFacts: LearnerFact[];
  learnerProgress: LearnerProgress;
  englishVoices: EnglishVoiceOption[];
  usedVocabulary: string[];
  start(scenarioOverride?: string): void;
  pause(): void;
  resume(): void;
  stop(): void;
  bargeIn(): void;
  returnToHub(): void;
  replaySpeech(text: string, rateOverride?: number): void;
  updateCorrectionMode(mode: CorrectionMode): void;
  updateCefrLevel(level: CefrLevel): void;
  updateScenarioId(scenarioId: string): void;
  updateTopic(topic: string): void;
  updateSpeakingSpeed(speed: number): void;
  updateEnglishVoice(voiceId: string): void;
  togglePreConversationScript(): void;
  togglePushToTalk(pushToTalk: boolean): void;
  holdPushToTalk(pttHeld: boolean): void;
  submitText(text: string): void;
  simulateSample(): void;
  deleteFact(factId: string): void;
  clearFacts(): void;
}

export function useSpeakingRoom(sessionClient: SpeakingSessionPort): SpeakingRoomHandle {
  const [vm, setVm] = useState(createInitialSpeakingRoomState);
  const [scenarios, setScenarios] = useState<RoleplayScenario[]>(DEFAULT_ROLEPLAY_SCENARIOS);
  const [learnerFacts, setLearnerFacts] = useState<LearnerFact[]>([]);
  const [learnerProgress, setLearnerProgress] = useState<LearnerProgress>(INITIAL_PROGRESS);
  const [englishVoices, setEnglishVoices] = useState<EnglishVoiceOption[]>(() => listPureEnglishVoices());

  useEffect(() => {
    let active = true;
    const unsubVoices = subscribeEnglishVoices((voices) => { if (active) setEnglishVoices(voices); });
    const refreshFactsAndProgress = (): void => {
      void sessionClient.listLearnerFacts?.().then((facts) => {
        if (active) setLearnerFacts(facts);
      }).catch(() => undefined);
      void sessionClient.getLearnerProgress?.().then((prog) => {
        if (active) setLearnerProgress(prog);
      }).catch(() => undefined);
    };
    refreshFactsAndProgress();
    void sessionClient.getPreferences?.().then((prefs) => {
      if (!active || !prefs) return;
      setVm((s) => ({
        ...s,
        correctionMode: prefs.correctionMode,
        cefrLevel: prefs.cefrLevel,
        pushToTalk: prefs.pushToTalkEnabled,
        speakingSpeed: prefs.speakingSpeed,
        englishVoiceId: prefs.englishVoiceId || 'en-US',
        preConversationScriptEnabled: Boolean(prefs.preConversationScriptEnabled),
      }));
    }).catch(() => undefined);
    void sessionClient.listScenarios?.().then((list) => {
      if (active && list.length > 0) setScenarios(list);
    }).catch(() => undefined);

    const unsubscribe = sessionClient.subscribeVoiceEvents((ev) => {
      setVm((s) => reduceVoiceLoopEvent(s, ev));
      if (ev.type === 'session-state-changed' && ev.state === 'Ended') {
        refreshFactsAndProgress();
        if (!ev.sessionReview && sessionClient.getSessionReview) {
          void sessionClient.getSessionReview(ev.sessionId).then((rev) => {
            if (active && rev) setVm((s) => ({ ...s, sessionReview: rev }));
          }).catch(() => undefined);
        }
      }
    });
    return () => { active = false; unsubVoices(); unsubscribe(); };
  }, [sessionClient]);

  const ignoreError = <T,>(promise: Promise<T>): void => { void promise.catch(() => undefined); };
  const applyScenarioSelection = (scenarioId: string): RoleplayScenario | undefined => {
    const picked = scenarios.find((item) => item.id === scenarioId);
    setVm((s) =>
      picked
        ? {
            ...s,
            scenarioId,
            topic: picked.title,
            targetVocabulary: picked.targetVocabulary,
            goals: picked.goals,
            completedGoalIds: [],
            maxTurns: picked.defaultMaxTurns,
          }
        : { ...s, scenarioId },
    );
    return picked;
  };

  return {
    vm,
    perms: getSpeakingControlPermissions(vm),
    scenarios,
    learnerFacts,
    learnerProgress,
    englishVoices,
    usedVocabulary: computeUsedVocabulary(vm.targetVocabulary, vm.turns, vm.sessionReview?.vocabularyUsed),
    start: (scenarioOverride) => {
      const picked = scenarioOverride ? applyScenarioSelection(scenarioOverride) : scenarios.find((s) => s.id === vm.scenarioId);
      ignoreError(
        sessionClient.startSpeakingSession({
          correctionMode: vm.correctionMode,
          pushToTalk: vm.pushToTalk,
          cefrLevel: vm.cefrLevel,
          scenarioId: picked ? picked.id : vm.scenarioId,
          topic: picked ? picked.title : vm.topic,
          targetVocabulary: picked ? picked.targetVocabulary : vm.targetVocabulary,
        }),
      );
    },
    pause: () => { if (vm.sessionId) ignoreError(sessionClient.pauseSpeakingSession(vm.sessionId)); },
    resume: () => { if (vm.sessionId) ignoreError(sessionClient.resumeSpeakingSession(vm.sessionId)); },
    stop: () => { if (vm.sessionId) ignoreError(sessionClient.stopSpeakingSession(vm.sessionId)); },
    bargeIn: () => { if (sessionClient.bargeIn) ignoreError(sessionClient.bargeIn()); },
    returnToHub: () => setVm((s) => ({ ...s, state: 'Idle', sessionReview: null, errorMessage: null })),
    replaySpeech: (text, rateOverride) => { sessionClient.replayTutorSpeech?.(text, rateOverride); },
    updateCorrectionMode: (m) => {
      ignoreError(sessionClient.updateCorrectionMode(m).then((mode) => setVm((s) => ({ ...s, correctionMode: mode }))));
    },
    updateCefrLevel: (cefrLevel) => {
      setVm((s) => ({ ...s, cefrLevel }));
      if (sessionClient.patchPreferences) ignoreError(sessionClient.patchPreferences({ cefrLevel }));
    },
    updateScenarioId: (scenarioId) => { applyScenarioSelection(scenarioId); },
    updateTopic: (topic) => setVm((s) => ({ ...s, topic })),
    updateSpeakingSpeed: (speakingSpeed) => {
      setVm((s) => ({ ...s, speakingSpeed }));
      if (sessionClient.patchPreferences) ignoreError(sessionClient.patchPreferences({ speakingSpeed }));
    },
    updateEnglishVoice: (englishVoiceId) => {
      setVm((s) => ({ ...s, englishVoiceId }));
      if (sessionClient.patchPreferences) ignoreError(sessionClient.patchPreferences({ englishVoiceId }));
    },
    togglePreConversationScript: () => {
      const next = !vm.preConversationScriptEnabled;
      setVm((s) => ({ ...s, preConversationScriptEnabled: !s.preConversationScriptEnabled }));
      if (sessionClient.patchPreferences) ignoreError(sessionClient.patchPreferences({ preConversationScriptEnabled: next }));
    },
    togglePushToTalk: (pushToTalk) => {
      setVm((s) => ({ ...s, pushToTalk }));
      if (sessionClient.patchPreferences) ignoreError(sessionClient.patchPreferences({ pushToTalkEnabled: pushToTalk }));
    },
    holdPushToTalk: (pttHeld) => {
      setVm((s) => ({ ...s, pttHeld }));
      if (sessionClient.setPushToTalkHeld) ignoreError(sessionClient.setPushToTalkHeld(pttHeld));
    },
    submitText: (text) => ignoreError(sessionClient.submitUtteranceText(text)),
    simulateSample: () => ignoreError(sessionClient.simulateSampleTurn()),
    deleteFact: (id) => {
      setLearnerFacts((list) => list.filter((f) => f.id !== id));
      if (sessionClient.deleteLearnerFact) ignoreError(sessionClient.deleteLearnerFact(id));
    },
    clearFacts: () => {
      setLearnerFacts([]);
      if (sessionClient.clearLearnerFacts) ignoreError(sessionClient.clearLearnerFacts());
    },
  };
}
