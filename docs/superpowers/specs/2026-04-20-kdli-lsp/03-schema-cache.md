# 03 — Schema cache & hot-reload

The LSP holds at most one `Arc<CompiledSchema>` per distinct schema file on disk. All open documents that resolve to the same schema share that Arc. Hot-reload evicts and rebuilds on any schema change.

## Structures

```rust
pub struct SchemaCache {
    // Compiled schemas keyed by canonicalized absolute path.
    compiled: DashMap<PathBuf, Arc<CompiledSchema>>,

    // For invalidation: which .kdl files currently bind to which schema path.
    bindings: DashMap<PathBuf, Binding>,
}

pub struct Binding {
    pub schema_path: Option<PathBuf>,   // None = no-match / format-only
    pub resolution_kind: ResolutionKind,
    pub last_resolved_at: Instant,
}

pub enum ResolutionKind { Sibling, Pragma, Registry, None, Ambiguous }
```

- `compiled` is a global cache. First use on a path compiles; subsequent uses clone the `Arc`.
- `bindings` is a per-open-document map from `.kdl` URI (as a canonical `PathBuf`) to the schema file it depends on. Used for reverse lookups on invalidation.

## Compile-on-demand

```rust
impl SchemaCache {
    fn get_or_compile(&self, path: &Path) -> Result<Arc<CompiledSchema>, Vec<KslDiagnostic>>;
}
```

- Reads the file, calls `ksl2rs::compile_schema_str(src, path.to_string_lossy())`.
- Runs `well_formedness::check`; any `Severity::Error` means we cache **nothing** (so next access retries) and return the diagnostics.
- On success, stores `Arc::new(CompiledSchema)` and returns it.

Concurrency: `DashMap` handles the cross-thread case. On a race, two threads may compile the same schema once each — acceptable (schemas are small, compilation is fast, and losers drop their `Arc` when they see a cached entry).

## Hot-reload triggers

Handled by `kdli-server/src/watcher.rs`. The watcher uses `notify` in recursive mode over two roots:

1. `registry_dir` (e.g. `~/.config/kdl/registry/`) — recursive.
2. The dirs containing currently-open `.kdl` files — non-recursive (for sibling `.ksl` detection). Added/removed dynamically as documents open/close.

On any event:

| Event | Action |
|-------|--------|
| `.ksl` modified in registry or sibling dir | Evict `compiled[<path>]`. Find all `bindings[k] = { schema_path: <path> }`. For each `k`: re-resolve (name/pragma may be unchanged, but schema is stale), re-compile, re-validate `k`, publish diagnostics. |
| `.ksl` renamed / moved in registry | Evict old path. Re-resolve every open `.kdl` whose resolution was `Registry` — the rename may have changed the winning match. |
| `.ksl` deleted | Evict. Re-resolve every open `.kdl` that bound to it. Binding may become `None` or `Ambiguous`. |
| `.ksl` created in registry | Re-resolve every open `.kdl` whose binding was `None` or `RegistryUnique` — the new schema might match or create ambiguity. |
| Sibling `.kdl` → `.ksl` created/deleted | Re-resolve only the specific `.kdl` whose sibling directory changed. |

The watcher debounces: coalesce events in a 100ms window to avoid thrashing on save-as-I-type or editor autosave bursts.

## `.ksl` compile failure → cascade diagnostic on `.kdl`

If a bound schema fails to compile or fails well-formedness:

1. Publish the compiler/well-formedness diagnostics on the `.ksl` file itself. These are the root-cause errors; the user fixes them here.
2. For every `.kdl` bound to this schema, publish a **single** diagnostic at `(0, 0)` with severity `Warning`:

   ```
   schema "zellij.ksl" has errors; validation for this file is paused until it is fixed.
   ```

Schema validation is skipped for the `.kdl` while the schema is broken. Formatting still runs.

## Invalidation on discovery-result change, not only on file change

When we re-resolve and the *kind* of match changes (e.g. was `RegistryUnique`, now `Ambiguous` because a second schema appeared), the binding is updated and a diagnostic is re-published even if no `.kdl` content changed. The watcher is the trigger; the Resolver is re-run to detect the shift.

## Lifecycle boundaries

- On `initialize`: no compilation — the cache is lazy.
- On `textDocument/didOpen`: resolve → bind → compile-if-needed → validate → publish.
- On `textDocument/didChange`: reparse → (re-read pragma, since it may have changed) → if pragma changed, re-resolve; else reuse binding → validate → publish.
- On `textDocument/didClose`: remove binding from `bindings`. Do **not** evict from `compiled` — other open docs may still depend on it. If no bindings reference a compiled schema, leave it cached; schema cache is bounded by the number of registry files + open siblings, which is small.
- On `shutdown`: drop everything.

## Concurrency model

- `SchemaCache` is `Send + Sync` and lives in an `Arc` inside the tower-lsp `Backend`.
- All cache reads are lock-free (`DashMap` shards).
- Compilation happens on whichever task triggered it; there is no compile thread. Validation blocks briefly on `get_or_compile` the first time a schema is seen — acceptable for a single ~ms compile. If this bites in practice, move compilation to `tokio::task::spawn_blocking`.
