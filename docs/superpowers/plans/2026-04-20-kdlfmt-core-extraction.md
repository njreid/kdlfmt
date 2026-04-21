# kdlfmt-core Extraction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract the formatter, config, and parser-integration code from the `kdlfmt/cli` crate into a new sibling library crate `kdlfmt-core`, so `kdli` (and other downstream consumers) can depend on the formatter without dragging in the CLI surface.

**Architecture:** Split the current single-crate workspace into `kdlfmt-core` (library: pure formatter, config, editorconfig resolution, parser helpers) + `cli` (thin CLI binary that depends on `kdlfmt-core`). Every CLI test must pass unchanged after the refactor — behavior is frozen.

**Tech Stack:** Rust 2024 edition, `kdl` fork at `njreid/kdl-rs` rev `05521dc`, `miette`, `ec4rs`, `dashmap`, `thiserror` (new dependency for core).

**Spec reference:** [`docs/superpowers/specs/2026-04-20-kdli-lsp/01-architecture.md`](../specs/2026-04-20-kdli-lsp/01-architecture.md) § "This repo (kdlfmt) after the refactor".

---

## Pre-flight

Before starting, verify the current state:

- [ ] **Verify on `main` branch, tree clean of uncommitted work related to this refactor.**

```bash
cd /home/njr/code/kdlfmt
git status --short
```
Expected: only unrelated `.remember/` log files; no src changes.

- [ ] **Verify existing tests pass.**

```bash
cargo test --workspace
```
Expected: all green. Any pre-existing failure means this refactor does not start — fix first.

- [ ] **Record the current version.**

