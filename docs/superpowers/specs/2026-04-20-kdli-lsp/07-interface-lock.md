# 07 — Interface lock (task T0f)

The single highest-leverage deliverable in the plan. This file's signatures are frozen before Batch 1 kicks off; every Batch-1 implementer codes against them; Batch 2 can wire tower-lsp with confidence that the core's public shape is stable.

The actual Rust source for these lives in `kdli-core/src/interfaces.rs` and is re-exported from `kdli-core/src/lib.rs`. The version below is the spec; the source version is authoritative once it lands.

## Convention

- Every `pub` item here must appear in `interfaces.rs` with the same signature.
- Function bodies in `interfaces.rs` are `unimplemented!("T<task-id>")` pointing at the task that fills them.
- Downstream modules import types from `crate::interfaces` only. This gives Batch-1 implementers a single file to open when they start work.

## Errors

```rust
use miette::Diagnostic;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
pub enum Error {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),

    #[error("parse: {0}")]
    Parse(#[from] kdl::KdlError),

    #[error("schema compile failed")]
    SchemaCompile { diagnostics: Vec<ksl2rs::KslDiagnostic> },

    #[error("glob: {0}")]
    Glob(#[from] globset::Error),
}
```

## Filesystem snapshot

Behind a trait so discovery is unit-testable without tempdirs.

```rust
pub trait FsSnapshot: Send + Sync {
    fn read(&self, path: &Path) -> std::io::Result<String>;
    fn exists(&self, path: &Path) -> bool;
    fn read_dir(&self, path: &Path) -> std::io::Result<Vec<PathBuf>>;
    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf>;
}

pub struct RealFs;
impl FsSnapshot for RealFs { /* std::fs delegation */ }

// For tests: kdli_core::test_support::MemFs { files: HashMap<PathBuf, String> }.
```

## Discovery

```rust
pub struct Resolver {
    home: PathBuf,
    registry_dir: PathBuf,
    fs: Arc<dyn FsSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaResolution {
    Sibling { schema_path: PathBuf },
    Pragma { schema_name: String, schema_path: PathBuf },
    /// Pragma referenced a schema name that does not exist in the registry.
    /// Caller emits a diagnostic on the pragma line; no fall-through to registry matching.
    PragmaMissing { schema_name: String },
    RegistryUnique { schema_name: String, schema_path: PathBuf },
    RegistryAmbiguous { candidates: Vec<(String, PathBuf)> },
    None,
}

impl Resolver {
    pub fn new(home: PathBuf, registry_dir: PathBuf, fs: Arc<dyn FsSnapshot>) -> Self;

    /// Default constructor using `$HOME` and `$XDG_CONFIG_HOME/kdl/registry` (or fallback).
    pub fn from_env() -> Self;

    /// Resolve a .kdl file to its schema. `pragma_name` is extracted by the caller
    /// (kdli-server reads the first slashdash-node from the parsed document).
    pub fn resolve(
        &self,
        kdl_file: &Path,
        pragma_name: Option<&str>,
    ) -> SchemaResolution;
}
```

## Schema cache

```rust
pub struct SchemaCache {
    compiled: dashmap::DashMap<PathBuf, Arc<ksl2rs::CompiledSchema>>,
    bindings: dashmap::DashMap<PathBuf, Binding>,
    fs: Arc<dyn FsSnapshot>,
}

pub struct Binding {
    pub schema_path: Option<PathBuf>,
    pub resolution_kind: ResolutionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionKind { Sibling, Pragma, PragmaMissing, Registry, None, Ambiguous }

impl SchemaCache {
    pub fn new(fs: Arc<dyn FsSnapshot>) -> Self;

    pub fn get_or_compile(
        &self,
        path: &Path,
    ) -> Result<Arc<ksl2rs::CompiledSchema>, Vec<ksl2rs::KslDiagnostic>>;

    pub fn bind(&self, kdl_file: &Path, resolution: &SchemaResolution);

    pub fn binding_for(&self, kdl_file: &Path) -> Option<Binding>;

    pub fn evict_schema(&self, schema_path: &Path);

    /// Returns the list of .kdl URIs that are bound to this schema path.
    /// Used by the watcher on schema change to figure out who to revalidate.
    pub fn dependents_of(&self, schema_path: &Path) -> Vec<PathBuf>;
}
```

## Document store

