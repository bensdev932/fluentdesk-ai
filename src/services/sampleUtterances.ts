export interface SampleUtterance {
  userText: string;
  confidence: number;
}

export const SAMPLE_UTTERANCES: readonly SampleUtterance[] = [
  {
    userText: 'Yesterday I go to a quiet coffee shop and talk with my old friend for two hours.',
    confidence: 0.96,
  },
  {
    userText: 'I am working on a remote engineering team since three years now.',
    confidence: 0.94,
  },
];