```bash
grep '^version' Cargo.toml
```
Expected: `version = "0.1.6-b"` (or similar 0.1.x — whatever's current).

---

## File Structure

After this plan, the workspace will look like:

```
kdlfmt/
├── Cargo.toml                       # workspace — add kdlfmt-core member
├── kdlfmt-core/                     # NEW library crate
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                   # public re-exports
│       ├── version.rs               # KdlVersion (moved from cli/src/cli.rs)
│       ├── config.rs                # KdlFmtConfig (moved verbatim from cli/src/config.rs)
│       ├── kdl.rs                   # parse_kdl, format_kdl, reject_expression_strings
│       ├── error.rs                 # FmtError (new, thinner than CLI's KdlFmtError)
│       └── high_level.rs            # format_document, format_check convenience fns
│   └── tests/
│       ├── format_document.rs       # library-level API tests
│       └── edge_cases.rs            # moved from cli/src/edge_cases.rs
└── cli/                             # existing bin — imports from kdlfmt-core
    ├── Cargo.toml                   # adds dependency on kdlfmt-core
    └── src/
        ├── main.rs                  # unchanged
        ├── cli.rs                   # removes KdlVersion (re-exported from core)
        ├── config.rs                # DELETED (moved to core)
        ├── kdl.rs                   # DELETED (moved to core)
        ├── edge_cases.rs            # DELETED (moved to core/tests)
        ├── error.rs                 # wraps kdlfmt_core::FmtError
        ├── commands/…                # imports updated
        ├── fs.rs                    # unchanged
        └── terminal/…                # unchanged
```

---

## Task 1: Scaffold the kdlfmt-core crate

**Files:**
- Create: `kdlfmt-core/Cargo.toml`
- Create: `kdlfmt-core/src/lib.rs`
- Modify: `Cargo.toml` (workspace root — add member)

- [ ] **Step 1: Write the failing test**

Create `kdlfmt-core/tests/smoke.rs`:

```rust
// Smoke test: the crate exists and exports a version constant.

#[test]
fn crate_compiles_and_exports_version() {
    assert!(!kdlfmt_core::VERSION.is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /home/njr/code/kdlfmt
cargo test -p kdlfmt-core
```
Expected: `error: could not find Cargo.toml in ...` or `package kdlfmt-core not found in workspace`.

- [ ] **Step 3: Create the crate manifest**

`kdlfmt-core/Cargo.toml`:

```toml
[package]
name = "kdlfmt-core"
version = { workspace = true }
authors = { workspace = true }
edition = { workspace = true }
description = "Library crate for kdlfmt: formatter, config, and parser helpers."
documentation = { workspace = true }
readme = { workspace = true }
homepage = { workspace = true }
repository = { workspace = true }
license = { workspace = true }
keywords = { workspace = true }
categories = { workspace = true }

[dependencies]
dashmap = { workspace = true }
ec4rs = { workspace = true }
kdl = { workspace = true }
miette = { workspace = true }
thiserror = "2"
```

- [ ] **Step 4: Create the library entry point**

`kdlfmt-core/src/lib.rs`:

```rust
//! Core library behind the `kdlfmt` CLI.
//!
//! This crate exposes the formatter, config types, and parser helpers that
//! used to live inside `kdlfmt/cli`. It is designed to be consumed by the
//! `kdlfmt` binary, `kdli` (the KDL LSP), and any other tool that wants the
//! same formatting behavior without the CLI surface.

/// Semver version string of this crate. Matches Cargo.toml.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
```

- [ ] **Step 5: Register the crate in the workspace**

Modify `Cargo.toml` at the repo root. Change:

```toml
[workspace]
resolver = "3"
members = ["cli"]
exclude = []
```

to:

```toml
[workspace]
resolver = "3"
members = ["cli", "kdlfmt-core"]
exclude = []
```

Also add `thiserror = "2"` to the `[workspace.dependencies]` block (sorted alphabetically with the others).

- [ ] **Step 6: Run the smoke test to verify it now passes**

```bash
cargo test -p kdlfmt-core
```
Expected: `test crate_compiles_and_exports_version ... ok`.

- [ ] **Step 7: Run the full workspace test suite**

```bash
cargo test --workspace
```
Expected: all green, including every existing `cli` test unchanged.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml kdlfmt-core/
git commit -m "$(cat <<'EOF'
feat(core): scaffold kdlfmt-core library crate

Empty crate with VERSION constant and smoke test.
Workspace now has members = ["cli", "kdlfmt-core"].

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Move KdlVersion into kdlfmt-core

`KdlVersion` currently lives in `cli/src/cli.rs` but is imported by `cli/src/kdl.rs` and `cli/src/config.rs`. The formatter needs it; so do CLI args. Move to core, re-export from cli.

**Files:**
- Create: `kdlfmt-core/src/version.rs`
- Modify: `kdlfmt-core/src/lib.rs`
- Modify: `cli/src/cli.rs` (remove definition, re-export from core)
- Modify: `cli/Cargo.toml` (add dep on `kdlfmt-core`)

- [ ] **Step 1: Write the failing test**

Append to `kdlfmt-core/tests/smoke.rs`:

```rust
#[test]
fn kdl_version_is_exported() {
    use kdlfmt_core::KdlVersion;
    let _: KdlVersion = KdlVersion::V1;
    let _: KdlVersion = KdlVersion::V2;
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p kdlfmt-core kdl_version_is_exported
```
Expected: `unresolved import 'kdlfmt_core::KdlVersion'`.

- [ ] **Step 3: Move the definition**

Read `cli/src/cli.rs` and find the `KdlVersion` enum + its `clap::ValueEnum` impl (lines around where the enum is declared). Copy the enum definition (NOT the clap derives — clap stays in cli).

Create `kdlfmt-core/src/version.rs`:

```rust
//! KDL specification version selection.

/// Which KDL specification version the formatter should target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KdlVersion {
    /// KDL v1 (legacy).
    V1,
    /// KDL v2 (current).
    V2,
}
```

Modify `kdlfmt-core/src/lib.rs` to add:

```rust
mod version;
pub use version::KdlVersion;
```

- [ ] **Step 4: Update cli to depend on kdlfmt-core**

Modify `cli/Cargo.toml`, under `[dependencies]`, add (alphabetically):

```toml
kdlfmt-core = { path = "../kdlfmt-core" }
```

- [ ] **Step 5: Update cli/src/cli.rs to re-export instead of define**

In `cli/src/cli.rs`, delete the `KdlVersion` enum definition. Add near the top of the file:

```rust
pub use kdlfmt_core::KdlVersion;
```

Keep the `clap::ValueEnum` derive in cli by adding a wrapper, OR keep clap support inside cli. **Recommended approach:** implement `clap::ValueEnum` manually for `KdlVersion` inside `cli/src/cli.rs` via a newtype OR by implementing the trait directly:

```rust
impl clap::ValueEnum for KdlVersion {
    fn value_variants<'a>() -> &'a [Self] {
        &[KdlVersion::V1, KdlVersion::V2]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        Some(match self {
            Self::V1 => clap::builder::PossibleValue::new("v1"),
            Self::V2 => clap::builder::PossibleValue::new("v2"),
        })
    }
}
```

Note: Rust's orphan rules permit implementing `clap::ValueEnum` for a foreign type only in the defining crate. Since `KdlVersion` is in `kdlfmt-core`, either (a) put the `clap::ValueEnum` impl in core behind a `clap` feature flag, or (b) use a thin newtype wrapper in cli. Prefer **(a) feature flag** — smaller blast radius:

Modify `kdlfmt-core/Cargo.toml`:

```toml
[features]
default = []
clap = ["dep:clap"]

[dependencies]
# … existing deps …
clap = { workspace = true, optional = true }
```

In `kdlfmt-core/src/version.rs`, add (only under the `clap` feature):

```rust
#[cfg(feature = "clap")]
impl clap::ValueEnum for KdlVersion {
    fn value_variants<'a>() -> &'a [Self] {
        &[KdlVersion::V1, KdlVersion::V2]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        Some(match self {
            Self::V1 => clap::builder::PossibleValue::new("v1"),
            Self::V2 => clap::builder::PossibleValue::new("v2"),
        })
    }
}
```

In `cli/Cargo.toml`, update the dep to enable the feature:

```toml
kdlfmt-core = { path = "../kdlfmt-core", features = ["clap"] }
```

- [ ] **Step 6: Update all imports in cli**

Any file that did `use crate::cli::KdlVersion` — keep as-is (since `cli.rs` now re-exports). Any file that did `use crate::cli::*` can stay. Run:

```bash
cargo build --workspace 2>&1 | head -40
```
Fix any import errors reported. Expected imports touched: `cli/src/kdl.rs`, `cli/src/config.rs`, `cli/src/commands/*.rs` — all should still compile because they import via `crate::cli::KdlVersion`.

- [ ] **Step 7: Run all tests**

```bash
cargo test --workspace
```
Expected: all green. The `kdl_version_is_exported` test passes.

- [ ] **Step 8: Commit**

```bash
git add kdlfmt-core/ cli/Cargo.toml cli/src/cli.rs Cargo.lock
git commit -m "$(cat <<'EOF'
refactor(core): move KdlVersion to kdlfmt-core

KdlVersion now lives in kdlfmt-core with an optional clap ValueEnum
impl behind a feature flag. The cli crate re-exports via use.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Move KdlFmtConfig into kdlfmt-core

`config.rs` is ~300 lines of config type + default + load + editorconfig resolution. It depends only on `kdl`, `ec4rs`, and `KdlVersion` (which just moved). No CLI dependencies.

**Files:**
- Create: `kdlfmt-core/src/config.rs` (moved from `cli/src/config.rs`)
- Modify: `kdlfmt-core/src/lib.rs` (add module + re-export)
- Delete: `cli/src/config.rs`
- Modify: `cli/src/main.rs`, `cli/src/commands/mod.rs`, and every command file that imports from `crate::config` — update to `use kdlfmt_core::KdlFmtConfig;`

- [ ] **Step 1: Write the failing test**

Create `kdlfmt-core/tests/config.rs`:

```rust
use kdlfmt_core::KdlFmtConfig;

#[test]
fn default_config_has_four_space_indent() {
    let cfg = KdlFmtConfig::default();
    assert_eq!(cfg.indent, "    ");
    assert!(!cfg.use_tabs);
}

#[test]
fn config_filename_constant_is_kdlfmt_kdl() {
    assert_eq!(KdlFmtConfig::filename(), "kdlfmt.kdl");
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p kdlfmt-core --test config
```
Expected: `unresolved import 'kdlfmt_core::KdlFmtConfig'`.

- [ ] **Step 3: Move the file**

```bash
git mv cli/src/config.rs kdlfmt-core/src/config.rs
```

Open `kdlfmt-core/src/config.rs` and update imports at the top. Any `use crate::cli::KdlVersion` becomes `use crate::KdlVersion`. Any `use crate::error::KdlFmtError` needs attention — `KdlFmtError` doesn't exist in core yet. Find every usage of `KdlFmtError` in this file; the load functions currently return `Result<_, KdlFmtError>`. Temporarily replace with `Result<_, miette::Report>` — we'll define `FmtError` in Task 5 and convert.

Actually, `cli/src/config.rs` currently uses `KdlFmtError` only inside `KdlFmtConfig::load` which does file I/O and parsing. For now, change its return type to `Result<KdlFmtConfig, miette::Report>` and map errors with `miette::Report::msg` or similar. Exact edits will depend on the file contents — read it first, then adapt.

- [ ] **Step 4: Register the module**

Add to `kdlfmt-core/src/lib.rs`:

```rust
mod config;
pub use config::KdlFmtConfig;
```

- [ ] **Step 5: Update cli to import from core**

The cli used to have `use crate::config::KdlFmtConfig;`. Now it should be `use kdlfmt_core::KdlFmtConfig;`.

Do a find/replace across `cli/src/`:

```bash
grep -rn 'crate::config' cli/src/
```

Expected matches: `cli/src/commands/mod.rs`, `cli/src/commands/check.rs`, `cli/src/commands/format.rs`, `cli/src/commands/init.rs`, and `cli/src/main.rs` if it imports from `crate::config`.

Replace every `use crate::config::` with `use kdlfmt_core::` and every `crate::config::KdlFmtConfig` with `kdlfmt_core::KdlFmtConfig`. Similarly for `KdlFmtConfig::filename()` etc. — these become method calls on the re-exported type.

Also remove the `mod config;` declaration from `cli/src/main.rs`.

- [ ] **Step 6: Build**

```bash
cargo build --workspace 2>&1 | head -60
```

Fix any remaining import errors. The most likely issue: `KdlFmtError` variants used inside the moved `config.rs`. If those remain, temporarily convert the error to `miette::Report::msg("…")` — Task 5 cleans this up properly.

- [ ] **Step 7: Run the tests**

```bash
cargo test --workspace
```
Expected: all green, new config tests pass.

- [ ] **Step 8: Commit**

```bash
git add kdlfmt-core/ cli/ Cargo.lock
git commit -m "$(cat <<'EOF'
refactor(core): move KdlFmtConfig to kdlfmt-core

Full config type (KdlFmtConfig, load, editorconfig resolution, filename
constants) moves from cli/src/config.rs to kdlfmt-core/src/config.rs
verbatim. Error type usage temporarily routed through miette::Report;
a proper core FmtError lands in a follow-up task.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Move parser + formatter helpers into kdlfmt-core

`cli/src/kdl.rs` contains `parse_kdl`, `format_kdl`, `reject_expression_strings`. Purely formatter logic. Moves to core.

**Files:**
- Create: `kdlfmt-core/src/kdl.rs` (moved from `cli/src/kdl.rs`)
- Modify: `kdlfmt-core/src/lib.rs`
- Delete: `cli/src/kdl.rs`
- Modify: every cli file that imported from `crate::kdl` — switch to `kdlfmt_core`

- [ ] **Step 1: Write the failing test**

Append to `kdlfmt-core/tests/config.rs` (or create a new `kdl.rs` test file):

Create `kdlfmt-core/tests/format.rs`:

```rust
use kdlfmt_core::{format_kdl, parse_kdl, KdlFmtConfig, KdlVersion};

#[test]
fn parse_and_format_roundtrips_simple_kdl() {
    let input = "node 1 2 3\n";
    let (doc, version) = parse_kdl(input, Some(KdlVersion::V2), false)
        .expect("valid kdl parses");
    let cfg = KdlFmtConfig::default();
    let out = format_kdl(doc, &cfg, version);
    assert!(out.contains("node"));
}

#[test]
fn reject_expression_strings_catches_backtick_args() {
    let input = "node `expr`\n";
    let err = parse_kdl(input, Some(KdlVersion::V2), false);
    assert!(err.is_err(), "backtick arg should be rejected when allow_expression_strings=false");
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p kdlfmt-core --test format
```
Expected: `unresolved import 'kdlfmt_core::format_kdl'`.

- [ ] **Step 3: Move the file**

```bash
git mv cli/src/kdl.rs kdlfmt-core/src/kdl.rs
```

Open the moved file and fix imports. Change `use crate::cli::KdlVersion` to `use crate::KdlVersion` (since it's now a sibling module in the same crate). Change `use crate::config::KdlFmtConfig` to `use crate::KdlFmtConfig`.

- [ ] **Step 4: Register the module**

Add to `kdlfmt-core/src/lib.rs`:

```rust
mod kdl;
pub use kdl::{format_kdl, parse_kdl};
```

(Do NOT re-export `reject_expression_strings` — it's an internal helper.)

- [ ] **Step 5: Update cli imports**

```bash
grep -rn 'crate::kdl' cli/src/
```

Expected matches: `cli/src/commands/check.rs`, `cli/src/commands/format.rs`, `cli/src/commands/init.rs`, and `cli/src/edge_cases.rs`.

Replace `use crate::kdl::{format_kdl, parse_kdl}` with `use kdlfmt_core::{format_kdl, parse_kdl}` in each file.

Remove `mod kdl;` from `cli/src/main.rs`.

- [ ] **Step 6: Build + test**

```bash
cargo build --workspace
cargo test --workspace
```
Expected: all green.

- [ ] **Step 7: Commit**

```bash
git add kdlfmt-core/ cli/ Cargo.lock
git commit -m "$(cat <<'EOF'
refactor(core): move parse_kdl/format_kdl to kdlfmt-core

Pure formatter helpers move from cli/src/kdl.rs to
kdlfmt-core/src/kdl.rs. reject_expression_strings stays private to
the crate; parse_kdl and format_kdl are publicly exported.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Define a proper FmtError in kdlfmt-core

Replace the stopgap `miette::Report` return values with a real error type. CLI keeps `KdlFmtError` for CLI-specific cases and wraps `FmtError` where it applies.

**Files:**
- Create: `kdlfmt-core/src/error.rs`
- Modify: `kdlfmt-core/src/lib.rs`
- Modify: `kdlfmt-core/src/config.rs` (return `Result<_, FmtError>`)
- Modify: `kdlfmt-core/src/kdl.rs` (return `Result<_, FmtError>` for parse_kdl)
- Modify: `cli/src/error.rs` (add a `From<FmtError>` conversion)
- Modify: `cli/src/commands/*.rs` (wrap FmtError where needed)

- [ ] **Step 1: Write the failing test**

Create `kdlfmt-core/tests/error.rs`:

```rust
use kdlfmt_core::FmtError;

#[test]
fn fmt_error_is_debug_and_display() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
    let err = FmtError::Io(io_err);
    let _ = format!("{err:?}");
    let _ = format!("{err}");
}

#[test]
fn fmt_error_parse_carries_underlying() {
    let bad = "invalid `";
    let result = kdlfmt_core::parse_kdl(bad, None, true);
    let Err(_err): Result<_, kdlfmt_core::FmtError> = result else {
        panic!("expected parse error");
    };
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p kdlfmt-core --test error
```
Expected: `unresolved import 'kdlfmt_core::FmtError'` (and the `parse_kdl` signature currently returns `miette::Report`, not `FmtError`).

- [ ] **Step 3: Create the error type**

`kdlfmt-core/src/error.rs`:

```rust
//! Error types surfaced by `kdlfmt-core`.

use miette::Diagnostic;
use thiserror::Error;

/// Errors produced by the core library.
///
/// CLI-specific errors (exit-code sentinels, init-already-exists, etc.)
/// stay out of this crate and live in the `kdlfmt` binary's own error type.
#[derive(Debug, Error, Diagnostic)]
pub enum FmtError {
    /// The input could not be parsed as KDL.
    #[error(transparent)]
    #[diagnostic(transparent)]
    Parse(#[from] kdl::KdlError),

    /// I/O failure (reading config, reading editorconfig).
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),

    /// A config file was present but malformed.
    #[error("invalid config: {0}")]
    InvalidConfig(String),

    /// A non-standard expression-string was encountered while
    /// parsing with expression-strings disallowed.
    #[error("expression strings are not permitted here")]
    #[diagnostic(
        code(kdlfmt::expression_string_rejected),
        help("enable expression strings by passing `--no-expression-strings=false` or removing the opt-out flag")
    )]
    ExpressionStringRejected {
        #[label("expression string here")]
        span: miette::SourceSpan,
        node_name: String,
        entry_label: String,
    },
}
```

- [ ] **Step 4: Register + update sites**

Add to `kdlfmt-core/src/lib.rs`:

```rust
mod error;
pub use error::FmtError;
```

Update `kdlfmt-core/src/kdl.rs`:

- Change `parse_kdl` signature from `miette::Result<(kdl::KdlDocument, KdlVersion)>` to `Result<(kdl::KdlDocument, KdlVersion), FmtError>`.
- Change `reject_expression_strings` to return `Result<(), FmtError>`; when it would fail, construct `FmtError::ExpressionStringRejected { span, node_name, entry_label }` with the location it already computes.
- Map the `kdl::KdlError` from `kdl::KdlDocument::parse_v1` / `parse_v2` through the `From<kdl::KdlError>` impl: use `?` and conversion is automatic.

Update `kdlfmt-core/src/config.rs`:

- Change `KdlFmtConfig::load` return type from `Result<_, miette::Report>` to `Result<KdlFmtConfig, FmtError>`.
- Map file-read errors through `FmtError::Io` (automatic via `?`).
- Map KDL parse errors through `FmtError::Parse` (automatic).
- For "invalid config" semantic errors (missing required node, wrong type), use `FmtError::InvalidConfig("missing indent_size".into())` style.

- [ ] **Step 5: Update cli's KdlFmtError to wrap FmtError**

Open `cli/src/error.rs`. Find the `KdlFmtError` enum. Add a variant:

```rust
#[error(transparent)]
#[diagnostic(transparent)]
Fmt(#[from] kdlfmt_core::FmtError),
```

Also add `use kdlfmt_core::FmtError;` at the top if needed. Any code that was explicitly constructing `KdlFmtError::Io(...)` for errors that came from core-side I/O should now let `?` promote via the new `From<FmtError>` conversion. Keep existing `KdlFmtError::Io(std::io::Error)` for CLI-only I/O (stdin read, walker-layer errors).

- [ ] **Step 6: Build + test**

```bash
cargo build --workspace 2>&1 | head -60
cargo test --workspace
```
Expected: all green.

- [ ] **Step 7: Commit**

```bash
git add kdlfmt-core/ cli/ Cargo.lock
git commit -m "$(cat <<'EOF'
feat(core): introduce FmtError

Adds a real error type for kdlfmt-core backed by thiserror + miette.
Replaces the stopgap miette::Report return values inside parse_kdl and
KdlFmtConfig::load. The cli crate's KdlFmtError grows a Fmt variant
that wraps FmtError transparently.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Add high-level convenience functions

Expose `format_document` and `format_check` so kdli (and other consumers) have a one-call API that does parse + format in a single shot.

**Files:**
- Create: `kdlfmt-core/src/high_level.rs`
- Modify: `kdlfmt-core/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Create `kdlfmt-core/tests/format_document.rs`:

```rust
use kdlfmt_core::{format_check, format_document, FormatCheck, KdlFmtConfig, KdlVersion};

#[test]
fn format_document_returns_formatted_string() {
    let input = "node 1   2    3\n";
    let out = format_document(input, KdlVersion::V2, &KdlFmtConfig::default())
        .expect("formats cleanly");
    // Idempotent: formatting again yields the same string.
    let out2 = format_document(&out, KdlVersion::V2, &KdlFmtConfig::default())
        .expect("second pass formats cleanly");
    assert_eq!(out, out2);
}

#[test]
fn format_check_reports_needs_format() {
    let input = "node   1\n";
    let check = format_check(input, KdlVersion::V2, &KdlFmtConfig::default())
        .expect("checks cleanly");
    assert!(!check.was_formatted, "raw input is not already formatted");
    assert_ne!(check.would_write, input);
}

#[test]
fn format_check_reports_already_formatted() {
    let input = "node 1\n";
    // Run once to normalize.
    let normalized = format_document(input, KdlVersion::V2, &KdlFmtConfig::default()).unwrap();
    let check = format_check(&normalized, KdlVersion::V2, &KdlFmtConfig::default()).unwrap();
    assert!(check.was_formatted, "normalized output is already formatted");
    assert_eq!(check.would_write, normalized);
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p kdlfmt-core --test format_document
```
Expected: `unresolved import 'kdlfmt_core::format_document'`.

- [ ] **Step 3: Implement the high-level API**

Create `kdlfmt-core/src/high_level.rs`:

```rust
//! Convenience API combining parse + format in one call.

use crate::{format_kdl, parse_kdl, FmtError, KdlFmtConfig, KdlVersion};

/// Parse `source` as KDL and return the formatted output.
///
/// Expression strings (backtick-delimited arguments) are *allowed*.
/// Callers that need to reject them should use [`parse_kdl`] directly.
pub fn format_document(
    source: &str,
    version: KdlVersion,
    cfg: &KdlFmtConfig,
) -> Result<String, FmtError> {
    let (doc, version) = parse_kdl(source, Some(version), true)?;
    Ok(format_kdl(doc, cfg, version))
}

/// Result of a format-check pass.
pub struct FormatCheck {
    /// `true` if the input was already byte-identical to the formatter's output.
    pub was_formatted: bool,
    /// What a format pass would produce (identical to input when `was_formatted`).
    pub would_write: String,
}

/// Parse + format, and report whether the input was already normalized.
pub fn format_check(
    source: &str,
    version: KdlVersion,
    cfg: &KdlFmtConfig,
) -> Result<FormatCheck, FmtError> {
    let formatted = format_document(source, version, cfg)?;
    Ok(FormatCheck {
        was_formatted: formatted == source,
        would_write: formatted,
    })
}
```

Add to `kdlfmt-core/src/lib.rs`:

```rust
mod high_level;
pub use high_level::{format_check, format_document, FormatCheck};
```

- [ ] **Step 4: Run the tests**

```bash
cargo test -p kdlfmt-core --test format_document
cargo test --workspace
```
Expected: all green.

- [ ] **Step 5: Commit**

```bash
git add kdlfmt-core/
git commit -m "$(cat <<'EOF'
feat(core): add format_document and format_check high-level API

format_document combines parse + format in one call. format_check
runs a format pass and reports whether the input was already
normalized. Both return Result<_, FmtError>.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: Move edge_cases tests into kdlfmt-core

`cli/src/edge_cases.rs` is a test-only file that exercises the formatter directly — no CLI state. Belongs in core.

**Files:**
- Create: `kdlfmt-core/tests/edge_cases.rs`
- Delete: `cli/src/edge_cases.rs`
- Modify: `cli/src/main.rs` (remove `mod edge_cases;`)

- [ ] **Step 1: Move and adjust imports**

```bash
git mv cli/src/edge_cases.rs kdlfmt-core/tests/edge_cases.rs
```

Edit `kdlfmt-core/tests/edge_cases.rs`:
- Change `use crate::cli::KdlVersion` → `use kdlfmt_core::KdlVersion`.
- Change `use crate::config::KdlFmtConfig` → `use kdlfmt_core::KdlFmtConfig`.
- Change `use crate::kdl::{format_kdl, parse_kdl}` → `use kdlfmt_core::{format_kdl, parse_kdl}`.

- [ ] **Step 2: Remove the module declaration from cli**

Find `mod edge_cases;` (likely inside a `#[cfg(test)]` block) in `cli/src/main.rs` (or wherever it was declared) and remove it.

- [ ] **Step 3: Run tests**

```bash
cargo test -p kdlfmt-core --test edge_cases
cargo test --workspace
```
Expected: all edge-case tests pass in the new location. CLI tests unchanged.

- [ ] **Step 4: Commit**

```bash
git add cli/ kdlfmt-core/
git commit -m "$(cat <<'EOF'
test(core): move edge_cases tests to kdlfmt-core

These tests exercise the formatter library directly with no CLI
dependencies; they belong alongside the code under test.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: Verify CLI compat gate (no-regression sweep)

The spec is explicit: every `assert_cmd`-based CLI test must pass **unchanged**. This is the tripwire.

- [ ] **Step 1: Enumerate existing CLI tests**

```bash
ls cli/tests/
```
Expected: `check.rs`, `completions.rs`, `format.rs`, `help.rs`, `init.rs`, `fixtures/`.

- [ ] **Step 2: Confirm no CLI test file was modified in this refactor**

```bash
git log --oneline -- cli/tests/ | head -5
```
Expected: no commits from this refactor touch `cli/tests/` (only `cli/src/` and `kdlfmt-core/`). If any CLI test file appears in the diff, that is a regression — back it out.

- [ ] **Step 3: Run full CLI test suite in release mode (matches CI)**

```bash
cargo test --release -p kdlfmt --all-features
```
Expected: all green.

- [ ] **Step 4: Run the full workspace in release mode**

```bash
cargo test --release --workspace
```
Expected: all green.

- [ ] **Step 5: Run the CLI against its own config as a smoke check**

```bash
cargo build --release
./target/release/kdlfmt check kdlfmt.kdl
```
Expected: exit 0, no stdout output (the config file is already formatted).

```bash
./target/release/kdlfmt --help
```
Expected: same help output as before the refactor. Diff against pre-refactor output if in doubt (regenerate `cli/src/*.rs` help templates are unchanged).

---

## Task 9: Version bump to 0.2.0

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Bump the workspace version**

Open `Cargo.toml` at the repo root. Change:

```toml
[workspace.package]
version = "0.1.6-b"
```

to:

```toml
[workspace.package]
version = "0.2.0"
```

- [ ] **Step 2: Update Cargo.lock**

```bash
cargo build --workspace
```
Expected: regenerates `Cargo.lock`, all builds succeed.

- [ ] **Step 3: Add a changelog entry**

Open `CHANGELOG.md`. Add a top entry:

```markdown
## v0.2.0

### Breaking / structural

- Extracted formatter internals into a new `kdlfmt-core` library crate.
  The `kdlfmt` CLI is now a thin wrapper; behavior, flags, config file,
  and `.kdlfmtignore` semantics are unchanged.
- `kdlfmt-core` exposes `format_document`, `format_check`, `parse_kdl`,
  `format_kdl`, `KdlFmtConfig`, `KdlVersion`, and `FmtError`. Downstream
  tools (e.g. `kdli`, the KDL LSP) can now depend on the formatter
  without depending on the CLI.
```

Note: if the project uses `npx auto-changelog` (per `mise.toml`), entries may be auto-generated. Keep the section header and let the generator fill commit details.

- [ ] **Step 4: Commit the bump**

```bash
git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "$(cat <<'EOF'
chore: bump version to 0.2.0

Ships the kdlfmt-core library crate extraction. CLI behavior is
unchanged; this is a structural/library API change, hence the minor bump.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: Final verification

- [ ] **Step 1: Lint clean**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```
Expected: both exit 0.

- [ ] **Step 2: Full test run (release)**

```bash
cargo test --release --workspace
```
Expected: all green.

- [ ] **Step 3: Verify the CLI binary still builds and runs against real files**

```bash
cargo build --release
./target/release/kdlfmt --version
./target/release/kdlfmt format --help
./target/release/kdlfmt check kdlfmt.kdl
```
Expected: version prints as `kdlfmt 0.2.0`; help unchanged; check exits clean.

- [ ] **Step 4: Verify kdlfmt-core publishes cleanly (dry-run)**

```bash
cargo publish -p kdlfmt-core --dry-run
```
Expected: no errors. If there are warnings about missing metadata, fix in a follow-up; dry-run passing is sufficient.

- [ ] **Step 5: Push**

```bash
git log --oneline -15
```
Expected: 7-10 new commits since `main` HEAD at plan start, with clear messages.

```bash
git push origin main
```

(Only if the user has authorized pushing to `main`. Otherwise, open a PR.)

---

## Self-review checklist (for the executing agent)

After completing all tasks, re-read this plan and verify:

- [ ] `kdlfmt-core/src/lib.rs` exports: `VERSION`, `KdlVersion`, `KdlFmtConfig`, `FmtError`, `format_document`, `format_check`, `FormatCheck`, `format_kdl`, `parse_kdl`.
- [ ] `cli/Cargo.toml` lists `kdlfmt-core` under `[dependencies]` with `features = ["clap"]`.
- [ ] `cli/src/config.rs`, `cli/src/kdl.rs`, `cli/src/edge_cases.rs` no longer exist.
- [ ] All imports in `cli/src/` that used to reference `crate::config`, `crate::kdl`, `crate::cli::KdlVersion` now reference `kdlfmt_core::…`.
- [ ] Workspace `version` is `0.2.0`.
- [ ] `cargo test --release --workspace` is green.
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` is clean.
- [ ] No `cli/tests/` file was modified.

If anything fails, the refactor is incomplete — fix before handing off.
