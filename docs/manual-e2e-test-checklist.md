# Manual End-to-End & Offline Speaking Loop Verification (`T049`, `T050`)

## 1. Prerequisites

1. **Local Whisper Model (`Phase 3`):**
   - `whisper-cli.exe` and `ggml-base.en.bin` installed locally (see [whisper-integration.md](file:///c:/Users/ben/Documents/English%20AI%20tutor/docs/whisper-integration.md)).
2. **Local Ollama Model (`Phase 4`):**
   - `ollama serve` running on `127.0.0.1:11434` with `llama3.2:3b` pulled (see [ollama-integration.md](file:///c:/Users/ben/Documents/English%20AI%20tutor/docs/ollama-integration.md)).
3. **Local Kokoro / Piper TTS Sidecar (`Phase 5`):**
   - Local TTS sidecar binary and voice model installed (see [tts-integration.md](file:///c:/Users/ben/Documents/English%20AI%20tutor/docs/tts-integration.md)).

---

## 2. Manual End-to-End Voice Loop Test (`T049`)

1. Launch FluentDesk AI (`npm run tauri dev`).
2. Confirm the Speaking Room header badge displays **Ready to Speak** (`Idle`) and **Mic Off**.
3. Click **Start Speaking**:
   - Verify the badge transitions `Initializing` $\to$ **Listening** (`Mic Live`).
4. Speak a complete English sentence into the microphone (e.g., *"Yesterday I go to the park with my friends"*):
   - Pause naturally for $\sim 800\text{ ms}$ (or release **Hold to Speak** in Push-to-Talk mode).
   - Verify the badge transitions to **Processing Speech** (`Mic Off`) while Whisper transcribes.
   - Verify your recognized utterance appears in the **Transcript** panel labeled `You` with a confidence percentage.
5. Verify AI Tutor Thinking & Feedback:
   - Verify the badge transitions to **Tutor Thinking** (`Mic Off`).
   - Verify the **AI Tutor Feedback** card displays the tutor's spoken reply, a gentle coaching tip (*"Say 'I went' instead of 'I go'"*), and a conversational follow-up question.
6. Verify Audible Response & Return to Listening:
   - Verify the badge transitions to **Tutor Speaking** while the local TTS audio plays through your speakers/headphones.
   - Verify that as soon as playback finishes, the session automatically returns to **Listening** (`Mic Live`) for the next turn.
7. Click **Stop Session** mid-turn:
   - Verify microphone capture, inference, and audio playback stop immediately and the badge shows **Session Ended** (`Mic Off`).

---

## 3. Offline Network-Disabled Verification (`T050`)

1. Disconnect Wi-Fi / Ethernet (or enable Airplane Mode on Windows 11).
2. Run the automated offline end-to-end test suite:
   ```powershell
   cargo test --offline --manifest-path src-tauri/Cargo.toml e2e_tests
   ```
3. Repeat Section 2 in the desktop app with network disabled and confirm that multi-turn speech recognition, local Ollama responses, local TTS playback, and SQLite persistence succeed with zero external network requests.