```rust
pub struct DocumentStore { /* Rope + LineIndex per URI */ }

pub struct Document {
    pub uri: PathBuf,
    pub rope: ropey::Rope,
    pub line_index: LineIndex,
    pub parsed: Result<kdl::KdlDocument, kdl::KdlError>,
    pub version: i32,
}

impl DocumentStore {
    pub fn new() -> Self;
    pub fn open(&self, uri: PathBuf, text: String, version: i32);
    pub fn apply_change(&self, uri: &Path, changes: &[TextEdit], version: i32);
    pub fn get(&self, uri: &Path) -> Option<Arc<Document>>;
    pub fn close(&self, uri: &Path);
    pub fn all_uris(&self) -> Vec<PathBuf>;
}

pub struct LineIndex { /* offsets per line */ }
impl LineIndex {
    pub fn new(src: &str) -> Self;
    pub fn offset_to_position(&self, offset: usize) -> Position;
    pub fn position_to_offset(&self, pos: Position) -> usize;
    pub fn span_to_range(&self, span: miette::SourceSpan) -> Range;
}

pub struct Position { pub line: u32, pub character: u32 }
pub struct Range { pub start: Position, pub end: Position }
pub struct TextEdit { pub range: Range, pub new_text: String }
```

Note: `Position`, `Range`, `TextEdit` are kdli-core's own types, NOT `lsp_types`. `kdli-server` converts to/from `lsp_types` at the boundary. This is what keeps `kdli-core` independent of tower-lsp.

## Cursor path

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CursorContext {
    Empty,                                   // top of file, no node yet
    NodeNameSlot { parent: Vec<Segment> },   // inside a children block, typing node name
    PropNameSlot { parent: Vec<Segment>, node: String },
    PropValueSlot { parent: Vec<Segment>, node: String, prop: String },
    ArgSlot { parent: Vec<Segment>, node: String, arg_index: usize },
    RefValueSlot { parent: Vec<Segment>, node: String, prop: String },
    BlockBody { parent: Vec<Segment> },
    Whitespace,
}

pub struct Segment { pub node: String }

pub fn compute_context(doc: &Document, pos: Position) -> CursorContext;
```

This is **the** module everything else uses for analysis. Owning it as a single task (T1a) with tight tests is critical — everything downstream treats `CursorContext` as a reliable oracle.

## Completion

```rust
pub fn compute_completions(
    schema: &ksl2rs::CompiledSchema,
    doc: &Document,
    pos: Position,
) -> Vec<CompletionItem>;

pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionKind,
    pub detail: Option<String>,
    pub documentation_markdown: Option<String>,
    pub insert_text: String,
    pub is_snippet: bool,
    pub sort_text: Option<String>,
}

pub enum CompletionKind { Node, Property, EnumVariant, Keyword, Reference, Snippet }
```

## Hover

```rust
pub fn compute_hover(
    schema: &ksl2rs::CompiledSchema,
    doc: &Document,
    pos: Position,
) -> Option<Hover>;

pub struct Hover {
    pub markdown: String,
    pub range: Option<Range>,  // span of the symbol under the cursor
}
```

## Code actions

```rust
pub fn compute_code_actions(
    schema: Option<&ksl2rs::CompiledSchema>,
    doc: &Document,
    requested_range: Range,
    diagnostics_in_range: &[Diagnostic],
    registry_candidates: &[(String, PathBuf)],
) -> Vec<CodeAction>;

pub struct CodeAction {
    pub title: String,
    pub kind: CodeActionKind,
    pub edits: Vec<TextEdit>,
    pub triggers_diagnostic: Option<String>,  // DiagnosticCode this resolves
}

pub enum CodeActionKind { QuickFix }
```

## Diagnostic model

```rust
pub struct Diagnostic {
    pub range: Range,
    pub severity: Severity,
    pub code: Option<String>,
    pub source: &'static str,      // "kdli", "kdl", "ksl"
    pub message: String,
    pub related: Vec<RelatedInformation>,
}

pub enum Severity { Error, Warning, Information, Hint }

pub struct RelatedInformation {
    pub location: Location,
    pub message: String,
}

pub struct Location { pub uri: PathBuf, pub range: Range }

pub trait DiagnosticMapper {
    fn from_kdl_error(e: &kdl::KdlError, line_index: &LineIndex) -> Vec<Diagnostic>;
    fn from_ksl_diagnostic(d: &ksl2rs::KslDiagnostic, line_index: &LineIndex) -> Diagnostic;
}
```

## Review protocol for T0f

T0f's output (the actual Rust source of `interfaces.rs`) is reviewed by the user directly, not by another agent. This is called out explicitly in [`08-swarm-execution.md`](./08-swarm-execution.md). The reason: bugs in the interface lock don't surface until Batch 1 integration, which is the most expensive moment to discover them.
