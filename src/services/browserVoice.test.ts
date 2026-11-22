import { afterEach, describe, expect, it, vi } from 'vitest';
import { cancelBrowserSpeech, normalizeForSpeech, speakTutorUtterance } from './browserVoice';
import { createQueuedSpeechHandle } from './speakingSessionStream';

interface StubUtterance {
  text: string;
  lang: string;
  onend: (() => void) | null;
  onerror: (() => void) | null;
}

function stubSpeechSynthesis() {
  const spoken: StubUtterance[] = [];
  const cancel = vi.fn();
  const speak = vi.fn((u: StubUtterance) => {
    spoken.push(u);
  });
  class MockUtterance implements StubUtterance {
    text: string;
    lang = '';
    onend: (() => void) | null = null;
    onerror: (() => void) | null = null;
    constructor(text: string) {
      this.text = text;
    }
  }
  vi.stubGlobal('speechSynthesis', { speak, cancel });
  vi.stubGlobal('SpeechSynthesisUtterance', MockUtterance);
  return { spoken, speak, cancel };
}

afterEach(() => {
  vi.unstubAllGlobals();
  delete (globalThis as { speechSynthesis?: unknown }).speechSynthesis;
  delete (globalThis as { SpeechSynthesisUtterance?: unknown }).SpeechSynthesisUtterance;
});

describe('browserVoice TTS queueing without mid-queue synth.cancel()', () => {
  it('queues consecutive sentences without calling synth.cancel() between them', async () => {
    const { spoken, speak, cancel } = stubSpeechSynthesis();
    const handle = createQueuedSpeechHandle(speakTutorUtterance);

    handle.enqueue('Sentence one.');
    handle.enqueue('Sentence two.');
    handle.enqueue('Sentence three.');

    await vi.waitFor(() => expect(speak).toHaveBeenCalledTimes(1));
    expect(cancel).not.toHaveBeenCalled();

    spoken[0].onend?.();
    await vi.waitFor(() => expect(speak).toHaveBeenCalledTimes(2));
    expect(cancel).not.toHaveBeenCalled();

    spoken[1].onend?.();
    await vi.waitFor(() => expect(speak).toHaveBeenCalledTimes(3));
    expect(cancel).not.toHaveBeenCalled();

    spoken[2].onend?.();
    await handle.flush();

    expect(spoken.map((u) => u.text)).toEqual([
      'Sentence one.',
      'Sentence two.',
      'Sentence three.',
    ]);
    expect(cancel).not.toHaveBeenCalled();
  });

  it('calls synth.cancel() only on stop() or cancelBrowserSpeech()', async () => {
    const { spoken, speak, cancel } = stubSpeechSynthesis();
    const handle = createQueuedSpeechHandle(speakTutorUtterance);

    handle.enqueue('First sentence.');
    handle.enqueue('Second sentence.');
    await vi.waitFor(() => expect(speak).toHaveBeenCalledTimes(1));

    expect(cancel).not.toHaveBeenCalled();

    handle.stop();
    await handle.flush();

    expect(cancel).toHaveBeenCalledTimes(1);
    expect(speak).toHaveBeenCalledTimes(1);
    expect(spoken).toHaveLength(1);

    cancelBrowserSpeech();
    expect(cancel).toHaveBeenCalledTimes(2);
  });

  it('handles cancelBrowserSpeech() safely when speechSynthesis is absent or throws', () => {
    expect(() => cancelBrowserSpeech()).not.toThrow();
    vi.stubGlobal('speechSynthesis', {
      cancel: () => {
        throw new Error('reset failed');
      },
    });
    expect(() => cancelBrowserSpeech()).not.toThrow();
  });

  it('strips markdown, stage directions, and emojis before browser speech', () => {
    const { spoken } = stubSpeechSynthesis();
    expect(normalizeForSpeech('**Nice!** [smiles] *nods* Try `this` 🎉')).toBe('Nice! nods Try this');
    speakTutorUtterance('### **Well done!** [claps] 👏');
    expect(spoken[0]?.text).toBe('Well done!');
  });
});
