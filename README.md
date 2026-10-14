# Writing Unicorns — Language Modules

Standalone, testable language support crates for the Writing Unicorns IDE.
Each module compiles to a native dynamic library (`.so` / `.dll` / `.dylib`) loaded at runtime via FFI.

## Modules

| Module | Language | Extensions | Debugger | Documentation |
|--------|----------|------------|----------|---------------|
| `rust-lang` | Rust | `.rs` | lldb-dap | [docs](https://doc.rust-lang.org/book/) |
| `typescript-lang` | TypeScript | `.ts`, `.tsx` | vscode-js-debug | [docs](https://www.typescriptlang.org/docs/) |
| `javascript-lang` | JavaScript | `.js`, `.jsx`, `.mjs` | vscode-js-debug | [docs](https://developer.mozilla.org/docs/Web/JavaScript) |
| `python-lang` | Python | `.py`, `.pyw` | debugpy | [docs](https://docs.python.org/3/) |
| `go-lang` | Go | `.go` | Delve | [docs](https://go.dev/doc/) |
| `vue-lang` | Vue | `.vue` | vscode-js-debug | [docs](https://vuejs.org/guide/introduction.html) |
| `react-lang` | React (JSX/TSX) | `.jsx`, `.tsx` | vscode-js-debug | [docs](https://react.dev/learn) |
| `svelte-lang` | Svelte | `.svelte` | vscode-js-debug | [docs](https://svelte.dev/docs) |
| `toml-lang` | TOML | `.toml` | — | [docs](https://toml.io/en/latest) |
| `xml-lang` | XML | `.xml`, `.svg` | — | [docs](https://www.w3.org/XML/) |
| `html-lang` | HTML | `.html`, `.htm` | vscode-js-debug | [docs](https://developer.mozilla.org/docs/Web/HTML) |
| `csharp-lang` | C# | `.cs` | netcoredbg | [docs](https://learn.microsoft.com/dotnet/csharp/) |
| `powershell-lang` | PowerShell | `.ps1`, `.psm1`, `.psd1` | PowerShell Editor Services | [docs](https://learn.microsoft.com/powershell/) |
| `spd-lang` | speedster-js | `.spd` | — | [docs](https://ajustor.github.io/speedster-js/) |
| `json-lang` | JSON / JSONC | `.json`, `.jsonc`, `.json5`, `.geojson`, `.webmanifest` | — | [docs](https://www.json.org/json-en.html) |
| `docker-lang` | Dockerfile, Docker Compose | `Dockerfile`, `Containerfile`, `Dockerfile.*`, `.dockerfile`, `compose.yaml`, `docker-compose.yml`, `compose.*.yml`, `docker-compose.*.yml` | — | [docs](https://docs.docker.com/reference/) |
| `markdown-lang` | Markdown (CommonMark, GFM) | `.md`, `.markdown`, `.mdown`, `.mkd`, `.mkdn`, `.mdwn` | — | [docs](https://commonmark.org/help/) |
| `java-lang` | Java, Gradle (Groovy / Kotlin DSL), Kotlin, properties | `.java`, `.gradle`, `.groovy`, `.gradle.kts`, `.kts`, `.kt`, `.properties`, `MANIFEST.MF` | java-debug (in JDT LS) | [docs](https://dev.java/learn/) |
| `yaml-lang` | YAML | `.yml`, `.yaml` | — | [docs](https://yaml.org/spec/1.2.2/) |
| `sql-lang` | SQL | `.sql` | — | [docs](https://www.postgresql.org/docs/current/sql.html) |

Dockerfiles and Compose files are recognised by their name (`file_names`, Coding Unicorns
0.10.6+). Compose files get their own `compose` language, with their own language server
(`[capabilities.lsp_servers.compose]`) and tokenizer (the `*_lang_ffi` exports receive the
language), so a future YAML module can own the other `.yml` files.

`docker-lang` also adds a **Docker** panel to the activity bar (`[[panels]]`, Coding Unicorns
0.10.7+): local images with *Pull*, *Run* (in a terminal), *Remove* and *Prune*, and a page with
the full table. The panel also lists the **running containers**, with *Logs* and *Stop*.
*Containers & logs* opens a page listing the containers (*Start*, *Stop*,
*Restart*, *Remove*, *Shell*) and a logs page following the chosen container
(`docker-lang/src/containers.rs`; the `log` view element needs Coding Unicorns 0.10.8+). The module returns JSON views from `ui_view_ffi` and handles clicks in
`ui_event_ffi`, running the `docker` CLI on its own threads (`docker-lang/src/panel.rs`).

`shared/highlight` (`unicorns-highlight`, not a module) holds the tree-sitter highlighting the
modules share: a grammar run over a document, and single-file components. `svelte-lang` and
`vue-lang` highlight their markup as HTML, `<script>` as TypeScript, `<style>` as CSS, and template
expressions (`{…}` and `{#if}` blocks in Svelte, `{{ … }}` and `v-…` / `:prop` / `@event`
directives in Vue) as TypeScript.

`yaml-lang` highlights YAML with tree-sitter; its language server,
[yaml-language-server](https://github.com/redhat-developer/yaml-language-server) (installed with npm),
validates and completes files from their JSON schema (GitHub Actions, Kubernetes, OpenAPI…). Compose
files stay with `docker-lang`. `sql-lang` highlights SQL with tree-sitter (common dialects).

`markdown-lang` highlights CommonMark and GitHub Flavored Markdown: headings, emphasis,
code spans and blocks, links and images, reference definitions and footnotes, lists and task
boxes, block quotes, tables, inline HTML and comments, and YAML/TOML front matter. Hovering a
reference label shows its target URL, a footnote label its text. Its language server,
[marksman](https://github.com/artempyanykh/marksman), downloaded on first use, adds the outline, link
and heading completion, go to definition, broken link diagnostics and rename. The rendered
preview is built into Coding Unicorns (Ctrl+Shift+V).

`java-lang` highlights Java with tree-sitter and shows a declaration's signature and Javadoc on
hover. Its language server, [Eclipse JDT LS](https://github.com/eclipse-jdtls/eclipse.jdt.ls)
(completion, diagnostics, navigation, refactoring, Maven and Gradle projects), is downloaded into
the module's folder when the first `.java` file is opened (`[capabilities.lsp_download]`, Coding
Unicorns 0.15.0+) and run by **Java 21 or later**, which must be the `java` on the `PATH`. Its data
(project index) goes to a folder per workspace (`${dataDir}`), outside the project.
Debugging (Coding Unicorns 0.16.0+) uses [java-debug](https://github.com/microsoft/java-debug), a
plugin the server loads (downloaded with it): F5 asks the server for the open file's main class
(or the launch configuration's `mainClass`), its classpath and the project's Java, then the server
starts the session (`transport = "lsp"`, `[[debugger.lsp_resolve]]` in `java-lang/manifest.toml`).
It also highlights the files around Java code with tree-sitter: Gradle scripts in Groovy
(`*.gradle`, `.groovy`, [tree-sitter-groovy](https://github.com/amaanq/tree-sitter-groovy)) and
in Kotlin (`*.gradle.kts`, `.kts`, `.kt`,
[tree-sitter-kotlin-ng](https://github.com/tree-sitter-grammars/tree-sitter-kotlin)), and
`.properties` files (`gradle.properties`, `application.properties`, `MANIFEST.MF`). Neither
grammar ships a highlight query, so `java-lang/queries/groovy.scm` and `kotlin.scm` are written
for their node types. These files have no language server: JDT LS is declared for `java` alone
(`[capabilities.lsp_servers.java]`), as a second instance can't share its data folder.

Language servers that are a single download are fetched into the module's folder when the
first file of their languages is opened, if they are not on the `PATH` (`[capabilities.lsp_download]`,
Coding Unicorns 0.15.0+): rust-analyzer (always the module's own copy, as rustup's
`rust-analyzer` fails until its component is added), taplo, lemminx and marksman.

## Debuggers

A module can ship its language's debug adapter (DAP over stdio) in a `[debugger]` section
of its `manifest.toml`; the IDE has none of its own. It names the adapter command, the
VS Code launch configuration `types` it serves, the transport (stdio, or TCP on
`${port}`), optional default launch arguments and an optional archive per platform,
unpacked into the module's folder (`${debuggerDir}`) on first use. Examples:
`csharp-lang` (downloaded binary), `typescript-lang` (downloaded script run by Node, over
TCP), `powershell-lang` (downloaded module run by `pwsh`), `go-lang` (TCP, tool installed
with the dependencies). The IDE README (*Créer une extension*) documents every field.

Without a launch configuration, F5 debugs the current file for TypeScript, JavaScript,
Python, PowerShell, Go (its package), Java (the open file's main class) and HTML (in Chrome). C#, Rust and the component
frameworks (React, Vue, Svelte) need a `.vscode/launch.json` entry giving the program or
dev server URL. TOML, XML, JSON, Markdown and Docker files are not programs, and speedster-js components run through Bun,
whose debugger does not speak DAP: these modules have no debugger.

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
- On every push to `master` (i.e. every merge), creates a GitHub release tagged with the
  last stable tag's patch bumped (`v0.4.0` → `v0.4.1`), with, for each module:
  - `<module>.zip`: manifest + the libraries for all three platforms
  - `<module>-<platform>.zip`: manifest + one platform's library (used by the registry)
- Then regenerates the registry (`scripts/build_registry.py`) and publishes it to GitHub
  Pages, so the IDE always offers the modules as they are on `master`.

Bump `version` in a changed module's `manifest.toml` in the same PR, so the IDE offers the
update. To choose the release version instead (a minor bump, a pre-release), push a tag:
```bash
git tag v0.5.0
git push origin v0.5.0
```
Pre-release tags (`v1.2.3-rc.1`) create a release but don't update the registry.

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
|--------|----------|------------|----------|---------------|
| `language_id` | `() -> *const c_char` | Language identifier |
| `file_extensions` | `() -> *const c_char` | Comma-separated extensions |
| `tokenize_line_ffi` | `(*const c_char) -> *mut c_char` | JSON token array for a line |
| `free_string` | `(*mut c_char)` | Free a string returned by the module |
