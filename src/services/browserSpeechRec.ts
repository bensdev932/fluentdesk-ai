import { sanitizeUntrustedText } from '../types/domain';
import { isTauriRuntime } from './runtimeUrl';

export interface SpeechRecognitionCallbacks {
  onInterim(text: string): void;
  onFinal(text: string, confidence: number): void;
  onStatus(note: string): void;
}

type SpeechRecCtor = new () => {
  lang: string;
  continuous: boolean;
  interimResults: boolean;
  onresult: ((event: { resultIndex: number; results: ArrayLike<{ isFinal: boolean; 0: { transcript: string; confidence?: number } }> }) => void) | null;
  onerror: ((event: { error?: string }) => void) | null;
  onend: (() => void) | null;
  start(): void;
  stop(): void;
};

const FATAL_STT_ERRORS = new Set(['service-not-allowed', 'not-allowed', 'audio-capture']);
const MAX_CONSECUTIVE_NETWORK_ERRORS = 2;
const RESTART_DELAY_MS = 250;

function formatUnavailableNote(errorCode: string): string {
  if (isTauriRuntime() && (errorCode === 'network' || errorCode === 'service-not-allowed')) {
    return 'Desktop mic speech-to-text unavailable in WebView2 — type your reply below or click Speak Sample Turn.';
  }
  return `Mic speech-to-text unavailable (${errorCode}) — type your reply below or click Speak Sample Turn.`;
}

export function startBrowserSpeechRecognition(cb: SpeechRecognitionCallbacks): () => void {
  const g = globalThis as unknown as { SpeechRecognition?: SpeechRecCtor; webkitSpeechRecognition?: SpeechRecCtor };
  const Ctor = g.SpeechRecognition || g.webkitSpeechRecognition;
  if (!Ctor) {
    cb.onStatus('Voice recognition unavailable in this browser — use text input or Speak Sample Turn.');
    return () => {};
  }
  let active = true;
  let networkFailures = 0;
  let restartTimer: ReturnType<typeof setTimeout> | null = null;
  const rec = new Ctor();
  rec.lang = 'en-US';
  rec.continuous = true;
  rec.interimResults = true;

  const haltRecognition = (note: string): void => {
    active = false;
    if (restartTimer) { clearTimeout(restartTimer); restartTimer = null; }
    cb.onStatus(note);
    try { rec.stop(); } catch { /* ignore */ }
  };

  rec.onresult = (event) => {
    if (!active) return;
    networkFailures = 0;
    let interim = '';
    for (let i = event.resultIndex; i < event.results.length; i += 1) {
      const res = event.results[i];
      const text = sanitizeUntrustedText(res[0]?.transcript ?? '');
      if (!text) continue;
      if (res.isFinal) cb.onFinal(text, res[0]?.confidence ?? 0.94);
      else interim = `${interim} ${text}`.trim();
    }
    if (interim && active) cb.onInterim(interim);
  };

  rec.onerror = (event) => {
    if (!active) return;
    const code = event.error ?? '';
    if (!code || code === 'no-speech' || code === 'aborted') return;
    if (FATAL_STT_ERRORS.has(code)) {
      haltRecognition(formatUnavailableNote(code));
      return;
    }
    if (code === 'network') {
      networkFailures += 1;
      if (networkFailures >= MAX_CONSECUTIVE_NETWORK_ERRORS) {
        haltRecognition(formatUnavailableNote(code));
        return;
      }
    }
    cb.onStatus(`Mic STT note (${code}) — you can also type or click Speak Sample Turn.`);
  };

  rec.onend = () => {
    if (!active) return;
    if (restartTimer) clearTimeout(restartTimer);
    restartTimer = setTimeout(() => {
      restartTimer = null;
      if (!active) return;
      try { rec.start(); } catch { /* ignore */ }
    }, RESTART_DELAY_MS);
  };

  try {
    rec.start();
    cb.onStatus('Browser voice recognition active — speak into your microphone.');
  } catch {
    cb.onStatus('Ready — speak into your microphone or use the turn input bar.');
  }

  return () => {
    active = false;
    if (restartTimer) { clearTimeout(restartTimer); restartTimer = null; }
    try { rec.stop(); } catch { /* ignore */ }
  };
}
