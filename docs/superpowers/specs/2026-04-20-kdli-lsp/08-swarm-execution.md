# 08 — Swarm execution plan

This section is the reason the design is shaped the way it is: isolated pure cores, trait-behind-I/O, interface-lock up front. They exist so a swarm of subagents can build the LSP in parallel without merge pain.

## Dispatching

Use `superpowers:subagent-driven-development` as the execution skill. That skill is designed for parallel subagents sharing a single repo and a tight integration cadence — exactly this project's shape.

## Dependency graph

```
Batch 0 (all parallel — no deps):
    T0a  kdlfmt refactor: extract kdlfmt-core
    T0b  kdli repo scaffold: workspace, Cargo.toml, CI
    T0c  Schema discovery module (pure)
    T0d  Bundled ksl-meta.ksl + meta_schema::compiles_cleanly test
    T0e  miette → internal Diagnostic converter (pure)
    T0f  Interface-lock (interfaces.rs + lib.rs mod tree)
    T0g  ksl2rs fork-pin propagation (owner: user)

            ┌─ integration meta-task B0 ─┐
            ▼                            ▼
Batch 1 (parallel — needs Batch 0):
    T1a  kdli-core crate skeleton + cursor module
    T1b  Completion provider (pure)
    T1c  Hover provider (pure)
    T1d  Code-action "pick schema" + "add required prop"

            ┌─ integration meta-task B1 ─┐
            ▼                            ▼
Batch 2 (mostly sequential — shared state):
    T2a  SchemaCache + watcher (single owner)
    T2b  DocumentStore (single owner)
      ↓   (T2a and T2b done)
    T2c  Diagnostic pipeline (glues T0e + T2a + T2b + T1b/c)
    T2d  Formatting handler (uses kdlfmt-core)

            ┌─ integration meta-task B2 ─┐
            ▼                            ▼
Batch 3 (sequential):
    T3a  tower-lsp server wiring + handler.rs
    T3b  LSP integration tests

            ┌─ integration meta-task B3 ─┐
            ▼                            ▼
Batch 4 (parallel — editor docs):
    T4a  Helix config + docs
    T4b  Neovim config + docs
    T4c  Zed extension scaffold (deferred content — scaffold only in v1)

            ┌─ integration meta-task B4 ─┐

v1 ship: tag kdli 0.1.0
```

## Interface-lock protocol (T0f)

1. T0f runs alone. No other Batch-0 task touches `kdli-core/src/lib.rs` or `kdli-core/src/interfaces.rs`.
2. T0f's deliverable: `interfaces.rs` compiling cleanly with `unimplemented!("T<id>")` bodies, full rustdoc on every `pub` item, referenced task IDs matching the plan.
3. **User reviews T0f before Batch 1 launches.** Not another agent. Rationale: interface-lock bugs only surface during Batch 1 integration, which is the most expensive place to fix them.
4. Once user approves, T0f is fast-forwarded into `main`; Batch 1 worktrees are created from that commit.
5. Batch 1 implementers modify only the *bodies* of functions in their owned files; they do not change signatures in `interfaces.rs`. Any signature change during Batch 1 is a coordination event — reported up, applied to `main` as a mini meta-task, all worktrees rebased.

## File ownership map

Swarm agents conflict when they write to the same file. This table is the conflict-prevention spec: each task's "owns (write)" cell is disjoint from every other parallel task's cell.

