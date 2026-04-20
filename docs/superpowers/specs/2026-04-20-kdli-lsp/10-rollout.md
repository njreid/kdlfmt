# 10 — Rollout, versioning, non-goals

## Release sequence

1. **`kdlfmt-core` extraction lands → `kdlfmt 0.2.0` ships.**
   Minor bump; the CLI is unchanged but the workspace now has a library crate. Downstream (kdli) can depend on it via git path or a published crate.
2. **`ksl2rs` fork-pin bump → `ksl2rs 0.2.0` (or git-rev bump) ships.**
   Independent from #1. Coherence CI in both repos asserts both resolve to the same `kdl` rev.
3. **`kdli 0.1.0-alpha` tagged.**
   Usable from Helix and Neovim with manual config. No editor docs yet. Registry discovery works. Formatting works. Completion and hover work. Hot-reload works.
4. **`kdli 0.1.0` tagged.**
   Editor docs complete (Helix, Neovim). `cargo-dist` prebuilds available. README covers install, registry layout, pragma, basic troubleshooting. Post-install sanity check (`kdli --version` + `kdli` run with a trivial JSON-RPC init) documented.
5. **Post-v1 (no date commitment):**
   - Zed extension fleshed out and submitted to the Zed extension registry.
   - VSCode extension (separate repo) shipped to Open VSX + Marketplace.
   - Tier 3 nav features: `definition`/`references`/`rename`/`documentSymbol`/`foldingRange`/`selectionRange`.
   - Tier 4 polish: `inlayHint`/`semanticTokens`/`codeLens`/`signatureHelp`/`rangeFormatting`.
   - Workspace-scoped `kdl-lsp.kdl` config, if the sibling+registry model hits real ergonomic limits.

## Versioning policy

- **`kdlfmt`**: minor bumps on library API changes; patch bumps on bug fixes; major bumps only if CLI UX changes.
- **`kdli`**: pre-1.0 semver applies — minor bumps may change `kdli-core` public API freely. Post-1.0, we respect the core's public types.
- **`ksl2rs`**: follows its own schedule; kdli pins a known-good rev.

## Publishing

- `kdli-server` is the published binary as `kdli` on crates.io.
- `kdli-core` is a published library on crates.io (for downstream reuse by tooling that doesn't want the LSP).
- Prebuilt binaries via `cargo-dist`, pattern-matched from kdlfmt's existing `dist-workspace.toml` (which already emits per-platform binaries).
- `cargo install kdli` remains the source-install path.

## Full non-goals for v1

(Summarized in the umbrella; full enumeration lives here.)

### LSP features deferred

- `textDocument/rangeFormatting` — kdlfmt-core formats whole docs. Adding range-aware support requires formatter work, not LSP work.
- `textDocument/onTypeFormatting` — noisy for KDL's structure.
- `textDocument/semanticTokens` — relies on tree-sitter in most editors already.
- `textDocument/codeLens` — schema-in-use badges are a nice-to-have.
- `textDocument/signatureHelp` — KDL's positional args are weakly analogous to function sigs; value is unclear.
- `textDocument/inlayHint` — high value but out of MVP cut per Q6; promotion blocked on MVP shipping first.
- `textDocument/definition` / `references` / `rename` — Tier 3; valuable for `.ksl` authoring but not MVP-critical.
- `textDocument/documentSymbol` / `foldingRange` / `selectionRange` — Tier 3 polish.
- `textDocument/colorProvider` — no compelling KDL use case yet.
- `workspace/symbol` / `workspace/executeCommand` — no use case for v1.
- Call hierarchy / type hierarchy — no use case.

### KSLv2 features out of scope

- **CEL expression evaluation for `when=` guards.** Mirrors ksl2rs' explicit non-goal. Pragma still records presence; evaluation does not happen. Schemas that rely on `when=` for conditional structure will validate only the unconditional shape.
- **Cross-file schema imports.** Mirrors ksl2rs. Every schema file is self-contained in v1.

### Discovery features deferred

- **Glob negation** (`!~/.config/zellij/old/*`) in `match {}` — might return in v0.2 if the need arises.
- **Workspace-scoped `kdl-lsp.kdl` config file** for per-project discovery overrides. Included only if the sibling + registry model proves inadequate in practice.
- **Recursive `~/.config/kdl/registry/` subdirectories.** Registry is flat in v1.

### Packaging deferred

- **VSCode extension.** Separate repo, post-v1.
- **Zed extension content** (scaffold exists in `kdli/zed-extension/` as T4c, contents post-v1).
- **Tree-sitter grammars.** kdli does not own any; we reference `kslv2/tree-sitter-ksl/` and community `tree-sitter-kdl`.
- **Docker image.** Not needed.

## Success criteria for v1

- `kdli --version` prints on every supported platform.
- Helix + Neovim users can, by following the docs, get schema-aware diagnostics + hover + completion + formatting in < 5 minutes from a clean install.
- Editing a `.ksl` in registry triggers hot-reload on every currently-open bound `.kdl`.
- A corrupt schema does not block the editor — formatting still works, a cascade warning appears on bound `.kdl` files.
- All fixture-based tests green, all LSP integration tests green, fork-pin CI green.
- Performance ballparks (see [`09-testing.md`](./09-testing.md)) pass manual benchmarking.

## Open questions at spec time

None blocking. Items flagged for revisit:

- **`match` block child form** — whether patterns are node names (string-literal identifiers) or `pattern "glob"` children. Pinned during T0f interface-lock; this doc will be updated with the final choice.
- **Tree-sitter grammar sourcing for Helix** — we assume `tree-sitter-kdl` upstream is usable. If not, v1 docs just say "highlighting may be degraded until a grammar ships."
- **Zed monorepo friction** — if the Rust workspace and the Zed extension toolchains start fighting in CI, we'll split `zed-extension/` out. Decision deferred until the Zed milestone starts.
