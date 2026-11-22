# FluentDesk AI Engineering Rules & Code Smells Guard

**Version:** 1.0.0  
**Status:** Binding architectural standards for Rust and TypeScript/React codebases  
**Companion Documents:** [constitution.md](.specify/memory/constitution.md), [AGENTS.md](docs/AGENTS.md), [tasks.md](docs/tasks.md)

---

## 1. Core Principles

Every pull request, task implementation, and refactoring step must adhere to these five core engineering tenets:

1. **Automatic Code Smell Detection & Removal**: Identify and eliminate code smells (Feature Envy, Primitive Obsession, Data Clumps, Long Parameter Lists, Shotgun Surgery) early during the TDD Refactor phase.
2. **SOLID Principles Across Modules**:
   - **Single Responsibility (SRP)**: Each struct, component, module, or function must have exactly one reason to change.
   - **Open/Closed (OCP)**: Extend behavior via traits (`AudioInput`, `SpeechToText`, `AiProvider`) and compositions rather than modifying existing call sites.
   - **Liskov Substitution (LSP)**: All mock and concrete adapters must adhere strictly to the behavioral contracts defined by their traits.
   - **Interface Segregation (ISP)**: Keep interfaces narrow and purpose-driven; avoid kitchen-sink service traits.
   - **Dependency Inversion (DIP)**: Depend on high-level trait abstractions, never direct concrete hardware or vendor implementations.
3. **Value Objects for Domain Primitives**: Eliminate primitive obsession by wrapping IDs, durations, confidence scores, and text chunks in strongly typed, invariant-enforcing value objects.
4. **Law of Demeter & Tell, Don't Ask**:
   - Objects must only interact with their direct dependencies. Do not chain accessors (`a.b().c().do_something()`).
   - Tell objects what to do through intention-revealing command verbs; do not ask for their internal state to make decisions on their behalf.
5. **Tight Size & Complexity Limits**:
   - **Files**: No source file (`.rs`, `.ts`, `.tsx`, `.sql`) may ever exceed **200 lines of code**. Split modules and tests into cohesive submodules before reaching 200 lines.
   - **Functions / Methods**: Keep methods under **10 lines** (excluding exhaustive state machine transition tables and SQL statements, which must delegate each branch immediately to single-responsibility helpers).
   - **Rust Struct Impls / Types**: Keep individual focused struct implementations under **50 lines** where practical; partition larger domains across cohesive traits and modules.
   - **Frontend Components**: Keep React `.tsx` components under **50 lines** by extracting business logic and state orchestration into custom hooks (`useSession`, `useAudioDevices`) and sub-components.

---

## 2. Value Object Specifications (Domain Primitives)

To prevent primitive obsession and invalid state representations, raw strings, floats, and ints must not cross domain boundaries unprotected:

### Invariant Enclosure Rules
- **Private Inner Storage**: Tuple structs representing value objects must keep their internal fields private (e.g. `pub struct SessionId(String);` or `pub struct ConfidenceScore(f32);`).
- **Constructor Validation**: Instantiation is only allowed through fallible constructors (`try_new` or `new` returning `Result<Self, DomainValidationError>`).
- **Behavior Over State**: Expose domain query methods (`score.is_high_confidence()`, `state.is_microphone_capturing()`) instead of exposing raw mutable internals.

### Examples

#### Rust Value Object
```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(String);

impl SessionId {
    pub fn try_new(raw: impl Into<String>) -> Result<Self, DomainValidationError> {
        let trimmed = raw.into().trim().to_string();
        if trimmed.is_empty() {
            return Err(DomainValidationError::EmptySessionId);
        }
        Ok(Self(trimmed))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
```

#### TypeScript Value Type & Guards
```typescript
export type SessionId = string & { readonly __brand: unique symbol };

export function parseSessionId(raw: string): SessionId {
  const trimmed = raw.trim();
  if (!trimmed) {
    throw new Error('SessionId cannot be empty');
  }
  return trimmed as SessionId;
}
```

---

## 3. Law of Demeter & Tell, Don't Ask

### Architectural Rule
Callers must never reach into child or grandchild dependencies to query state and make operational decisions. 

- **Violation (Asking & Chaining)**:
  ```rust
  // BAD: Demeter violation and asking for state
  if session.audio_pipeline().input_device().is_capturing() {
      session.audio_pipeline().input_device().stop();
  }
  ```
- **Adherence (Telling)**:
  ```rust
  // GOOD: Tell the root orchestrator what to do
  session.stop_listening().await?;
  ```

Orchestration roots (`SessionManager`, `SpeakingRoomService`) encapsulate child adapters, audio channels, and tasks behind coarse, atomic commands (`start_session`, `pause`, `resume`, `stop`).

---

## 4. Method & Component Size Limits

| Artifact | Target Bound | Allowed Exceptions | Remediation Strategy |
|---|---|---|---|
| **Source Files (`.rs`, `.ts`, `.tsx`, `.sql`)** | $\le$ 200 lines | None | Split domain types, trait implementations, hooks, UI components, and test suites into dedicated files. |
| **Rust Functions / Methods** | $\le$ 10 lines | Exhaustive `match` dispatch tables, embedded SQL queries, async select loops | Each match arm or loop body must immediately call a helper function under 10 lines. |
| **Rust Structs & Modules** | $\le$ 50 lines (impl blocks) | Large Serde data transfer mappings or complex state machines | Split into focused trait implementations (`impl TraitA for X`, `impl TraitB for X`) across modular files. |
| **React Components (.tsx)** | $\le$ 50 lines | None | Extract logic into custom hooks (`useSpeakingSession`); extract UI into atomic sub-components (`MicButton`, `TranscriptCard`). |

---

## 5. Code Smell Detection & Refactoring Catalog

During TDD cycles, inspect for and refactor the following smells before completing any task:

1. **Primitive Obsession**:
   - *Smell*: Passing bare `String` for IDs, `f32` for audio levels, or `u32` for timestamps across services.
   - *Fix*: Wrap in domain Value Objects with validation.
2. **Feature Envy**:
   - *Smell*: A method in module A frequently reads fields from module B to compute a result.
   - *Fix*: Move the method or behavior directly into module B (Tell, Don't Ask).
3. **Data Clumps**:
   - *Smell*: `(sample_rate: u32, channels: u8, format: SampleFormat)` passed together repeatedly.
   - *Fix*: Bundle into a validated `AudioFormat` struct.
4. **Shotgun Surgery**:
   - *Smell*: Adding a new session state requires editing 8 different match tables across multiple modules.
   - *Fix*: Centralize state transitions inside the `SessionManager` state machine.
5. **Leaky Interfaces**:
   - *Smell*: Exposing provider-specific SDK types (`ollama::Client`, `cpal::Stream`) to the UI or IPC layer.
   - *Fix*: Mediate strictly through high-level domain contracts (`AiProvider`, `AudioInput`).