| Task | Owns (write) | Reads (may depend on) |
|------|--------------|-----------------------|
| T0a | `kdlfmt/kdlfmt-core/**`, `kdlfmt/cli/Cargo.toml`, `kdlfmt/Cargo.toml` | kdlfmt existing CLI code |
| T0b | `kdli/` entire repo, initial commit | n/a |
| T0c | `kdli/kdli-core/src/discovery.rs`, `kdli/kdli-core/tests/discovery.rs` | `interfaces.rs` |
| T0d | `kdli/kdli-core/assets/ksl-meta.ksl`, `kdli/kdli-core/tests/meta_schema.rs` | ksl2rs docs |
| T0e | `kdli/kdli-core/src/diagnostics.rs`, `kdli/kdli-core/tests/diagnostics.rs` | ksl2rs diag types |
| T0f | `kdli/kdli-core/src/interfaces.rs`, `kdli/kdli-core/src/lib.rs` mod declarations | spec section 07 |
| T0g | `ksl2rs/Cargo.toml` (separate repo) | n/a |
| T1a | `kdli/kdli-core/src/cursor.rs`, `kdli/kdli-core/tests/cursor.rs` | `interfaces.rs` |
| T1b | `kdli/kdli-core/src/completion.rs`, `kdli/kdli-core/tests/completion.rs` | `cursor.rs` interface |
| T1c | `kdli/kdli-core/src/hover.rs`, `kdli/kdli-core/tests/hover.rs` | `cursor.rs` interface |
| T1d | `kdli/kdli-core/src/code_action.rs`, `kdli/kdli-core/tests/code_action.rs` | diagnostics, resolution |
| T2a | `kdli/kdli-server/src/cache.rs`, `kdli/kdli-server/src/watcher.rs`, `kdli/kdli-server/tests/hot_reload.rs` | core interfaces |
| T2b | `kdli/kdli-server/src/documents.rs`, `kdli/kdli-server/tests/documents.rs` | core interfaces |
| T2c | `kdli/kdli-server/src/pipeline.rs`, `kdli/kdli-server/tests/pipeline.rs` | T2a, T2b, T0e |
| T2d | `kdli/kdli-server/src/format.rs`, `kdli/kdli-server/tests/format.rs` | kdlfmt-core |
| T3a | `kdli/kdli-server/src/main.rs`, `kdli/kdli-server/src/handler.rs` | all of kdli-core + kdli-server |
| T3b | `kdli/kdli-server/tests/lsp_integration.rs` + fixtures | running server |
| T4a | `kdli/docs/editors/helix.md` | n/a |
| T4b | `kdli/docs/editors/neovim.md` | n/a |
| T4c | `kdli/zed-extension/**` | n/a |

"Reads" never triggers conflicts — only "owns". The table is the lint.

## Worktree topology

Each leaf task runs in its own worktree, created via `superpowers:using-git-worktrees`:

```
kdli/
  (main)                     # canonical; advances on meta-task merges
  .worktrees/
    t0a-kdlfmt-refactor/
    t0c-discovery/
    t0d-meta-schema/
    …
```

- Base: current `main`.
- Merge: after integration meta-task, fast-forward `main` → archive worktree.
- Conflicts: should be rare given the ownership map. When they occur (typically during meta-task rebasing), coordinator agent resolves by re-running tests on the merged tree; if anything fails, the task author re-runs locally.

## Integration meta-task (per batch)

After every batch completes:

1. Coordinator rebases every task's branch onto latest `main`.
2. Runs `cargo build --workspace && cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --check`.
3. Runs the cross-repo fork-pin coherence check (see [`09-testing.md`](./09-testing.md)).
4. If green: fast-forward merge into `main`, archive worktrees, unblock the next batch.
5. If red: report per-task failure, authors fix in their worktrees, coordinator retries. Do not advance `main` until clean.

The meta-task is single-owner (no parallelism). Its cost sets an upper bound on how small tasks should get — smaller tasks → more integration overhead. Leaf tasks should aim for ~100-400 LOC of new code each, so meta-tasks aren't dominant.

## Stub-and-fill (TDD per `superpowers:test-driven-development`)

Batch-1 agents write tests first, against the `interfaces.rs` stubs. Tests compile on day 1 — they panic at `unimplemented!`. Agent then fills the body, tests pass. The test file is the contract; review focuses on whether the test captures the real requirement.

## Which tasks are *not* parallelizable

Worth naming explicitly so the swarm doesn't try:

- **T2a (SchemaCache + watcher)** — shared mutable state + async lifecycle. One agent.
- **T2b (DocumentStore)** — shared mutable state. One agent.
- **T3a (tower-lsp wiring)** — single-state-machine protocol translation. One agent.
- **T0f (interface-lock)** — by construction, it produces the thing others coordinate on. One agent, reviewed by user.

## Rough wall-clock estimate

Per-agent workday assumptions, no major surprises:

| Batch | Parallel agents | Wall-clock |
|-------|-----------------|------------|
| 0 | 7 | ~1 day |
| 1 | 4 | ~1 day |
| 2 | 2 (phases) | ~1 day |
| 3 | 1 + tests | ~1 day |
| 4 | 2-3 | ~0.5 day |

Total: ~4-5 agent-days for v1 scope. Integration meta-tasks add ~0.5-1 day. Real wall time depends on how swarm runs are scheduled; this sets a floor.
