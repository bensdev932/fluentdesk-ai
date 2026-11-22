import { useEffect, useRef, useState } from 'react';
import {
  startBrowserMicCapture,
  startBrowserSpeechRecognition,
  type MicCaptureHandle,
} from '../services/browserVoice';
import type { SpeakingSessionPort } from '../services/tauriService';
import type { CefrLevel, SessionState } from '../types/domain';

export interface BrowserVoiceStatus {
  micLevel: number;
  interimText: string;
  finalText: string;
  voiceNote: string;
}

export function cefrHesitationMs(cefrLevel?: CefrLevel): number {
  if (!cefrLevel) return 0;
  if (cefrLevel === 'A1' || cefrLevel === 'A2') return 900;
  if (cefrLevel === 'B1' || cefrLevel === 'B2') return 600;
  return 400;
}

function isWarmVoiceState(state: SessionState): boolean {
  return state === 'Listening' || state === 'Processing' || state === 'Thinking' || state === 'Speaking';
}

export function useBrowserVoiceSession(
  state: SessionState,
  pushToTalk: boolean,
  pttHeld: boolean,
  sessionClient: SpeakingSessionPort,
  cefrLevel?: CefrLevel,
): BrowserVoiceStatus {
  const [micLevel, setMicLevel] = useState(0);
  const [interimText, setInterimText] = useState('');
  const [finalText, setFinalText] = useState('');
  const [voiceNote, setVoiceNote] = useState('');

  const interimRef = useRef('');
  const deviceNoteRef = useRef('');
  const graceRef = useRef(0);
  graceRef.current = cefrHesitationMs(cefrLevel);
  const pendingRef = useRef<{ text: string; conf: number; timer: ReturnType<typeof setTimeout> | null } | null>(null);
  const submitRef = useRef(sessionClient.submitUtteranceText);
  submitRef.current = sessionClient.submitUtteranceText;

  const isWarm = sessionClient.hasVoiceCaptureUi && isWarmVoiceState(state);
  const canAccept = isWarm && state === 'Listening' && (!pushToTalk || pttHeld);
  const canAcceptRef = useRef(canAccept);
  canAcceptRef.current = canAccept;

  const flushPending = (): void => {
    const p = pendingRef.current;
    if (!p) return;
    pendingRef.current = null;
    if (p.timer) clearTimeout(p.timer);
    setFinalText(p.text);
    void submitRef.current(p.text, p.conf).catch(() => undefined);
  };

  useEffect(() => {
    if (!isWarm) {
      deviceNoteRef.current = '';
      return;
    }
    let cancelled = false;
    let micHandle: MicCaptureHandle | null = null;
    void startBrowserMicCapture((level) => {
      if (!cancelled && canAcceptRef.current) setMicLevel(level);
    }).then((h) => {
      if (cancelled) { h.stop(); return; }
      micHandle = h;
      if (h.reason) { deviceNoteRef.current = h.reason; setVoiceNote(h.reason); }
    });
    const stopRec = startBrowserSpeechRecognition({
      onInterim: (txt) => {
        if (cancelled || !canAcceptRef.current) return;
        interimRef.current = txt;
        const prefix = pendingRef.current?.text;
        setInterimText(prefix && txt ? `${prefix} ${txt}` : txt);
        if (txt && !prefix) setFinalText('');
      },
      onFinal: (txt, conf) => {
        if (cancelled || !canAcceptRef.current) return;
        interimRef.current = '';
        setInterimText('');
        const ms = graceRef.current;
        if (ms <= 0) {
          setFinalText(txt);
          void submitRef.current(txt, conf).catch(() => undefined);
          return;
        }
        const prev = pendingRef.current;
        if (prev?.timer) clearTimeout(prev.timer);
        const merged = prev?.text ? `${prev.text} ${txt}`.trim() : txt;
        setFinalText(merged);
        pendingRef.current = { text: merged, conf, timer: setTimeout(flushPending, ms) };
      },
      onStatus: (msg) => {
        if (cancelled) return;
        deviceNoteRef.current = msg;
        setVoiceNote(msg);
      },
    });
    return () => {
      cancelled = true;
      if (pendingRef.current?.timer) clearTimeout(pendingRef.current.timer);
      pendingRef.current = null;
      micHandle?.stop();
      stopRec();
    };
  }, [isWarm]);

  useEffect(() => {
    canAcceptRef.current = canAccept;
    if (canAccept) {
      setVoiceNote(deviceNoteRef.current || 'Listening on microphone...');
      return;
    }
    setMicLevel(0);
    if (pendingRef.current && state === 'Listening' && pushToTalk && !pttHeld) {
      const combined = interimRef.current ? `${pendingRef.current.text} ${interimRef.current}`.trim() : pendingRef.current.text;
      if (pendingRef.current.timer) clearTimeout(pendingRef.current.timer);
      pendingRef.current = null;
      interimRef.current = '';
      setInterimText('');
      setFinalText(combined);
      void submitRef.current(combined, 0.88).catch(() => undefined);
      setVoiceNote(deviceNoteRef.current || 'Hold Push-to-Talk to speak...');
      return;
    }
    flushPending();
    if (pushToTalk && !pttHeld && state === 'Listening') {
      if (interimRef.current) {
        const pending = interimRef.current;
        interimRef.current = '';
        setInterimText('');
        setFinalText(pending);
        void submitRef.current(pending, 0.88).catch(() => undefined);
      }
      setVoiceNote(deviceNoteRef.current || 'Hold Push-to-Talk to speak...');
      return;
    }
    interimRef.current = '';
    setInterimText('');
    if (state === 'Idle' || state === 'Ended') { setFinalText(''); setVoiceNote(''); return; }
    setVoiceNote(deviceNoteRef.current);
  }, [canAccept, pushToTalk, pttHeld, state]);

  return { micLevel, interimText, finalText, voiceNote };
}
