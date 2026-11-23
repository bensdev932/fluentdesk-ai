import React from 'react';
import type { TutorReply } from '../types/domain';

export interface CoachingFeedbackBoxProps {
  reply: TutorReply;
  onListen?(text: string): void;
}

export function CoachingFeedbackBox({ reply, onListen }: CoachingFeedbackBoxProps): React.JSX.Element | null {
  const properWay = reply.properWay?.trim();
  const why = reply.why?.trim();
  const legacy = reply.correction?.trim();
  if (!properWay && !legacy) return null;

  return (
    <aside aria-label="Coaching feedback" className="gentle-correction">
      <div className="coaching-header-row">
        <span className="coaching-badge">{properWay ? 'Try saying' : 'Coaching Tip'}</span>
        {properWay && onListen && (
          <button type="button" className="btn-audio-chip btn-listen-inline" onClick={() => onListen(properWay)}>
            Listen
          </button>
        )}
      </div>
      {properWay ? (
        <>
          <p className="proper-way-quote">{properWay}</p>
          {why && <p className="feedback-why">{why}</p>}
        </>
      ) : (
        <p>{legacy}</p>
      )}
    </aside>
  );
}
