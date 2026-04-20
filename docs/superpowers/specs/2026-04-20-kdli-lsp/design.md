# kdli — KDL Language Server (design umbrella)

**Date:** 2026-04-20
**Status:** Draft — awaiting user review
**Scope:** Evolve this workspace's formatter + the `ksl2rs` validator into a full LSP for KDL documents, with KSLv2 schema-aware diagnostics, completion, hover, and formatting.

## Summary

Today `kdlfmt` is a thin CLI over a forked `kdl-rs`. `ksl2rs` is a working KSLv2 compiler + validator with miette-backed diagnostics and a `CompiledSchema` type that is `Send + Sync` and designed for compile-once / validate-many. Neither speaks LSP.

This design introduces a new binary, **`kdli`**, living in its own repo, that composes the two: formatting via `kdlfmt-core` (extracted from this repo as a library crate), validation via `ksl2rs`, and a schema discovery layer driven by a user-owned registry at `~/.config/kdl/registry/`. `kdli-core` contains all analysis as pure functions; `kdli-server` contains the tower-lsp glue, file watching, and JSON-RPC surface.

The discovery model is filesystem-first: a sibling `<name>.ksl` wins; otherwise a `/- ksl-schema <name>` pragma wins; otherwise the LSP scans registry `.ksl` files whose top-level `match {}` block globs the current path. Multiple registry matches surface as a diagnostic with a "pick schema" code action that writes the pragma.

## Goals

- Format-on-save and on-demand formatting through LSP, backed by the same formatter the CLI uses.
- KDL syntax diagnostics plus KSLv2 schema diagnostics in the same stream, with precise source spans.
- Schema-aware completion, hover, and code actions for `.kdl` instance files.
- Authoring support for `.ksl` schemas themselves (meta-schema + well-formedness).
- Hot-reload when a schema changes on disk.
- Work in Helix and Neovim at v1; Zed deferred but housed in-repo; VSCode in a separate repo, deferred.
- Ship work as a swarm of parallel subagents with minimal merge pain.

## Non-goals (v1)

Brief list; full list lives in [`10-rollout.md`](./10-rollout.md).

- `rangeFormatting`, `onTypeFormatting`, `semanticTokens`, `codeLens`, `signatureHelp`, `inlayHint` (Tier 4 — deferred).
- `definition`/`references`/`rename`/`documentSymbol`/`foldingRange`/`selectionRange` (Tier 3 — deferred).
- CEL evaluation for `when=` guards.
- Cross-file schema imports.
- Glob negation in `match {}`.
- Tree-sitter grammars shipped from kdli.
- VSCode extension (separate repo, later milestone).
- Workspace-scoped `kdl-lsp.kdl` config file.

## Table of contents

1. [Repo & crate layout](./01-architecture.md)
2. [Schema discovery](./02-schema-discovery.md)
3. [Schema cache & hot-reload](./03-schema-cache.md)
4. [LSP capabilities (MVP)](./04-lsp-capabilities.md)
5. [Meta-schema for `.ksl`](./05-meta-schema.md)
6. [Editor integrations](./06-editor-integrations.md)
7. [Interface-lock (T0f)](./07-interface-lock.md)
8. [Swarm execution plan](./08-swarm-execution.md)
9. [Testing strategy](./09-testing.md)
10. [Rollout, versioning, non-goals](./10-rollout.md)

## Related projects

- `kdlfmt` (this repo) — becomes `kdlfmt-core` (library) + `cli` (bin).
- [`kslv2`](../../../../../kslv2/) — KSLv2 specification and tree-sitter grammar for `.ksl`.
- [`ksl2rs`](../../../../../ksl2rs/) — KSLv2 compiler + validator. `CompiledSchema` is the canonical schema handle kdli holds.
- [`kdl-rs` fork](https://github.com/njreid/kdl-rs) pinned at rev `05521dc` — v1 support + expression-strings (backtick `when=` guards). Required by kdli and must be propagated into `ksl2rs` (task T0g).

## Approval gate

This umbrella is part of the spec; review the linked sections, leave comments or ask for revisions, and mark approved. After approval, the next step is `superpowers:writing-plans` to generate the implementation plan that feeds the swarm.
