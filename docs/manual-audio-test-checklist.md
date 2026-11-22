# Manual Microphone Permission & Device Testing Runbook (Windows 11)

**Task ID:** `T021`  
**Target OS:** Windows 11 (`Win32NT 10.0.26300.0`, WASAPI)

---

## 1. Automated Opt-In vs. Human-in-the-Loop Verification

In accordance with `constitution.md` Principle 6 ("Audio tests MUST include recorded fixtures and must not require a physical microphone in normal CI"), normal CI runs deterministic audio fixtures (`T015`, `T020`).

Use this manual checklist on the target Windows 11 workstation to verify physical WASAPI microphone permissions, device hot-plugging, and push-to-talk / VAD behavior.

---

## 2. Manual Test Cases

### Case 1 — Windows 11 Microphone Privacy Permission Denial
1. Open **Windows Settings $\to$ Privacy & security $\to$ Microphone**.
2. Toggle **Let desktop apps access your microphone** to **Off**.
3. In FluentDesk AI, click **Start Speaking Session**.
4. **Expected Result:**
   - Session transitions to `Error` (never shows `Listening` / `Mic Live`).
   - User-friendly message is displayed: *"Microphone access was denied."*
   - No crash or unhandled panic occurs.

### Case 2 — Microphone Enumeration & Selection (`T016`)
1. Re-enable **Let desktop apps access your microphone** in Windows Settings.
2. Connect a secondary headset/USB microphone alongside the built-in microphone.
3. Query `list_audio_input_devices` and select the secondary device via `set_audio_input_device`.
4. **Expected Result:**
   - Both input devices appear with human-readable names and `isDefault` flags.
   - Selecting a valid device ID succeeds; selecting a disconnected ID returns `AudioDeviceUnavailable`.

### Case 3 — Live Utterance Capture & Unplugging Mid-Session (`T017`, `T020`)
1. Start a session in `Listening` mode and speak a short sentence with a 0.4s natural thinking pause.
2. Unplug the active USB microphone while in `Listening`.
3. **Expected Result:**
   - The 0.4s mid-sentence pause does not prematurely cut off the turn (`silence_hangover_ms = 800`).
   - Unplugging the microphone transitions the session cleanly to `Error` (`AudioDeviceUnavailable`) and turns the `Mic Live` indicator to `Mic Off`.
