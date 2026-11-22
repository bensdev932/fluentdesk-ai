import React from 'react';
import type { SpeakingSessionPort } from '../services/tauriService';
import { CorrectionModeSelector } from './CorrectionModeSelector';
import { EmulatorVoiceBar } from './EmulatorVoiceBar';
import { EnglishPracticeHub } from './EnglishPracticeHub';
import { EnglishVoiceBar } from './EnglishVoiceBar';
import { LearnerFactsCard } from './LearnerFactsCard';
import { LearnerProgressStrip } from './LearnerProgressStrip';
import { ProviderSettingsCard } from './ProviderSettingsCard';
import { ScenarioGoalBar } from './ScenarioGoalBar';
import { SessionProgressPill } from './SessionProgressPill';
import { SessionStateBadge } from './SessionStateBadge';
import { SpeakingControls } from './SpeakingControls';
import { TranscriptPanel } from './TranscriptPanel';
import { TutorReplyCard } from './TutorReplyCard';
import { VoiceOrbStage } from './VoiceOrbStage';
import { useBrowserVoiceSession } from './useBrowserVoiceSession';
import { useProviderSettings } from './useProviderSettings';
import { useSpeakingRoom } from './useSpeakingRoom';

export interface SpeakingRoomProps { sessionClient: SpeakingSessionPort; }

export function SpeakingRoom({ sessionClient }: SpeakingRoomProps): React.JSX.Element {
  const room = useSpeakingRoom(sessionClient);
  const voice = useBrowserVoiceSession(room.vm.state, room.vm.pushToTalk, room.vm.pttHeld, sessionClient, room.vm.cefrLevel);
  const providerCfg = useProviderSettings();
  return (
    <div className="speaking-room">
      <LearnerProgressStrip progress={room.learnerProgress} />
      <div className="console-toolbar">
        <SessionStateBadge state={room.vm.state} errorMessage={room.vm.errorMessage ?? undefined} />
        <SessionProgressPill cefrLevel={room.vm.cefrLevel} scenarioId={room.vm.scenarioId} scenarios={room.scenarios} topic={room.vm.topic} phase={room.vm.phase} turnCount={room.vm.turnCount} maxTurns={room.vm.maxTurns} editable={room.perms.canStart} onChangeCefr={room.updateCefrLevel} onChangeScenario={room.updateScenarioId} onChangeTopic={room.updateTopic} />
        <CorrectionModeSelector value={room.vm.correctionMode} disabled={!room.perms.canStart} onChange={room.updateCorrectionMode} />
        <EnglishVoiceBar speakingSpeed={room.vm.speakingSpeed} englishVoiceId={room.vm.englishVoiceId} voices={room.englishVoices} onChangeSpeed={room.updateSpeakingSpeed} onChangeVoice={room.updateEnglishVoice} />
      </div>
      <div className="console-grid">
        <div className="stage-column">
          <ScenarioGoalBar goals={room.vm.goals} completedGoalIds={room.vm.completedGoalIds} targetVocabulary={room.vm.targetVocabulary} usedVocabulary={room.usedVocabulary} />
          <SpeakingControls vm={room.vm} perms={room.perms} onStart={() => room.start()} onPause={room.pause} onResume={room.resume} onStop={room.stop} onTogglePushToTalk={room.togglePushToTalk} onHoldPushToTalk={room.holdPushToTalk} />
          <VoiceOrbStage state={room.vm.state} review={room.vm.sessionReview} onBargeIn={room.bargeIn} onRestart={() => room.start()} onReturnToHub={room.returnToHub} />
          {room.vm.state === 'Idle' && <EnglishPracticeHub scenarios={room.scenarios} selectedScenarioId={room.vm.scenarioId} onSelectScenario={room.updateScenarioId} onStartScenario={(id) => room.start(id)} />}
          {sessionClient.hasVoiceCaptureUi && <EmulatorVoiceBar canSpeak={room.vm.state === 'Listening'} voice={voice} onSubmitText={room.submitText} onSimulateSample={room.simulateSample} />}
          <div className="dialogue-split"><TranscriptPanel items={room.vm.turns} /><TutorReplyCard reply={room.vm.latestReply} playbackWarning={room.vm.playbackWarning} replySource={room.vm.replySource} onReplay={(t) => room.replaySpeech(t)} onReplaySlow={(t) => room.replaySpeech(t, 0.75)} /></div>
        </div>
        <div className="side-column"><ProviderSettingsCard cfg={providerCfg} /><LearnerFactsCard facts={room.learnerFacts} onDeleteFact={room.deleteFact} onClearFacts={room.clearFacts} /></div>
      </div>
    </div>
  );
}
