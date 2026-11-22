import React from 'react';
import type { SpeakingControlPermissions, SpeakingRoomViewModel } from './speakingRoomController';

export interface SpeakingControlsProps {
  vm: SpeakingRoomViewModel;
  perms: SpeakingControlPermissions;
  onStart(): void;
  onPause(): void;
  onResume(): void;
  onStop(): void;
  onTogglePushToTalk(enabled: boolean): void;
  onHoldPushToTalk(held: boolean): void;
}

export function SpeakingControls(p: SpeakingControlsProps): React.JSX.Element {
  const { vm, perms } = p;
  const stopHold = (): void => { if (vm.pttHeld) p.onHoldPushToTalk(false); };
  return (
    <section aria-label="Session Controls" className="speaking-controls">
      <div className="primary-buttons">
        <button type="button" className="btn-primary" onClick={p.onStart} disabled={!perms.canStart}>Start Speaking</button>
        <button type="button" className="btn-secondary" onClick={p.onPause} disabled={!perms.canPause}>Pause</button>
        <button type="button" className="btn-secondary" onClick={p.onResume} disabled={!perms.canResume}>Resume</button>
        <button type="button" className="btn-danger" onClick={p.onStop} disabled={!perms.canStop} data-stop-btn="true">Stop Session</button>
      </div>
      <div className="ptt-controls">
        <label className="ptt-toggle">
          <input type="checkbox" checked={vm.pushToTalk} onChange={(e) => p.onTogglePushToTalk(e.target.checked)} />
          <span>Push-to-Talk Mode</span>
        </label>
        {vm.pushToTalk && (
          <button
            type="button"
            className="btn-ptt"
            data-held={vm.pttHeld}
            disabled={!perms.canHoldPtt}
            onPointerDown={() => p.onHoldPushToTalk(true)}
            onPointerUp={stopHold}
            onPointerCancel={stopHold}
            onPointerLeave={stopHold}
          >
            {vm.pttHeld ? 'Recording... Release to Send' : 'Hold to Speak'}
          </button>
        )}
      </div>
    </section>
  );
}
