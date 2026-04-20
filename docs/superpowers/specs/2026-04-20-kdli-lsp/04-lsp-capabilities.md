# 04 — LSP capabilities (MVP = Tier 1 + 2)

Each capability is a pure computation in `kdli-core` and a protocol wrapper in `kdli-server`. The server does no analysis; the core does no I/O.

## Tier 1 — foundational

### Document sync

- `textDocument/didOpen` / `didChange` / `didClose` / `didSave`.
- Storage: `DocumentStore { docs: DashMap<Url, Document> }`. Each `Document` wraps a `ropey::Rope` plus a cached `LineIndex` (offset ↔ line/col).
- `didChange`: apply TextDocumentContentChangeEvents in order. Re-derive `LineIndex`. Reparse (full parse — no incremental in v1).
- Parse result cached per document as `Result<KdlDocument, KdlError>` alongside the rope.

### `publishDiagnostics`

Pipeline per document (runs on `didOpen` and after every `didChange`):

```
1. Parse the rope with kdl::KdlDocument::parse_v{1,2}() or parse_any per config.
2. If KdlError → emit diagnostic(s) from the error's labels, return.
3. Read the first node; if it is "/- ksl-schema <name>", capture <name>.
4. Resolver.resolve(uri, pragma_name) → SchemaResolution.
5. Match resolution:
   - Sibling | Pragma | RegistryUnique: get_or_compile → validate → emit.
   - PragmaMissing:                     emit diagnostic on the pragma arg
                                        ("schema <name> not found in <registry_dir>"); no validation.
   - RegistryAmbiguous:                 emit ambiguity diagnostic at (0,0).
   - None:                              emit only parse diagnostics (already empty).
6. For the bound schema: run validator::validate; convert each KslDiagnostic
   via DiagnosticMapper → lsp_types::Diagnostic.
7. Send textDocument/publishDiagnostics with the full list (LSP semantics:
   a publish replaces all previous diagnostics for the URI).
```

Diagnostics carry:
- `range` — from miette `SourceSpan` → `Range` via `LineIndex`.
- `severity` — from `KslDiagnostic::severity`.
- `code` — the `DiagnosticCode` enum's string form (e.g. `"KSL0042"` or `"kdl/syntax"`).
- `source` — `"kdli"` for all kdli-emitted, `"kdl"` for raw parse errors from `kdl-rs`.
- `relatedInformation` — for violations that reference the schema, include a link to the schema source span (turns into "go to definition" in editors).

### `textDocument/formatting`

- Load resolved config: `kdlfmt_core::load_config(…)` combined with `resolve_with_editorconfig(uri, cfg)`. Cache per-URI, invalidate on `workspace/didChangeConfiguration` or `.editorconfig` change.
- Call `kdlfmt_core::format_document(rope.to_string(), version, &cfg)`.
- Return a single `TextEdit` covering the full document range. The editor diffs and applies.

Version detection: if the document's resolved KDL version (from config or heuristic) parses cleanly, format with it; otherwise fall back to `Auto`. Mirrors existing CLI behavior.

### `textDocument/rangeFormatting`

Deferred to Tier 4 — kdlfmt-core currently formats whole documents only. Adding range-aware formatting requires either (a) formatting the whole doc and diffing to the range, or (b) teaching `kdl-rs`'s formatter to handle subtrees. Out of scope for MVP.

### `workspace/didChangeWatchedFiles`

Declared in server capabilities so editors (VSCode especially) propagate fs events even if the server isn't watching. `notify` still runs as the primary watcher; this is a supplementary channel.

## Tier 2 — schema-aware authoring

### `textDocument/completion`

Triggered by: `{` `}`, whitespace after a node name, `=` after a prop name, `#` for refs/keywords, and explicit invocation.

Input: `(CompiledSchema, Document, Position)`.

Pipeline:
1. `cursor::compute_path(doc, pos)` → `CursorPath` (see [`07-interface-lock.md`](./07-interface-lock.md)).
2. Match the `CursorPath` context:

| Context | Items emitted |
|---------|---------------|
| At child-insertion position inside a node with `children { … }` | One `CompletionItem` per allowed child node subject. `kind = Class`. `label = node_name`. `insertText` = `node_name $1` if it has required args, else `node_name`. Snippets preferred. |
| After a node name, before `{` or newline | One item per allowed prop. Required props first (annotated `(required)`). `insertText = "<name>=$0"`. |
| Inside a prop value whose subject has `enum a b c` | One item per enum variant. `kind = EnumMember`. |
| After `ref=#` | One item per key in `schema.definitions`. `kind = Reference`. |
| After `#` at a value position | `#true`, `#false`, `#null`. `kind = Keyword`. |
| At top of a `.ksl` file (empty doc) | `schema $1`, `define #$1`, `match { $0 }` snippets. |
| After `doc:` annotation prefix | `doc:summary`, `doc:description`, `doc:example`, `doc:deprecated`. |

Each `CompletionItem` carries:
- `detail` — one-line type/constraint summary: `integer 1..65535`, `string enum { tcp, unix }`, etc.
- `documentation` — markdown block: `doc:summary` header + `doc:description` body.
- `sortText` — required items sort before optional; within groups, alpha.

### `textDocument/hover`

Input: `(CompiledSchema, Document, Position)`.

Pipeline:
1. `cursor::compute_path(doc, pos)` → `CursorPath`.
2. Resolve the path to a schema subject (node / prop / arg / child block).
3. Render a markdown block:

```
### `port` · integer
Range: 1..65535
Required when `props.mode == "tcp"`

**Summary:** listening TCP port for the service.

**Description:**
The port the service will bind to when running in TCP mode.
…
```

Hover returns `None` when the cursor is in whitespace or outside any schema-known subject.

### `textDocument/codeAction`

MVP actions:

- **`"pick schema: <name>"`** — produced when the diagnostic at `(0,0)` is `registry-ambiguous`. One action per candidate. Writes `/- ksl-schema <name>\n` at offset `0` via a `WorkspaceEdit`. Groups under `CodeActionKind::QuickFix`.
- **`"add required prop <name>"`** — produced when a `missing-required-prop` diagnostic is present. Inserts `<name>=` at the end of the node declaration line.
- **`"add required child node <name>"`** — for `missing-required-child`. Inserts `    <name>\n` inside the children block.

Actions are computed on demand from the current diagnostic set; no proactive code-action scanning.

## Deferred capabilities (Tier 3 / Tier 4)

Listed for traceability. Stub modules exist in `kdli-core` from day 1 so adding them later is additive, not restructuring.

- Tier 3: `definition`, `references`, `rename`, `documentSymbol`, `foldingRange`, `selectionRange`.
- Tier 4: `inlayHint`, `semanticTokens`, `codeLens`, `signatureHelp`, `rangeFormatting`.

See [`10-rollout.md`](./10-rollout.md) for the full deferred list and when each is revisited.

## Capability negotiation

`kdli-server` advertises in `initialize`:

```jsonc
{
  "capabilities": {
    "textDocumentSync": { "openClose": true, "change": 2 /* Incremental */ },
    "diagnosticProvider": { "interFileDependencies": true, "workspaceDiagnostics": false },
    "documentFormattingProvider": true,
    "completionProvider": {
      "triggerCharacters": ["{", " ", "=", "#", "\"", "\n"],
      "resolveProvider": false
    },
    "hoverProvider": true,
    "codeActionProvider": {
      "codeActionKinds": ["quickfix"],
      "resolveProvider": false
    },
    "workspace": {
      "fileOperations": {
        "didChangeWatchedFiles": { "dynamicRegistration": true }
      }
    }
  }
}
```
