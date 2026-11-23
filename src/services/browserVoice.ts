import { ENGLISH_VOICE_PRESETS } from '../types/domain';
export { startBrowserSpeechRecognition, type SpeechRecognitionCallbacks } from './browserSpeechRec';

export interface MicCaptureHandle {
  stop(): void;
  reason?: string;
}

export interface EnglishVoiceOption {
  id: string;
  label: string;
  lang: string;
}

export interface SpeakUtteranceOptions {
  rate?: number;
  englishVoiceId?: string;
}

export async function startBrowserMicCapture(onLevel: (level: number) => void): Promise<MicCaptureHandle> {
  if (typeof navigator === 'undefined' || !navigator.mediaDevices?.getUserMedia) {
    return { stop: () => {}, reason: 'Microphone API unavailable in this environment.' };
  }
  try {
    const stream = await navigator.mediaDevices.getUserMedia({
      audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true },
    });
    const g = globalThis as unknown as { AudioContext?: typeof AudioContext; webkitAudioContext?: typeof AudioContext };
    const AudioCtx = g.AudioContext || g.webkitAudioContext;
    if (!AudioCtx) {
      return {
        stop: () => { stream.getTracks().forEach((t) => t.stop()); },
      };
    }
    const ctx = new AudioCtx();
    const source = ctx.createMediaStreamSource(stream);
    const analyser = ctx.createAnalyser();
    analyser.fftSize = 256;
    source.connect(analyser);
    const samples = new Uint8Array(analyser.frequencyBinCount || 128);
    let active = true;
    let rafId = 0;
    const gRaf = globalThis as unknown as {
      requestAnimationFrame?: (cb: (t: number) => void) => number;
      cancelAnimationFrame?: (id: number) => void;
    };
    const schedule = gRaf.requestAnimationFrame ?? ((cb: (t: number) => void) => Number(setTimeout(() => cb(16), 80)));
    const cancelSchedule = gRaf.cancelAnimationFrame ?? ((id: number) => clearTimeout(id));
    const tick = (): void => {
      if (!active) return;
      analyser.getByteTimeDomainData(samples);
      let sum = 0;
      for (let i = 0; i < samples.length; i += 1) {
        const norm = (samples[i] - 128) / 128;
        sum += norm * norm;
      }
      const rms = samples.length > 0 ? Math.sqrt(sum / samples.length) : 0;
      onLevel(Math.min(1, rms * 3.5));
      rafId = schedule(tick);
    };
    rafId = schedule(tick);
    return {
      stop() {
        active = false;
        cancelSchedule(rafId);
        onLevel(0);
        stream.getTracks().forEach((t) => t.stop());
        void ctx.close().catch(() => {});
      },
    };
  } catch {
    return { stop: () => {}, reason: 'Microphone permission unavailable — use text input or Speak Sample Turn.' };
  }
}

