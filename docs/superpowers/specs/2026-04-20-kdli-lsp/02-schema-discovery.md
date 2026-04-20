# 02 — Schema discovery

When a `.kdl` file opens in the editor, `kdli` needs to pick a schema. The model is filesystem-first: sibling file, then pragma, then a user-owned registry. No workspace config file in v1.

## Precedence (locked)

Evaluate in order; the first match wins.

1. **Sibling file.** In the same directory as `foo.kdl`, look for `foo.ksl`. If present, use it.
2. **Pragma.** First node in `foo.kdl` is a slashdash-commented `/- ksl-schema <name>`. `<name>` must resolve to `~/.config/kdl/registry/<name>.ksl`. If the file is missing, emit a diagnostic on the pragma line — do not fall through.
3. **Registry unique match.** Compile every `<name>.ksl` in the registry, extract its top-level `match {}` globs, test the open file's absolute path against each globset. If exactly one matches, use it.
4. **Registry ambiguous match.** More than one schema matches. Emit an error diagnostic at `(line 0, col 0)` of the open file with message `"multiple schemas match this file: a, b, c. Add '/- ksl-schema <name>' at the top to pick one."` Offer one code action per candidate — see [`04-lsp-capabilities.md`](./04-lsp-capabilities.md).
5. **No match.** No schema bound. Formatting + KDL-level syntax diagnostics still work; validation is skipped silently.

## Pragma format

```kdl
/- ksl-schema zellij

other content…
```

Rules:

- Must be the **first node** in the document (after optional whitespace / line comments). Not the first *line* — KDL comments and blanks above are allowed.
- Exactly one positional arg (the schema name). No props, no children.
- `<name>` matches `[a-zA-Z0-9_-]+`. Anything else is a diagnostic on the arg.
- The slashdash prefix (`/-`) makes the node a no-op for any KDL consumer that doesn't know about the pragma. This is the whole reason the pragma is shaped this way.
- Resolution: join `<registry_dir>/<name>.ksl`; must exist and pass compilation. If not, diagnostic on the pragma.

## Registry layout

```
~/.config/kdl/registry/
├── zellij.ksl
├── zellij-layout.ksl
├── cargo.ksl
└── myproject.ksl
```

`registry_dir` is `$XDG_CONFIG_HOME/kdl/registry` if set, else `~/.config/kdl/registry`. Not recursive — schemas live directly in this dir. Non-`.ksl` files ignored.

A registry `.ksl` file has the shape:

```kdl
// Optional top-level match block. If absent, this schema
// participates in registry matching only through the pragma.
match {
    "~/.config/zellij/**/*.kdl"
    "$XDG_CONFIG_HOME/zellij/**/*.kdl"
}

schema "https://example.com/zellij" version="0.1.0" {
    define #color { /* … */ }
    document { /* … */ }
}
```

The `match` node:

- Is at the top of the file (first KDL node). Enforced by the meta-schema + a dedicated discovery-time check.
- Has zero or more children. Each child's node name is treated as a literal glob pattern (KDL allows unquoted identifiers with most punctuation; if ambiguous, users quote the string).
- Actually — for unambiguous parsing, **each child is a string arg under an unnamed-style node or single-node `pattern "…"`** — we'll lock the exact form in T0f. Working assumption: children are nodes whose **name is the glob pattern as a string**:

  ```kdl
  match {
      "~/.config/zellij/**/*.kdl"
      "$XDG_CONFIG_HOME/zellij/**/*.kdl"
  }
  ```

  That parses as a `match` node whose children are nodes named by string literals. If this turns out awkward, the alternative is `pattern "glob"` child nodes — decide during interface-lock, document the final choice here.

- Empty `match {}` is legal and means "never match anything automatically" (schema is pragma-only).
- Absent `match {}` is equivalent to empty.

## Glob semantics

- Syntax: `globset`-flavored (gitignore-adjacent): `**` = recursive, `*` = single segment wildcard, `?` = single char, `[abc]` = character class.
- Anchored to absolute paths. Patterns must start with `/`, `~`, `$VAR`, or `$ENV{…}`.
- Expansion applied at schema compile time:
  - `~` → user home (via `shellexpand::tilde`).
  - `$VAR` and `${VAR}` → env vars (via `shellexpand::env`). Missing vars are a warning, and that pattern is skipped.
- Relative patterns rejected with a diagnostic on the pattern.
- No negation in v1 (explicit non-goal; see [`10-rollout.md`](./10-rollout.md)).

## Path normalization

Both the glob's expanded path and the open file's path are canonicalized (symlinks resolved, `..` collapsed) before matching. Prevents false negatives when the user opens a file via a symlinked path.

## `.ksl` files open in the editor

When the open document is itself a `.ksl`, the Resolver is bypassed — the effective schema is the bundled meta-schema (see [`05-meta-schema.md`](./05-meta-schema.md)). The `.ksl`'s own `match {}` block is parsed for diagnostics but not used for self-discovery.

## Error cases the Resolver must report

| Case | Diagnostic location | Message |
|------|---------------------|---------|
| Pragma references non-existent schema | pragma arg | `schema "<name>" not found in <registry_dir>` |
| Pragma schema fails to compile | pragma arg, plus cascade to target file | `schema "<name>.ksl" has errors; see file` |
| Ambiguous registry match | line 0 col 0 | `multiple schemas match: a, b. Add '/- ksl-schema <name>' to pick one.` |
| Registry `.ksl` glob missing leading anchor | the pattern node | `pattern "<glob>" must start with /, ~, or $VAR` |
| Registry `.ksl` glob invalid | the pattern node | propagated from `globset::Error` |
