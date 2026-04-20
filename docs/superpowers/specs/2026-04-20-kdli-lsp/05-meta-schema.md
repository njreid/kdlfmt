# 05 — Meta-schema for `.ksl` files

A `.ksl` file is a KDL document. The exact same LSP pipeline works for it; we just need a schema that describes KSLv2's surface. We get this in two layers.

## Layer A — well-formedness (free, already exists in ksl2rs)

When the open document is a `.ksl`, compile it with `ksl2rs::compile_schema_str` and then run `well_formedness::check`. Surface both the compile-time diagnostics and the well-formedness diagnostics on the `.ksl` itself.

This alone catches:

- Duplicate `define` names.
- Unresolved `ref=#name`.
- `ref` cycles.
- Contradictory `open`/`closed` on the same `children` block.
- Invalid occurrence bounds.
- Type/constraint mismatches (`enum` on a non-string, `between` on a string, etc.).
- Choice-ambiguity problems (see `kslv2/CHOICE_AMBIGUITY.md`).

ksl2rs already implements this; we just wire the existing `Vec<KslDiagnostic>` through the same `DiagnosticMapper` used for instance validation.

## Layer B — bundled meta-schema

A `.ksl` file shipped inside `kdli-core/assets/ksl-meta.ksl`, describing the KSLv2 surface language so completion and hover work on `.ksl` files too.

Coverage targets:

- Top-level: `schema "<id>" version=…{ … }`, `define #<name> { … }`, `match { … }`, `document { … }`.
- Subject nodes: `node`, `prop`, `arg`, `children` with their cardinality keywords (`required`, `optional`, `many`, `one-of-many`).
- Constraint vocabulary: `type`, `enum`, `const`, `between`, `pattern`, `format`, `ref`.
- Conditional: `when="<expr>"` as an **opaque string prop** (see scope limit).
- Composition: `all-of`, `any-of`, `one-of`, `not`.
- Openness: `open`, `closed`.
- `doc:*` annotation namespace: `doc:summary`, `doc:description`, `doc:example`, `doc:deprecated`, `doc:see-also`.
- `match` children are glob strings (see [`02-schema-discovery.md`](./02-schema-discovery.md)).

## Bundling

- Lives at `kdli-core/assets/ksl-meta.ksl`. Static file, version-controlled.
- Loaded via `include_str!("assets/ksl-meta.ksl")` at build time.
- Compiled lazily on first `.ksl` open, cached in a `OnceLock<Arc<CompiledSchema>>`.
- Validated in CI: `cargo test -p kdli-core meta_schema::compiles_cleanly` asserts no well-formedness errors.

## Pipeline for `.ksl` documents

Bypass the `Resolver`. Binding is implicit: every `.ksl` URI binds to the meta-schema `Arc`. The cache never evicts this entry.

```
didOpen(uri.ksl)
  ↓
parse KDL          → parse diagnostics if bad
  ↓
compile as schema  → compile-time diagnostics (Layer A part 1)
  ↓
well-formedness    → well-formedness diagnostics (Layer A part 2)
  ↓
validate against
bundled meta       → structural/shape diagnostics (Layer B)
  ↓
publish union
```

Diagnostics from the three layers are merged and published in one stream. Severity and message text make the source obvious; `code` field disambiguates if needed (`ksl/wf/…` vs `ksl/meta/…`).

## Scope limits

- **`when=` expressions are opaque.** The meta-schema models `when` as a required string prop when present; it does **not** model the CEL-like expression syntax inside the string. Parsing CEL is out of scope (mirrors ksl2rs' non-goal). If authors want CEL diagnostics later, that's a future layer (probably handled by embedding a CEL parser, not extending the meta-schema).
- **No cross-file import modeling.** KSLv2 (per ksl2rs non-goals) doesn't do cross-file imports in this release. The meta-schema assumes single-file scope.
- **No `define` cross-references beyond what ksl2rs already checks.** Well-formedness already handles `ref`/`define` resolution and cycle detection; the meta-schema doesn't duplicate that work.

## Completion / hover behavior on `.ksl`

Identical code paths as for `.kdl` instance files — the only difference is which `Arc<CompiledSchema>` gets passed in. Authors of `.ksl` files get:

- Completion for `schema`, `define`, `match`, `document`, subject keywords, constraint keywords, composition ops, `doc:*` keys.
- Hover with descriptions sourced from `doc:*` annotations in the bundled meta-schema.
- Ambiguity errors where two subjects could match the same position (caught by well-formedness, not meta).