export function normalizeForSpeech(text: string): string {
  return text
    .replace(/```[\s\S]*?```/g, ' ')
    .replace(/\[[^\]]*\]/g, ' ')
    .replace(/`([^`]*)`/g, '$1')
    .replace(/\*\*([^*]+)\*\*/g, '$1')
    .replace(/\*([^*]+)\*/g, '$1')
    .replace(/[_#~>|]/g, ' ')
    .replace(/\p{Extended_Pictographic}/gu, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

export function scoreEnglishVoice(v: Pick<SpeechSynthesisVoice, 'name' | 'lang' | 'localService'>): number {
  const name = (v.name ?? '').toLowerCase();
  const lang = (v.lang ?? '').toLowerCase();
  let score = 0;
  if (name.includes('natural') || name.includes('neural')) score += 100;
  if (name.includes('online')) score += 20;
  if (name.includes('google us english') || name.includes('google uk english')) score += 75;
  if (/\b(aria|jenny|guy|ava|andrew|emma|samantha|libby|sonia|ryan)\b/.test(name)) score += 50;
  if (lang === 'en-us' || lang === 'en-gb') score += 20;
  if (name.includes('espeak') || name.includes('compact')) score -= 50;
  return score;
}

function getRankedSystemEnglishVoices(synth?: SpeechSynthesis): SpeechSynthesisVoice[] {
  const raw = synth && typeof synth.getVoices === 'function' ? synth.getVoices() : [];
  return raw
    .filter((v) => (v.lang ?? '').trim().toLowerCase().startsWith('en'))
    .sort((a, b) => scoreEnglishVoice(b) - scoreEnglishVoice(a) || a.name.localeCompare(b.name));
}

export function listPureEnglishVoices(): EnglishVoiceOption[] {
  const presets: EnglishVoiceOption[] = ENGLISH_VOICE_PRESETS.map((p) => ({ id: p.id, label: p.label, lang: p.id }));
  const synth = (globalThis as unknown as { speechSynthesis?: SpeechSynthesis }).speechSynthesis;
  const seen = new Set<string>(presets.map((p) => p.id.toLowerCase()));
  for (const v of getRankedSystemEnglishVoices(synth)) {
    const lang = (v.lang ?? '').trim();
    const id = (v.name || v.voiceURI || lang).trim();
    if (!id || seen.has(id.toLowerCase())) continue;
    seen.add(id.toLowerCase());
    presets.push({ id, label: `${v.name} (${lang})`, lang });
  }
  return presets;
}

export function subscribeEnglishVoices(onChange: (voices: EnglishVoiceOption[]) => void): () => void {
  onChange(listPureEnglishVoices());
  const synth = (globalThis as unknown as { speechSynthesis?: SpeechSynthesis }).speechSynthesis;
  if (!synth) return () => {};
  const handler = (): void => { onChange(listPureEnglishVoices()); };
  if (typeof synth.addEventListener === 'function') {
    synth.addEventListener('voiceschanged', handler);
    return () => synth.removeEventListener('voiceschanged', handler);
  }
  const prev = synth.onvoiceschanged;
  synth.onvoiceschanged = handler;
  return () => { if (synth.onvoiceschanged === handler) synth.onvoiceschanged = prev ?? null; };
}

export function speakTutorUtterance(
  text: string,
  opts?: SpeakUtteranceOptions,
): { done: Promise<void>; stop(): void } {
  const clean = normalizeForSpeech(text);
  const g = globalThis as unknown as {
    speechSynthesis?: SpeechSynthesis;
    SpeechSynthesisUtterance?: typeof SpeechSynthesisUtterance;
  };
  const synth = g.speechSynthesis;
  const Utter = g.SpeechSynthesisUtterance;
  if (!clean || !synth || !Utter) return { done: Promise.resolve(), stop: () => {} };
  let settled = false;
  let finish: () => void = () => {};
  const done = new Promise<void>((resolve) => {
    finish = () => { if (!settled) { settled = true; clearTimeout(fallbackTimer); resolve(); } };
  });
  const utter = new Utter(clean);
  const reqVoice = (opts?.englishVoiceId ?? 'en-US').trim();
  const safeVoice = reqVoice.toLowerCase().startsWith('en') ? reqVoice : 'en-US';
  utter.lang = safeVoice.length <= 8 ? safeVoice : 'en-US';
  utter.rate = Math.min(2.0, Math.max(0.5, opts?.rate ?? 1.0));
  const voices = getRankedSystemEnglishVoices(synth);
  const matched = voices.find((v) => v.name === reqVoice || v.voiceURI === reqVoice)
    ?? voices.find((v) => v.lang?.toLowerCase() === safeVoice.toLowerCase())
    ?? voices[0];
  if (matched) { utter.voice = matched; utter.lang = matched.lang || 'en-US'; }
  utter.onend = () => finish();
  utter.onerror = () => finish();
  const fallbackTimer = setTimeout(() => finish(), Math.min(20_000, Math.max(3_500, clean.length * 90)));
  try { synth.speak(utter); } catch { finish(); }
  return { done, stop() { try { synth.cancel(); } catch { /* ignore */ } finish(); } };
}

export function cancelBrowserSpeech(): void {
  const synth = (globalThis as unknown as { speechSynthesis?: SpeechSynthesis }).speechSynthesis;
  try { synth?.cancel?.(); } catch { /* ignore */ }
}
