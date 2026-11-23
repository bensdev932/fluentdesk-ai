import React from 'react';
import { resolveSuggestedReplies } from '../services/preConversationScripts';
import type { CefrLevel, SessionState, TutorReply } from '../types/domain';
import { PreConversationScriptCard } from './PreConversationScriptCard';
import { SuggestedRepliesBar } from './SuggestedRepliesBar';

export interface GuidedScriptPanelProps {
  scriptEnabled: boolean;
  state: SessionState;
  scenarioId: string;
  cefrLevel: CefrLevel;
  latestReply: TutorReply | null;
  canSpeak: boolean;
  onListen: (text: string) => void;
  onUseLine: (text: string) => void;
}

export function GuidedScriptPanel(p: GuidedScriptPanelProps): React.JSX.Element | null {
  if (!p.scriptEnabled) return null;
  if (p.state === 'Idle') {
    return <PreConversationScriptCard scenarioId={p.scenarioId} cefrLevel={p.cefrLevel} onListen={p.onListen} />;
  }
  if (p.state === 'Ended') return null;
  const replies = resolveSuggestedReplies(p.latestReply, p.scenarioId, p.cefrLevel);
  return <SuggestedRepliesBar replies={replies} canSpeak={p.canSpeak} onListen={p.onListen} onUseLine={p.onUseLine} />;
}
