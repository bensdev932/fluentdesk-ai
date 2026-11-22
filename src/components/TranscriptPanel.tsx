import React, { useEffect, useRef } from 'react';
import type { ConversationRole } from '../types/domain';

export interface TranscriptItem {
  id: string;
  role: ConversationRole;
  text: string;
  timestampUnixMs: number;
  confidence?: number;
}

export interface TranscriptPanelProps {
  items: TranscriptItem[];
}

export function TranscriptPanel({ items }: TranscriptPanelProps): React.JSX.Element {
  const listRef = useRef<HTMLUListElement>(null);
  useEffect(() => {
    if (listRef.current) listRef.current.scrollTop = listRef.current.scrollHeight;
  }, [items.length]);
  return (
    <section aria-label="Conversation Transcript" className="transcript-panel">
      <div className="card-header-row">
        <h2>Transcript</h2>
        <span className="card-tag">{items.length} {items.length === 1 ? 'turn' : 'turns'}</span>
      </div>
      {items.length === 0 ? (
        <p className="empty-message">No spoken turns yet. Speak into the microphone to begin.</p>
      ) : (
        <ul ref={listRef} className="turn-list">
          {items.map((item) => (
            <li key={item.id} data-role={item.role} className={`turn-bubble ${item.role.toLowerCase()}`}>
              <div className="turn-header">
                <span className="badge">{item.role === 'User' ? 'You' : 'AI Tutor'}</span>
                {item.role === 'User' && item.confidence !== undefined && (
                  <span className="confidence">({Math.round(item.confidence * 100)}%)</span>
                )}
              </div>
              <p className="turn-text">{item.text}</p>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
