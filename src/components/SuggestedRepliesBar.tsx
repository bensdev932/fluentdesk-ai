import React, { useState } from 'react';

export interface SuggestedRepliesBarProps {
  replies: string[];
  canSpeak: boolean;
  onListen: (text: string) => void;
  onUseLine: (text: string) => void;
}

export function SuggestedRepliesBar(p: SuggestedRepliesBarProps): React.JSX.Element | null {
  const [collapsed, setCollapsed] = useState(false);
  if (p.replies.length === 0) return null;
  return (
    <section className="suggested-replies-bar" aria-label="Suggested Replies">
      <div className="suggested-head">
        <strong>Suggested Replies</strong>
        <button type="button" className="btn-secondary btn-xs" onClick={() => setCollapsed((v) => !v)}>
          {collapsed ? 'Show Hints' : 'Hide Hints'}
        </button>
      </div>
      {!collapsed && (
        <div className="reply-chips">
          {p.replies.map((line) => (
            <div key={line} className="reply-chip">
              <span>{line}</span>
              <button type="button" className="btn-secondary btn-xs" onClick={() => p.onListen(line)}>Listen</button>
              <button type="button" className="btn-primary btn-xs" disabled={!p.canSpeak} onClick={() => p.onUseLine(line)}>Use line</button>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
