# Writing Unicorns — Language Modules

Standalone, testable language support crates for the Writing Unicorns IDE.
Each module compiles to a native dynamic library (`.so` / `.dll` / `.dylib`) loaded at runtime via FFI.

## Modules

| Module | Language | Extensions |
|--------|----------|------------|
| `rust-lang` | Rust | `.rs` |
| `typescript-lang` | TypeScript | `.ts`, `.tsx` |
| `javascript-lang` | JavaScript | `.js`, `.jsx`, `.mjs` |
| `python-lang` | Python | `.py`, `.pyw` |
| `go-lang` | Go | `.go` |
| `vue-lang` | Vue | `.vue` |
| `react-lang` | React (JSX/TSX) | `.jsx`, `.tsx` |
| `svelte-lang` | Svelte | `.svelte` |
| `toml-lang` | TOML | `.toml` |
| `xml-lang` | XML | `.xml`, `.svg` |
| `html-lang` | HTML | `.html`, `.htm` |
| `csharp-lang` | C# | `.cs` |
| `powershell-lang` | PowerShell | `.ps1`, `.psm1`, `.psd1` |
| `spd-lang` | speedster-js | `.spd` |

## Installation in the IDE

**From the registry (recommended):** open the extension picker in the IDE, go to the
**Registry** tab and click *Install* (or *Update*). The IDE reads the module index published
on GitHub Pages:

- Index: <https://ajustor.github.io/coding-unicorns-modules/registry.json>
- Browsable list: <https://ajustor.github.io/coding-unicorns-modules/>

Each entry gives the module's id, version, languages and LSP server, plus a download URL,
SHA-256 and size per platform (`windows-x86_64`, `linux-x86_64`, `macos-aarch64`). The IDE
verifies the checksum before installing.

**Manually:** download `<module>.zip` from the
[Releases page](https://github.com/Ajustor/coding-unicorns-modules/releases) and use
*Install from ZIP* in the extension picker.

Or build from source (see below).

## Building from source

**Prerequisites:** Rust stable toolchain ([rustup.rs](https://rustup.rs))

```bash
# Build all modules
cargo build --release --workspace

# Artifacts are in target/release/
# Linux:   lib<name>.so
# Windows: <name>.dll
# macOS:   lib<name>.dylib
```

## Running tests

```bash
cargo test                        # all modules
cargo test -p rust-lang           # just Rust
cargo test -p typescript-lang     # just TypeScript
```

## CI / CD

The pipeline runs on every push and pull request:
- Builds all modules for Linux, Windows, and macOS in parallel
- On a `v*` tag push, creates a GitHub release with, for each module:
  - `<module>.zip`: manifest + the libraries for all three platforms
  - `<module>-<platform>.zip`: manifest + one platform's library (used by the registry)
- Then regenerates the registry (`scripts/build_registry.py`) and publishes it to GitHub
  Pages. Pre-release tags (`v1.2.3-rc.1`) create a release but don't update the registry.

To cut a release (bump `version` in the changed modules' `manifest.toml` first, so the IDE
offers the update):
```bash
git tag v0.4.0
git push origin v0.4.0
```

## Adding a new language module

1. Create a new directory: `my-lang/`
2. Add `Cargo.toml` with `crate-type = ["rlib", "cdylib"]`
3. Implement `tokenize_line(line: &str) -> Vec<Token>`
4. Export the FFI functions (see below)
5. Add to workspace `Cargo.toml` members
6. Add the module entry in this README

## FFI Interface

Each module exports:

| Symbol | Signature | Description |
|--------|-----------|-------------|
| `language_id` | `() -> *const c_char` | Language identifier |
| `file_extensions` | `() -> *const c_char` | Comma-separated extensions |
| `tokenize_line_ffi` | `(*const c_char) -> *mut c_char` | JSON token array for a line |
| `free_string` | `(*mut c_char)` | Free a string returned by the module |
