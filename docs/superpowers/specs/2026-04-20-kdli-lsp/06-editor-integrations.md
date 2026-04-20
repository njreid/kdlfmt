# 06 — Editor integrations

`kdli` is a stdio LSP binary. Editor integration for the vast majority of editors is just "install `kdli` on PATH and tell the editor to spawn it for `.kdl` and `.ksl` files." No per-editor plugin work required for v1 except Zed (deferred) and VSCode (separate repo, deferred).

## Launch model

- `kdli` with no arguments runs as an LSP over stdio.
- `kdli --version` prints version and exits 0.
- No other subcommands in v1. Formatting-from-CLI stays with `kdlfmt`.

## Helix (v1 milestone T4a)

Deliverable: `docs/editors/helix.md` with setup instructions.

```toml
# ~/.config/helix/languages.toml

[[language]]
name = "kdl"
scope = "source.kdl"
file-types = ["kdl"]
language-servers = ["kdli"]
indent = { tab-width = 4, unit = "    " }

[[language]]
name = "ksl"
scope = "source.ksl"
file-types = ["ksl"]
language-servers = ["kdli"]
indent = { tab-width = 4, unit = "    " }

[language-server.kdli]
command = "kdli"
```

Tree-sitter grammar notes:
- Plain `.kdl` highlighting: point users at an existing community grammar (`tree-sitter-kdl`). `kdli` does not ship one.
- `.ksl` highlighting: point users at the grammar already in `kslv2/tree-sitter-ksl/`. Helix installs tree-sitter grammars via a separate step; docs include the `hx --grammar fetch` / `hx --grammar build` commands and the `languages.toml` grammar entry.

## Neovim (v1 milestone T4b)

Deliverable: `docs/editors/neovim.md` with a Lua setup snippet using `nvim-lspconfig` (or its post-`:LspConfig` successor as of the Nvim 0.11+ built-in LSP API).

```lua
-- ~/.config/nvim/lua/plugins/kdli.lua
vim.filetype.add({
  extension = { kdl = "kdl", ksl = "ksl" },
})

-- With nvim-lspconfig (older Nvim):
require("lspconfig.configs").kdli = {
  default_config = {
    cmd = { "kdli" },
    filetypes = { "kdl", "ksl" },
    root_dir = require("lspconfig.util").root_pattern(".git", "."),
    single_file_support = true,
  },
}
require("lspconfig").kdli.setup({})

-- With Nvim 0.11+ native API:
-- vim.lsp.config("kdli", { cmd = { "kdli" }, filetypes = { "kdl", "ksl" } })
-- vim.lsp.enable("kdli")
```

Docs describe both paths. No upstream PR to `nvim-lspconfig` in v1 — that's post-v1.

## Zed (deferred, housed in-repo)

Lives at `kdli/zed-extension/`. Scaffold created as part of v1 planning so the structure exists, but the extension itself is a **post-v1 milestone**. It will be a minimal Zed extension:

```
kdli/zed-extension/
├── extension.toml
├── languages/
│   ├── kdl/config.toml
│   └── ksl/config.toml
└── src/           # optional Rust registration
```

The extension spawns `kdli` as the LSP, wires `.kdl` and `.ksl` file types, and points at whatever tree-sitter grammar Zed provides for KDL. Published to Zed's extension registry when ready. Decision to house in-repo (vs. separate) is deliberate: Zed extensions are small, and the sync cost of a separate repo outweighs the monorepo friction at this scale.

## VSCode (deferred, separate repo)

Explicit non-goal for v1. Design note: the future extension is a ~100-line TypeScript package that spawns `kdli` as a stdio LSP. It lives in a separate repo because:

- Different release cadence (VSCode extensions republish frequently; `kdli` ships tagged releases).
- Different toolchain (Node / TS / `vsce` vs. Rust / cargo).
- Different publishing pipeline (Open VSX + VS Code Marketplace).

Mixing these in the `kdli` repo creates CI complexity with no upside.

## Tree-sitter grammar policy

`kdli` does not own any tree-sitter grammars. We reference:

- `kslv2/tree-sitter-ksl/` for `.ksl` files.
- `tree-sitter-kdl` (upstream community) for `.kdl` files.

If grammar gaps bite users at v1, we file upstream issues or fork into a dedicated `tree-sitter-*` repo. Not something that should happen inside `kdli/`.

## Installation snippet (editor-agnostic)

Top-level `docs/install.md`:

```
# Install
cargo install kdli            # from crates.io
# or
curl -fsSL https://install.kdli.sh | sh    # cargo-dist prebuilts

# Verify
kdli --version
```

Exact install URL lives in CI (cargo-dist).

## Swarm parallelism for editor docs

T4a (Helix), T4b (Neovim), and T4c (Zed scaffold) are parallelizable; three concurrent subagents, distinct files, no shared state. See [`08-swarm-execution.md`](./08-swarm-execution.md).
