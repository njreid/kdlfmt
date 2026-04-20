# 09 — Testing strategy

Four layers, bottom-up. Each is owned by specific tasks in the swarm plan.

## Layer 1 — unit tests per module

Every Batch-0 and Batch-1 task owns `<module>.rs` and `tests/<module>.rs`. Pure functions → trivial tests, no fixtures required beyond small string inputs.

`FsSnapshot` being a trait means discovery, cache, and pipeline tests run with an in-memory `MemFs` — no tempdirs, no CI flake, no cleanup ceremony.

Example (T0c discovery):

```rust
#[test]
fn sibling_wins_over_pragma() {
    let fs = MemFs::from_paths(&[
        ("/proj/foo.kdl", "/- ksl-schema alice\n"),
        ("/proj/foo.ksl", "/* sibling */"),
        ("/home/u/.config/kdl/registry/alice.ksl", "/* registry */"),
    ]);
    let r = Resolver::new("/home/u".into(), "/home/u/.config/kdl/registry".into(), Arc::new(fs));
    assert_matches!(r.resolve(Path::new("/proj/foo.kdl"), Some("alice")),
        SchemaResolution::Sibling { .. });
}
```

## Layer 2 — golden fixtures

`kdli-core/tests/golden/<name>/` directories, each containing:

- `tree/` — an input filesystem tree (rendered into `MemFs` at test time).
- `open.txt` — the URI of the document to open.
- `action.txt` — one of: `diagnose`, `complete <line> <col>`, `hover <line> <col>`, `code_actions <line> <col>`.
- `expected.snap` — `insta`-snapshotted expected output.

Required fixtures for v1:

- `01-sibling-match/`
- `02-pragma-match/`
- `03-pragma-missing-schema/`
- `04-registry-unique/`
- `05-registry-ambiguous/`
- `06-no-match-format-only/`
- `07-hot-reload-schema-edit/`
- `08-schema-compile-error-cascade/`
- `09-ksl-meta-validation/`
- `10-enum-completion/`
- `11-required-prop-missing-action/`
- `12-hover-with-doc-annotations/`
- `13-when-guard-opaque/`
- `14-composition-one-of/`

Snapshots are checked in. `cargo insta review` is used for updates. Snapshot drift is a first-class signal.

## Layer 3 — LSP integration tests

`kdli-server/tests/lsp_integration.rs` spawns the server in-process via a `tower-lsp` test harness. One test per MVP capability:

- initialize roundtrip (verify capabilities advertisement).
- `didOpen` → expect `publishDiagnostics` within 500ms.
- `didChange` → expect updated diagnostics.
- `textDocument/formatting` → expect edits matching `kdlfmt-core` output.
- `textDocument/completion` at a child-insertion point → expect known node names.
- `textDocument/hover` at a known prop → expect markdown.
- `textDocument/codeAction` on ambiguity diagnostic → expect "pick schema" actions.
- Hot-reload: modify a registry schema on disk (via `MemFs` + watcher test hook) → expect re-publication within 500ms.

Test harness: `async-lsp-test` or a hand-rolled `tower::Service` test wrapper — pick whichever is in good repair at implementation time. Both are cheap; integration cost is the fixture authoring, not the harness.

## Layer 4 — end-to-end smoke test

Spawned in CI. Runs the actual `kdli` binary, pipes JSON-RPC init + didOpen over stdio, asserts a `publishDiagnostics` payload appears on stdout.

Catches packaging regressions the in-process tests miss: PATH resolution, binary panics, missing asset embeds (`ksl-meta.ksl` must be `include_str!`'d, not read from disk).

One test file: `kdli-server/tests/smoke.sh` invoked via `xtask`. Fails loud on non-zero exit, missing response, or malformed JSON.

## Layer 5 — kdlfmt compat gate

Not new tests — the existing `kdlfmt/cli/tests/` must pass **unchanged** after T0a (the refactor). This is the no-regression tripwire for the extraction.

CI runs `cargo test -p kdlfmt --all-features` against the pre-refactor set of tests after `kdlfmt-core` is extracted. Any test that requires modification is a regression; the change is rolled back and re-done correctly.

## Fork-pin coherence CI

Both `kdli` and `ksl2rs` CI include a check:

```bash
cargo tree --invert --package kdl --format '{p} {r}' | \
  grep 'git+https://github.com/njreid/kdl-rs' | \
  grep -q '05521dce0f3d2915f3fa6707f240c938dde3b818'
```

(Exact rev constant lives in a CI env var so upgrades are one-file changes.) A mismatch fails CI before the build does, with a clear message.

## Performance tests

Out of scope for v1 automated CI, but there's a manual benchmark checklist before tagging 0.1.0:

- Open a 1000-node `.kdl` in an editor; `publishDiagnostics` should return in < 100ms.
- Edit a key, type 30 characters; steady-state diagnostic republish latency < 50ms per keystroke.
- `didOpen` on a cold cache (no pre-compiled schemas) should return in < 200ms total.

These aren't enforced; they're the "does it feel good" gate. If any fails, address before tagging.
