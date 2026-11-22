import type { ReplySourceStatus, VoiceLoopEvent } from '../services/tauriService';
import type { SpeakingRoomViewModel } from './speakingRoomController';
import type { TranscriptItem } from './TranscriptPanel';

export function appendUserTranscript(
  vm: SpeakingRoomViewModel,
  ev: Extract<VoiceLoopEvent, { type: 'user-transcript-ready' }>,
): SpeakingRoomViewModel {
  const item: TranscriptItem = {
    id: ev.turn.id,
    role: 'User',
    text: ev.turn.text,
    timestampUnixMs: ev.turn.timestampUnixMs,
    confidence: ev.confidence,
  };
  const deduped = vm.turns.filter((t) => t.id !== ev.turn.id);
  if (vm.streamingTurnId) {
    const streamIdx = deduped.findIndex((t) => t.id === vm.streamingTurnId);
    if (streamIdx >= 0) {
      return { ...vm, turns: [...deduped.slice(0, streamIdx), item, ...deduped.slice(streamIdx)] };
    }
  }
  return { ...vm, turns: [...deduped, item] };
}

export function applyStreamingTutorReply(
  vm: SpeakingRoomViewModel,
  ev: Extract<VoiceLoopEvent, { type: 'tutor-reply-streaming' }>,
): SpeakingRoomViewModel {
  const text = ev.partialText.trim();
  if (!text) return vm;
  const streamId = ev.turnId ?? vm.streamingTurnId ?? `stream-${ev.sessionId}`;
  const existingIdx = vm.turns.findIndex((t) => t.id === streamId);
  const nextItem: TranscriptItem = existingIdx >= 0
    ? { ...vm.turns[existingIdx], text }
    : { id: streamId, role: 'Tutor', text, timestampUnixMs: Date.now() };
  const turns = existingIdx >= 0
    ? vm.turns.map((t, i) => (i === existingIdx ? nextItem : t))
    : [...vm.turns, nextItem];
  return {
    ...vm,
    turns,
    streamingTurnId: streamId,
    latestReply: { spokenReply: text },
    playbackWarning: null,
  };
}

function deriveReplySource(ev: Extract<VoiceLoopEvent, { type: 'tutor-reply-ready' }>): ReplySourceStatus | null {
  if (ev.replySource) return ev.replySource;
  if (ev.playbackWarning?.kind === 'AiProviderUnavailable') {
    return { kind: 'offline-fallback', label: 'Offline Fallback' };
  }
  return null;
}

export function appendTutorReply(
  vm: SpeakingRoomViewModel,
  ev: Extract<VoiceLoopEvent, { type: 'tutor-reply-ready' }>,
): SpeakingRoomViewModel {
  const item: TranscriptItem = {
    id: ev.turn.id,
    role: 'Tutor',
    text: ev.turn.text,
    timestampUnixMs: ev.turn.timestampUnixMs,
  };
  const replaceId = vm.streamingTurnId;
  let replaced = false;
  const nextTurns: TranscriptItem[] = [];
  for (const t of vm.turns) {
    if (t.id === replaceId || t.id === ev.turn.id) {
      if (!replaced) {
        nextTurns.push(item);
        replaced = true;
      }
      continue;
    }
    nextTurns.push(t);
  }
  if (!replaced) nextTurns.push(item);
  const warning = ev.playbackWarning ? ev.playbackWarning.userMessage : null;
  return {
    ...vm,
    turns: nextTurns,
    streamingTurnId: null,
    latestReply: ev.reply,
    replySource: deriveReplySource(ev),
    playbackWarning: warning,
  };
}
