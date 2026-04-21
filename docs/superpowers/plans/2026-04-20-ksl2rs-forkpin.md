# ksl2rs Fork-Pin Propagation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Update the `ksl2rs` workspace to pin its `kdl` dependency at the `njreid/kdl-rs` fork rev `05521dce0f3d2915f3fa6707f240c938dde3b818` with the `v1` + `expression-strings` feature flags, matching the pin used by `kdlfmt` and (soon) `kdli`. Ship as `ksl2rs 0.2.0`.

**Architecture:** Single-repo, single-file-of-note change. Swap `kdl = "6.5"` (stock crates.io) for the pinned git fork. Verify all tests still pass. Bump workspace version.

**Tech Stack:** Rust 2021 edition (as per ksl2rs today), `kdl` fork at `njreid/kdl-rs`, `miette`, `thiserror`, `regex`, `proc-macro2`, `quote`, `syn`, `prettyplease`, `insta`, `proptest`.

**Spec reference:** [`docs/superpowers/specs/2026-04-20-kdli-lsp/01-architecture.md`](../specs/2026-04-20-kdli-lsp/01-architecture.md) § "Fork-pin coherence".

**Repo location:** `/home/njr/code/ksl2rs/` — runs in a different repo from this plan file. Executing agent should `cd` there (or work in a worktree created from that repo).

---

## Pre-flight

- [ ] **Verify the ksl2rs repo is on a clean main branch.**

```bash
cd /home/njr/code/ksl2rs
git status --short
```
Expected: empty output (clean tree). If dirty, stop and sort that out first.

- [ ] **Verify current tests pass against the existing `kdl = "6.5"` dependency.**

```bash
cargo test --workspace
```
Expected: all green. Baseline before modification.

- [ ] **Note the current pin.**

```bash
grep '^kdl' Cargo.toml
```
Expected: `kdl = "6.5"` under `[workspace.dependencies]`.

---

## Task 1: Swap the kdl dependency to the pinned fork

**Files:**
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Update the workspace dependency**

Open `/home/njr/code/ksl2rs/Cargo.toml`. Under `[workspace.dependencies]`, change:

```toml
kdl = "6.5"
```

to:

```toml
kdl = { git = "https://github.com/njreid/kdl-rs", rev = "05521dce0f3d2915f3fa6707f240c938dde3b818", features = ["v1", "expression-strings"] }
```

- [ ] **Step 2: Regenerate the lockfile**

```bash
cargo update -p kdl
cargo build --workspace
```

Expected: the fork source is fetched; the build completes without source-incompatible errors. If a minor API drift between stock 6.5 and the fork surfaces here, fix the call sites in `ksl2rs-core` to match the fork's current shape. Most likely there is no drift since the fork is built on the same 6.x line.

- [ ] **Step 3: Run all tests**

```bash
cargo test --workspace
```

Expected: all green. If a test fails due to parser-behavior differences (e.g. expression strings now parse where they used to error), update the test expectation — the fork's behavior is the new canonical behavior.

---

## Task 2: Add CI pin-coherence check

The spec requires that both `kdli` and `ksl2rs` assert they resolve `kdl` to the same rev. Adding the check here (rather than only on the kdli side) catches future accidental upgrades on the ksl2rs side too.

**Files:**
- Create or modify: `.github/workflows/ci.yml` (or whatever CI config `ksl2rs` uses — check first)

- [ ] **Step 1: Identify the CI config**

```bash
ls -la /home/njr/code/ksl2rs/.github/workflows/ 2>&1
```
Expected: one or more YAML files. If there's no CI yet, this task becomes "add a minimal CI config with the pin-check job." Otherwise, extend the existing workflow.

- [ ] **Step 2: Add a pin-check job**

Add to the CI workflow (adapt to whatever runner/shell the existing CI uses). A shell-only check is sufficient:

```yaml
  fork-pin-check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Verify kdl fork pin
        run: |
          set -euo pipefail
          EXPECTED="05521dce0f3d2915f3fa6707f240c938dde3b818"
          if ! cargo tree -p kdl 2>/dev/null | grep -q "$EXPECTED"; then
            echo "ERROR: kdl dependency is not resolving to expected fork rev $EXPECTED" >&2
            cargo tree -p kdl >&2
            exit 1
          fi
          echo "OK: kdl resolves to $EXPECTED"
```

- [ ] **Step 3: Run the check locally to verify it would pass**

```bash
cd /home/njr/code/ksl2rs
EXPECTED="05521dce0f3d2915f3fa6707f240c938dde3b818"
cargo tree -p kdl | grep -q "$EXPECTED" && echo "OK" || echo "FAIL"
```
Expected: `OK`.

---

## Task 3: Version bump to 0.2.0

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `CHANGELOG.md` if present; otherwise skip

- [ ] **Step 1: Bump workspace version**

In `Cargo.toml`, change:

```toml
[workspace.package]
version = "0.1.0"
```

to:

```toml
[workspace.package]
version = "0.2.0"
```

- [ ] **Step 2: Refresh lockfile**

```bash
cargo build --workspace
```

- [ ] **Step 3: Add a changelog entry (if the repo has a CHANGELOG.md)**

```bash
ls CHANGELOG.md 2>/dev/null && echo exists || echo skip
```

If exists, prepend:

```markdown
## v0.2.0

- Switch `kdl` dependency to the `njreid/kdl-rs` fork at rev `05521dc`
  with `v1` + `expression-strings` features. Matches the pin used by
  `kdlfmt` and `kdli`.
- Add CI job asserting fork-pin coherence.
```

If no CHANGELOG.md, skip silently — the commit message carries the rationale.

---

## Task 4: Commit, tag, push

- [ ] **Step 1: Review the changeset**

```bash
git diff --stat
```
Expected: `Cargo.toml`, `Cargo.lock`, possibly `.github/workflows/ci.yml` and `CHANGELOG.md`. No source changes in `ksl2rs-core/src/`, `ksl2rs-codegen/src/`, `ksl2rs/src/` unless API drift forced them.

- [ ] **Step 2: Commit**

```bash
git add Cargo.toml Cargo.lock
# Also add CHANGELOG.md and .github/workflows/ci.yml if they were modified:
git add CHANGELOG.md 2>/dev/null || true
git add .github/workflows/ 2>/dev/null || true
git commit -m "$(cat <<'EOF'
feat: pin kdl to njreid/kdl-rs fork (rev 05521dc)

Aligns the kdl dependency with kdlfmt and kdli so CompiledSchema spans
interoperate across the three crates. Adds a CI fork-pin coherence
check. Bumps workspace version to 0.2.0.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 3: Tag**

```bash
git tag -a v0.2.0 -m "ksl2rs v0.2.0 — fork-pin propagation"
```

- [ ] **Step 4: Push**

```bash
git push origin main
git push origin v0.2.0
```

---

## Self-review checklist

- [ ] `Cargo.toml` shows `kdl = { git = "https://github.com/njreid/kdl-rs", rev = "05521dc...", features = ["v1", "expression-strings"] }`.
- [ ] `cargo tree -p kdl` output contains the expected rev hash.
- [ ] `cargo test --workspace` passes.
- [ ] Workspace version is `0.2.0`.
- [ ] CI pin-coherence job is present.
- [ ] No source file under `ksl2rs-core/src/`, `ksl2rs-codegen/src/`, or `ksl2rs/src/` was modified (unless API drift forced it — in which case, document the drift in the commit).
- [ ] `v0.2.0` tag exists and is pushed.
