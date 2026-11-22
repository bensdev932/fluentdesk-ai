import { isMicrophoneCapturingState, type SessionState } from '../types/domain';

export type SessionTone = 'neutral' | 'active' | 'working' | 'paused' | 'danger';

export interface SessionStatePresentation {
  state: SessionState;
  label: string;
  description: string;
  microphoneActive: boolean;
  tone: SessionTone;
}

const STATE_DETAILS: Record<
  SessionState,
  { label: string; description: string; tone: SessionTone }
> = {
  Idle: {
    label: 'Ready to Speak',
    description: 'Start a session to practice English conversation out loud.',
    tone: 'neutral',
  },
  Initializing: {
    label: 'Preparing Session',
    description: 'Checking microphone and local speech models...',
    tone: 'working',
  },
  Listening: {
    label: 'Listening — Speak Now',
    description: 'Microphone is live. Speak naturally at your own pace.',
    tone: 'active',
  },
  Processing: {
    label: 'Recognizing Speech',
    description: 'Converting your spoken turn into text...',
    tone: 'working',
  },
  Thinking: {
    label: 'Tutor Thinking',
    description: 'Preparing a natural spoken response...',
    tone: 'working',
  },
  Speaking: {
    label: 'Tutor Speaking',
    description: 'Listen to your AI tutor reply.',
    tone: 'active',
  },
  Paused: {
    label: 'Session Paused',
    description: 'Microphone is muted. Resume whenever you are ready.',
    tone: 'paused',
  },
  Error: {
    label: 'Attention Needed',
    description: 'A device or local model issue paused the conversation.',
    tone: 'danger',
  },
  Ended: {
    label: 'Session Ended',
    description: 'Great practice! Review your transcript or start a new session.',
    tone: 'neutral',
  },
};

export function getSessionStatePresentation(
  state: SessionState,
): SessionStatePresentation {
  const details = STATE_DETAILS[state];
  return {
    state,
    label: details.label,
    description: details.description,
    microphoneActive: isMicrophoneCapturingState(state),
    tone: details.tone,
  };
}
