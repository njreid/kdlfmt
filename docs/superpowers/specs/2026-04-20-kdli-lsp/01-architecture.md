# 01 — Architecture: repos, crates, and the kdlfmt refactor

## This repo (kdlfmt) after the refactor

Today `kdlfmt` is a single-crate workspace with just `cli/`. We extract the formatter and config layer into a sibling library crate so `kdli-core` can depend on it:

```
kdlfmt/
├── Cargo.toml                       # workspace root
├── kdlfmt-core/                     # NEW — library crate
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── format.rs                # format_document / format_check
│       ├── config.rs                # KdlFmtConfig (indent_size, use_tabs, …)
│       └── editorconfig.rs          # ec4rs integration
└── cli/                             # existing bin crate; depends on kdlfmt-core
    ├── Cargo.toml                   # no longer owns kdl/miette directly — via core
    └── src/
        ├── main.rs                  # clap dispatch (unchanged)
        └── commands/…                # rayon walking, ignore, stdin — unchanged
```

**What moves into `kdlfmt-core`:**

- The actual formatter call (currently a thin wrapper over `kdl-rs`'s `KdlDocument::autoformat`).
- `KdlFmtConfig` struct + loader.
- `.editorconfig` resolution (via `ec4rs`).
- KDL version detection / dual-version parsing logic.

**What stays in `cli`:**

- clap argument parsing (`format`, `check`, `init`, `completions` subcommands).
- File walking via `ignore` (gitignore semantics).
- `.kdlfmtignore` handling.
- Parallelism via `rayon`.
- stdin/stdout streaming.
- Exit code conventions.
- Logging (`env_logger`).

**Public API of `kdlfmt-core` (locked in T0a):**

```rust
pub struct KdlFmtConfig { /* indent_size, use_tabs, … */ }
pub enum KdlVersion { V1, V2, Auto }
pub enum FmtError { Parse(kdl::KdlError), Io(std::io::Error) }

pub fn format_document(
    source: &str,
    version: KdlVersion,
    cfg: &KdlFmtConfig,
) -> Result<String, FmtError>;

pub fn format_check(
    source: &str,
    version: KdlVersion,
    cfg: &KdlFmtConfig,
) -> Result<FormatCheck, FmtError>;

pub struct FormatCheck { pub was_formatted: bool, pub would_write: String }

pub fn load_config(path: Option<&Path>) -> Result<KdlFmtConfig, FmtError>;
pub fn resolve_with_editorconfig(path: &Path, base: &KdlFmtConfig) -> KdlFmtConfig;
```

**Compat guarantee.** The CLI behavior, flags, default config values, `.kdlfmtignore` semantics, exit codes, and log output must be byte-for-byte unchanged after the refactor. Existing `assert_cmd` + `predicates` tests in `cli/tests/` must pass with zero modifications. Anything that diverges is a regression, not a refactor.

## The new repo (kdli)

Path: `../kdli` (sibling to `kdlfmt`, `kslv2`, `ksl2rs` in the user's `~/code/` layout).

```
kdli/
├── Cargo.toml                          # workspace root
├── README.md
├── .github/workflows/ci.yml            # build, test, clippy, fork-pin check
├── docs/editors/                       # editor setup snippets (see 06)
├── kdli-core/
│   ├── Cargo.toml
│   ├── assets/
│   │   └── ksl-meta.ksl                # bundled meta-schema (see 05)
│   ├── src/
│   │   ├── lib.rs
│   │   ├── interfaces.rs               # T0f — signature-only public API
│   │   ├── discovery.rs                # Resolver, SchemaResolution
│   │   ├── cursor.rs                   # (uri, position) → CursorPath
│   │   ├── completion.rs
│   │   ├── hover.rs
│   │   ├── code_action.rs
│   │   └── diagnostics.rs              # miette → internal Diagnostic
│   └── tests/
│       ├── discovery.rs
│       ├── completion.rs
│       ├── hover.rs
│       ├── code_action.rs
│       └── golden/…                     # insta fixtures
└── kdli-server/
    ├── Cargo.toml
    └── src/
        ├── main.rs                     # tower-lsp entry
        ├── handler.rs                  # LanguageServer impl
        ├── documents.rs                # DocumentStore (ropey)
        ├── cache.rs                    # SchemaCache
        ├── watcher.rs                  # notify integration
        ├── pipeline.rs                 # open → parse → validate → publish
        └── format.rs                   # delegates to kdlfmt-core
```

## Dependency graph

```
kdli-server  ──► kdli-core  ──► kdlfmt-core (git path dep on this repo)
    │                │
    │                ├────────► ksl2rs   (git dep — must carry same kdl-rs fork pin)
    │                │
    │                └────────► kdl       (njreid/kdl-rs fork, v1 + expression-strings)
    │
    ├── tower-lsp
    ├── tokio (rt + macros + io-std)
    ├── notify
    ├── globset
    ├── shellexpand
    ├── ropey
    └── miette
```

## Fork-pin coherence

Both `kdli-core` and `ksl2rs` must resolve `kdl` to the **same** `njreid/kdl-rs` rev, because `CompiledSchema` borrows spans from the parsed schema document and the types would not match across two different `kdl` crate versions.

**Task T0g** (owned by you, trivial): update `ksl2rs/Cargo.toml` to point `kdl` at the fork, run `cargo test`, push. Ship as `ksl2rs 0.2.0` or a git-revision bump.

**CI tripwire (T0b):** both repos add a `cargo tree -p kdl --format '{p} {r}'` check that asserts the rev matches an expected constant. Drift fails CI loudly, before anyone wastes time on a confusing type error.

## Why separate `kdli-core` from `kdli-server`

A split core/server layout costs one extra `Cargo.toml` and buys three things:

1. **Testability.** Every analysis function is a pure `(input) → output`. No tower-lsp fixtures, no async runtime, no JSON-RPC. `cargo test -p kdli-core` runs in milliseconds.
2. **Swarm-friendliness.** Batch 1 of the swarm (completion, hover, code actions) writes only inside `kdli-core/`. Batch 2 touches only `kdli-server/`. File-level isolation prevents merge conflicts by construction (see [`08-swarm-execution.md`](./08-swarm-execution.md)).
3. **Reuse.** If anyone later wants `kdli` analysis inside a CLI, a docs generator, or a pre-commit hook, they depend on the core — no LSP baggage.

## Binary & install story

`kdli` is a single binary. Invoked with no args, it runs as an LSP (stdio JSON-RPC). `kdli --version` for sanity checks. No other subcommands in v1.

Distribution mirrors kdlfmt: `cargo install kdli` from crates.io, plus `cargo-dist` prebuilt binaries per platform. The `dist-workspace.toml` recipe kdlfmt already uses is copied verbatim.
