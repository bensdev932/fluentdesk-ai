# Local Ollama Integration Guide (`T031`, `T037`)

## 1. Architectural Overview

FluentDesk AI uses a local-first AI provider boundary (`AiProvider` in `src-tauri/src/ai/provider.rs`) with a default localhost Ollama HTTP adapter (`OllamaAiProvider` in `src-tauri/src/ai/ollama.rs` and `LocalHttpOllamaTransport` in `src-tauri/src/ai/ollama_http.rs`).

- **Privacy Guarantee:** `OllamaConfig::try_new` enforces that the host is strictly `127.0.0.1`, `localhost`, or `::1`. Non-localhost endpoints are rejected at construction time.
- **Health & Model Verification:** `OllamaAiProvider::health_check` queries `GET /api/tags` on the local Ollama daemon and verifies that the configured model is installed before starting a speaking turn.
- **Voice-First Output Parsing:** `parse_tutor_response_or_fallback` in `src-tauri/src/modules/tutor/feedback.rs` parses structured JSON (`spokenReply`, `correction`, `followUpPrompt`), strips optional Markdown code fences, suppresses `correction` when `CorrectionMode::Off` is active, and safely falls back to sanitized plain text if the local model outputs unstructured text.

## 2. Local Prerequisites

1. Install [Ollama](https://ollama.com/) on your development machine.
2. Start the local daemon:
   ```powershell
   ollama serve
   ```
3. Pull a lightweight, low-latency instruction model suitable for real-time voice conversation:
   ```powershell
   ollama pull llama3.2:3b
   ```

## 3. Running the Opt-In Local Ollama Integration Test (`T037`)

All CI tests run offline using deterministic mocks (`FakeOllamaTransport` and `MockAiProvider`). To run the opt-in live Ollama integration test against your local `ollama serve` instance:

```powershell
$env:FLUENTDESK_OLLAMA_MODEL = "llama3.2:3b"
cargo test --manifest-path src-tauri/Cargo.toml real_local_ollama_generates_tutor_reply_when_available -- --ignored --nocapture
```
