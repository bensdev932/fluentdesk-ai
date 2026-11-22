import React from 'react';
import type { ReplySourceStatus } from '../services/tauriService';
import type { TutorReply } from '../types/domain';
import { formatReplySourceBadge, resolveDistinctFollowUp } from './speakingRoomController';

export interface TutorReplyCardProps {
  reply: TutorReply | null;
  playbackWarning: string | null;
  replySource?: ReplySourceStatus | null;
  onReplay?(text: string): void;
  onReplaySlow?(text: string): void;
}

export function TutorReplyCard({ reply, playbackWarning, replySource, onReplay, onReplaySlow }: TutorReplyCardProps): React.JSX.Element {
  const distinctPrompt = reply ? resolveDistinctFollowUp(reply) : null;
  const badge = formatReplySourceBadge(reply, replySource);
  return (
    <section aria-label="AI Tutor Response" className="tutor-reply-card">
      <div className="card-header-row">
        <h3>Tutor Response</h3>
        <div className="reply-header-actions">
          {reply && onReplay && <button type="button" className="btn-audio-chip" onClick={() => onReplay(reply.spokenReply)}>Replay</button>}
          {reply && onReplaySlow && <button type="button" className="btn-audio-chip" onClick={() => onReplaySlow(reply.spokenReply)}>Slow (0.75x)</button>}
          <span className="card-tag" data-source-tone={badge.tone}>{badge.text}</span>
        </div>
      </div>
      {playbackWarning && <div role="alert" className="playback-warning">{playbackWarning}</div>}
      {reply ? (
        <div className="reply-body">
          <p className="spoken-reply">{reply.spokenReply}</p>
          {reply.correction && (
            <aside aria-label="Gentle grammar feedback" className="gentle-correction">
              <span className="coaching-badge">Coaching Tip</span>
              <p>{reply.correction}</p>
            </aside>
          )}
          {distinctPrompt && <p className="follow-up-prompt">Next prompt: {distinctPrompt}</p>}
        </div>
      ) : (
        <p className="empty-coaching-hint">Start speaking and your AI tutor will reply verbally and display gentle coaching tips here.</p>
      )}
    </section>
  );
}
