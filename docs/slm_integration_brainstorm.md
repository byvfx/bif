# BIF SLM Integration — Brainstorm

> **Status:** Future / post-1.0 idea. Not planned for current milestones.

## Context

Exploring whether/how to embed a Small Language Model (SLM) trained on BIF's source so users can drive the app via natural language — assemble USD scenes, ask questions, trigger operations. No scripting runtime exists today; `AppEvent` + `NodeGraphEvent` buses are the only programmatic surface.

---

## Verdict: Yes, but start tiny

Both AI Engineering and Senior Dev perspectives agree on the same path.

---

## Phase 1 — Weekend MVP: Read-only Q&A Panel

**No app control. Just a knowledgeable assistant.**

**What:**

- egui text input panel + scrollable response area (new optional UI panel)
- Sidecar: `ollama` running locally (`qwen2.5:3b` or `qwen2.5-coder:3b`)
- Static context injected in system prompt: node type docs + AppEvent descriptions + wiki excerpts
- HTTP POST to `localhost:11434/api/generate` → stream tokens → display

**Stack:**

- `ureq` (sync HTTP) — don't add tokio for this
- `std::thread` + `mpsc::channel` — inference off main thread
- Lives in `bif_viewer` behind `--features ai`
- Model: `ollama pull qwen2.5:3b` (~2GB), user installs ollama separately

**Effort:** ~200 lines of Rust. Achievable in one weekend.

---

## Phase 2 — AppEvent Dispatch (structured output)

**SLM can actually drive BIF.**

**What:**

- System prompt adds: JSON schema of `AppEvent`/`NodeGraphEvent` variants + current scene state (compact JSON of active nodes + key params, ~500 tokens)
- Grammar-constrained decoding via GBNF (ollama supports `format: "json"` + schema)
- SLM outputs:

  ```json
  {
    "actions": [
      { "type": "emit_event", "event": "LoadUsdFile", "params": { "path": "..." } }
    ],
    "explanation": "..."
  }
  ```

- Rust parses → validates (enum check + type check) → dispatches to `EventBus`
- New `AiAction` enum in `bif_ai` crate — subset of AppEvents that are AI-callable

**Critical guard:** Never dispatch unvalidated output. All `node_type` values checked against hardcoded enum. All params schema-validated via serde before emit.

**Files to create/modify:**

- New crate `bif_ai/` — model client, `AiAction` enum, prompt builder, validation
- `crates/bif_viewport/src/app_event.rs` — may need `AiDispatch(Vec<AiAction>)` variant
- `crates/bif_viewer/src/main.rs` — wire AI panel + feature flag
- `Cargo.toml` — `--features ai` gate

---

## Key Decisions

| Question | Answer |
|---|---|
| Fine-tune or RAG? | RAG — codebase too small to fine-tune without overfitting |
| Embedded or sidecar? | **Sidecar (ollama)** on day 1; embedded (`llama-cpp-2`) is v2 |
| Model size? | Qwen2.5-Coder 1.5B (CPU) or 3B (light GPU) |
| Output format? | Grammar-constrained JSON via GBNF |
| GPU inference? | CPU only — avoid Vulkan compute conflict with wgpu |
| .bifa as output? | Only for "generate full project" use case, not interactive |

---

## RAG Corpus (ordered by ROI)

1. `AppEvent` + `NodeGraphEvent` enum → JSON schema with descriptions
2. Node type catalog (name, all fields, valid ranges) → JSON (10 nodes, ~500 tokens)
3. 5-10 example `.bifa` project files as few-shot demonstrations
4. Wiki articles (42 exist, embed as markdown chunks)
5. Doc comments only — not full source bodies

---

## What Works vs What Fails

**Works:**

- "Load /tmp/scene.usda" → `LoadUsdFile` event
- "Export to /tmp/out.usda" → `ExportUsd` event
- "Switch variant to winter" → `VariantChanged` event
- "What prims are in my stage?" (with scene state in context)
- "What does the Scatter node do?" (pure Q&A)

**Unreliable:**

- Complex multi-step graph assembly from scratch
- Anything requiring reasoning about implicit scene state not in context
- Creative/aesthetic decisions

---

## Failure Mode Mitigations

- Hallucinated node types → validate against enum before dispatch, reject + re-prompt
- Wrong param types → serde schema validation before any EventBus emit
- Context overflow → inject schema + compact scene state only, never full source

---

## Open Questions (answer before building)

1. Bundle llamafile or require ollama install?
2. `--features ai` hard dependency or graceful degradation if ollama not running?
3. Panel placement — docked panel, floating overlay, or command palette (Ctrl+Space)?
4. Phase 2: confirmation UI before dispatching AI actions, or fire immediately?
5. Which wiki articles are highest priority for the RAG corpus?
