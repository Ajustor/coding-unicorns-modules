//! JSON language module: highlighting for JSON, JSONC (comments, trailing
//! commas) and JSON5-style unquoted keys, plus hover docs for literals and
//! the well-known keys of `package.json` and `tsconfig.json`.
//!
//! Object keys are reported as `property`, values by type (`string`,
//! `number`, `keyword` for `true`/`false`/`null`), comments as `comment`.

use std::ffi::{c_char, CStr, CString};

/// Token kinds understood by the IDE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Property,
    String,
    Number,
    Keyword,
    Comment,
    Normal,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Property => "property",
            Kind::String => "string",
            Kind::Number => "number",
            Kind::Keyword => "keyword",
            Kind::Comment => "comment",
            Kind::Normal => "normal",
        }
    }
}

/// Tokens of each line of `source` (one vector per line, line breaks
/// excluded). Block comments and strings may span lines.
pub fn tokenize(source: &str) -> Vec<Vec<(String, Kind)>> {
    let chars: Vec<char> = source.chars().collect();
    let mut lines: Vec<Vec<(String, Kind)>> = vec![Vec::new()];
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let start = i;
        let kind = match c {
            '\n' => {
                lines.push(Vec::new());
                i += 1;
                continue;
            }
            '\r' => {
                i += 1;
                continue;
            }
            '"' | '\'' => {
                i = scan_string(&chars, i);
                if is_key(&chars, i) {
                    Kind::Property
                } else {
                    Kind::String
                }
            }
            '/' if chars.get(i + 1) == Some(&'/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                Kind::Comment
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                i += 2;
                while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                    i += 1;
                }
                i = (i + 2).min(chars.len());
                Kind::Comment
            }
            '-' | '+' | '.' | '0'..='9' => {
                i += 1;
                while i < chars.len()
                    && (chars[i].is_ascii_alphanumeric() || matches!(chars[i], '.' | '+' | '-'))
                {
                    // `1e-5`, `0x1F`, `Infinity` after a sign…
                    if matches!(chars[i], '+' | '-') && !matches!(chars[i - 1], 'e' | 'E') {
                        break;
                    }
                    i += 1;
                }
                Kind::Number
            }
            c if c.is_alphabetic() || c == '_' || c == '$' => {
                while i < chars.len()
                    && (chars[i].is_alphanumeric() || matches!(chars[i], '_' | '$'))
                {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                if is_key(&chars, i) {
                    Kind::Property // JSON5 unquoted key
                } else if matches!(word.as_str(), "true" | "false" | "null") {
                    Kind::Keyword
                } else if matches!(word.as_str(), "Infinity" | "NaN") {
                    Kind::Number
                } else {
                    Kind::Normal
                }
            }
            c if c.is_whitespace() => {
                while i < chars.len()
                    && chars[i].is_whitespace()
                    && chars[i] != '\n'
                    && chars[i] != '\r'
                {
                    i += 1;
                }
                Kind::Normal
            }
            _ => {
                i += 1;
                Kind::Normal
            }
        };
        push_text(&mut lines, &chars[start..i], kind);
    }
    lines
}

/// End index (exclusive) of the string literal starting at `start`.
fn scan_string(chars: &[char], start: usize) -> usize {
    let quote = chars[start];
    let mut i = start + 1;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 2,
            '\n' => return i, // unterminated: stop at the end of the line
            c if c == quote => return i + 1,
            _ => i += 1,
        }
    }
    chars.len()
}

/// Whether the token ending at `end` is an object key: the next significant
/// character (skipping whitespace and comments) is `:`.
fn is_key(chars: &[char], end: usize) -> bool {
    let mut i = end;
    loop {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if chars.get(i) == Some(&'/') && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i += 2;
        } else if chars.get(i) == Some(&'/') && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else {
            return chars.get(i) == Some(&':');
        }
    }
}

/// Add a token, splitting it on line breaks (multi-line comments/strings)
/// and merging it with the previous token of the same kind.
fn push_text(lines: &mut Vec<Vec<(String, Kind)>>, text: &[char], kind: Kind) {
    let text: String = text.iter().filter(|&&c| c != '\r').collect();
    for (n, piece) in text.split('\n').enumerate() {
        if n > 0 {
            lines.push(Vec::new());
        }
        if piece.is_empty() {
            continue;
        }
        let line = lines.last_mut().expect("at least one line");
        match line.last_mut() {
            Some((prev, k)) if *k == kind => prev.push_str(piece),
            _ => line.push((piece.to_string(), kind)),
        }
    }
}

// ── Hover ─────────────────────────────────────────────────────────────────────

const LITERALS: &[(&str, &str)] = &[
    ("true", "Boolean literal `true`."),
    ("false", "Boolean literal `false`."),
    ("null", "The `null` literal: an empty or unknown value."),
    ("$schema", "URL of the JSON Schema this document follows; the language server uses it for validation and completion."),
];

const PACKAGE_JSON: &[(&str, &str)] = &[
    (
        "name",
        "Package name: lowercase, URL-safe, optionally scoped (`@scope/name`).",
    ),
    (
        "version",
        "Package version, in semantic versioning (`major.minor.patch`).",
    ),
    ("description", "Short description, shown by `npm search`."),
    (
        "main",
        "Entry point when the package is required (CommonJS).",
    ),
    ("module", "ES module entry point, used by bundlers."),
    ("types", "TypeScript declaration file of the package."),
    (
        "type",
        "`\"module\"` makes `.js` files ES modules; `\"commonjs\"` (default) keeps CommonJS.",
    ),
    (
        "exports",
        "Public entry points of the package, per import condition (`import`, `require`, `types`…).",
    ),
    (
        "bin",
        "Executables installed on `PATH`: command name → script.",
    ),
    (
        "scripts",
        "Commands run with `npm run <name>` (`start`, `test`, `build`…).",
    ),
    (
        "dependencies",
        "Packages required at runtime: name → version range.",
    ),
    (
        "devDependencies",
        "Packages only needed for development and builds.",
    ),
    (
        "peerDependencies",
        "Packages the host project must provide (e.g. a framework for a plugin).",
    ),
    (
        "optionalDependencies",
        "Dependencies whose installation may fail without failing the install.",
    ),
    (
        "engines",
        "Supported runtime versions (e.g. `{ \"node\": \">=18\" }`).",
    ),
    (
        "private",
        "`true` prevents publishing the package to the registry.",
    ),
    (
        "workspaces",
        "Folders (globs) of the packages of a monorepo.",
    ),
    ("license", "SPDX license identifier (e.g. `MIT`)."),
    (
        "repository",
        "Where the source code lives (`{ \"type\": \"git\", \"url\": … }`).",
    ),
    ("files", "Files included when the package is published."),
    (
        "keywords",
        "Words that help find the package with `npm search`.",
    ),
    (
        "author",
        "Package author: `\"Name <email> (url)\"` or an object.",
    ),
];

const TSCONFIG_JSON: &[(&str, &str)] = &[
    ("compilerOptions", "Options of the TypeScript compiler."),
    ("include", "Globs of the files part of the project."),
    (
        "exclude",
        "Globs removed from `include` (default: `node_modules`, the `outDir`).",
    ),
    ("extends", "Base configuration file this one overrides."),
    (
        "references",
        "Other TypeScript projects this one depends on (project references).",
    ),
    (
        "target",
        "JavaScript version emitted (`ES2022`, `ESNext`…).",
    ),
    (
        "module",
        "Module system emitted (`NodeNext`, `ESNext`, `CommonJS`…).",
    ),
    (
        "moduleResolution",
        "How imports are resolved (`Bundler`, `NodeNext`…).",
    ),
    ("strict", "Enables every strict type-checking option."),
    ("outDir", "Folder the compiled files are written to."),
    ("rootDir", "Root folder of the input files."),
    (
        "lib",
        "Built-in API declarations available (`DOM`, `ES2023`…).",
    ),
    ("jsx", "How JSX is compiled (`react-jsx`, `preserve`…)."),
    ("paths", "Import path aliases, relative to `baseUrl`."),
    ("baseUrl", "Base folder of non-relative imports."),
    (
        "esModuleInterop",
        "Allows default imports of CommonJS modules.",
    ),
    (
        "skipLibCheck",
        "Skips type checking of declaration files (`.d.ts`).",
    ),
    ("declaration", "Emits `.d.ts` declaration files."),
    ("sourceMap", "Emits source maps (`.js.map`) for debugging."),
    ("noEmit", "Type-checks only, without writing output files."),
];

/// Hover documentation for `word` in a document: JSON literals, then the
/// keys of `package.json` / `tsconfig.json` when the document looks like one.
pub fn hover(word: &str, content: &str) -> Option<String> {
    let word = word.trim_matches(|c| c == '"' || c == '\'');
    if let Some((_, doc)) = LITERALS.iter().find(|(k, _)| *k == word) {
        return Some(doc.to_string());
    }
    let has_key = |k: &str| content.contains(&format!("\"{k}\""));
    let table = if has_key("compilerOptions") {
        TSCONFIG_JSON
    } else if has_key("dependencies") || has_key("devDependencies") || has_key("scripts") {
        PACKAGE_JSON
    } else {
        return None;
    };
    // Only keys: the word must appear quoted and followed by `:`.
    if !is_key_in(content, word) {
        return None;
    }
    table
        .iter()
        .find(|(k, _)| *k == word)
        .map(|(k, doc)| format!("**`{k}`** — {doc}"))
}

fn is_key_in(content: &str, word: &str) -> bool {
    let quoted = format!("\"{word}\"");
    content
        .match_indices(&quoted)
        .any(|(i, _)| content[i + quoted.len()..].trim_start().starts_with(':'))
}

// ── Serialization ─────────────────────────────────────────────────────────────

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn line_to_json(tokens: &[(String, Kind)]) -> String {
    let toks: Vec<String> = tokens
        .iter()
        .map(|(text, kind)| {
            format!(
                r#"{{"text":{},"kind":"{}"}}"#,
                json_escape(text),
                kind.as_str()
            )
        })
        .collect();
    format!("[{}]", toks.join(","))
}

fn document_to_json(source: &str) -> String {
    let lines: Vec<String> = tokenize(source).iter().map(|l| line_to_json(l)).collect();
    format!("[{}]", lines.join(","))
}

fn into_c(s: String) -> *mut c_char {
    CString::new(s).unwrap_or_default().into_raw()
}

/// # Safety
/// `ptr` must be null or a valid NUL-terminated string.
unsafe fn from_c<'a>(ptr: *const c_char) -> &'a str {
    if ptr.is_null() {
        return "";
    }
    unsafe { CStr::from_ptr(ptr) }.to_str().unwrap_or("")
}

// ── FFI ───────────────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn language_id() -> *const c_char {
    c"json".as_ptr()
}

#[no_mangle]
pub extern "C" fn file_extensions() -> *const c_char {
    c"json,jsonc,json5,geojson,webmanifest".as_ptr()
}

#[no_mangle]
pub extern "C" fn reset_tokenizer() {}

/// # Safety
/// `text_ptr` must be null or a valid NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn tokenize_document_ffi(text_ptr: *const c_char) -> *mut c_char {
    into_c(document_to_json(unsafe { from_c(text_ptr) }))
}

/// # Safety
/// `line_ptr` must be null or a valid NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn tokenize_line_ffi(line_ptr: *const c_char) -> *mut c_char {
    let line = unsafe { from_c(line_ptr) };
    let tokens = tokenize(line).into_iter().next().unwrap_or_default();
    into_c(line_to_json(&tokens))
}

/// # Safety
/// Both pointers must be null or valid NUL-terminated strings.
#[no_mangle]
pub unsafe extern "C" fn hover_info_ffi(
    word_ptr: *const c_char,
    file_content_ptr: *const c_char,
) -> *mut c_char {
    let (word, content) = unsafe { (from_c(word_ptr), from_c(file_content_ptr)) };
    match hover(word, content) {
        Some(doc) => into_c(doc),
        None => std::ptr::null_mut(),
    }
}

/// # Safety
/// `ptr` must be null or a string returned by this module.
#[no_mangle]
pub unsafe extern "C" fn free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe { drop(CString::from_raw(ptr)) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(line: &[(String, Kind)]) -> Vec<(&str, Kind)> {
        line.iter()
            .filter(|(t, _)| !t.trim().is_empty())
            .map(|(t, k)| (t.trim(), *k))
            .collect()
    }

    #[test]
    fn keys_values_and_literals() {
        let lines = tokenize(r#"{"name": "app", "n": -1.5e+3, "ok": true, "x": null}"#);
        assert_eq!(
            kinds(&lines[0]),
            [
                ("{", Kind::Normal),
                ("\"name\"", Kind::Property),
                (":", Kind::Normal),
                ("\"app\"", Kind::String),
                (",", Kind::Normal),
                ("\"n\"", Kind::Property),
                (":", Kind::Normal),
                ("-1.5e+3", Kind::Number),
                (",", Kind::Normal),
                ("\"ok\"", Kind::Property),
                (":", Kind::Normal),
                ("true", Kind::Keyword),
                (",", Kind::Normal),
                ("\"x\"", Kind::Property),
                (":", Kind::Normal),
                ("null", Kind::Keyword),
                ("}", Kind::Normal),
            ]
        );
    }

    #[test]
    fn escaped_quotes_stay_in_the_string() {
        let lines = tokenize(r#"["a \"b\" c", "d"]"#);
        assert_eq!(
            kinds(&lines[0]),
            [
                ("[", Kind::Normal),
                (r#""a \"b\" c""#, Kind::String),
                (",", Kind::Normal),
                ("\"d\"", Kind::String),
                ("]", Kind::Normal),
            ]
        );
    }

    #[test]
    fn jsonc_comments_span_lines_and_keys_before_comments() {
        let src = "{\n  // line\n  \"a\" /* c */ : 1, /* multi\n  line */ \"b\": [2,]\n}";
        let lines = tokenize(src);
        assert_eq!(lines.len(), 5);
        assert_eq!(kinds(&lines[1]), [("// line", Kind::Comment)]);
        assert_eq!(
            kinds(&lines[2]),
            [
                ("\"a\"", Kind::Property),
                ("/* c */", Kind::Comment),
                (":", Kind::Normal),
                ("1", Kind::Number),
                (",", Kind::Normal),
                ("/* multi", Kind::Comment),
            ]
        );
        assert_eq!(kinds(&lines[3])[0], ("line */", Kind::Comment));
        assert_eq!(kinds(&lines[3])[1], ("\"b\"", Kind::Property));
    }

    #[test]
    fn key_on_its_own_line_and_json5_unquoted_keys() {
        let lines = tokenize("{\n  \"long\"\n    : 1,\n  short: 'x', n: Infinity\n}");
        assert_eq!(kinds(&lines[1]), [("\"long\"", Kind::Property)]);
        assert_eq!(
            kinds(&lines[3]),
            [
                ("short", Kind::Property),
                (":", Kind::Normal),
                ("'x'", Kind::String),
                (",", Kind::Normal),
                ("n", Kind::Property),
                (":", Kind::Normal),
                ("Infinity", Kind::Number),
            ]
        );
    }

    #[test]
    fn crlf_and_unterminated_string() {
        let lines = tokenize("{\r\n\"a\": \"open\r\n}");
        assert_eq!(lines.len(), 3);
        assert_eq!(kinds(&lines[1])[2], ("\"open", Kind::String));
        assert_eq!(kinds(&lines[2]), [("}", Kind::Normal)]);
        // Line text round-trips (no \r kept).
        let text: String = lines[1].iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(text, "\"a\": \"open");
    }

    #[test]
    fn hover_literals_and_known_files() {
        assert!(hover("null", "").unwrap().contains("null"));
        assert!(hover("$schema", "{}").unwrap().contains("JSON Schema"));
        let pkg = r#"{"name": "x", "scripts": {"build": "tsc"}, "dependencies": {}}"#;
        assert!(hover("scripts", pkg).unwrap().contains("npm run"));
        assert!(hover("\"dependencies\"", pkg).unwrap().contains("runtime"));
        assert!(hover("build", pkg).is_none(), "not a known key");
        assert!(hover("tsc", pkg).is_none(), "values have no hover");
        let ts = r#"{"compilerOptions": {"strict": true}}"#;
        assert!(hover("strict", ts).unwrap().contains("strict"));
        assert!(hover("scripts", ts).is_none());
        assert!(
            hover("name", r#"{"name": 1}"#).is_none(),
            "unknown file kind"
        );
    }

    #[test]
    fn ffi_round_trip() {
        unsafe {
            let src = CString::new("{\"a\": 1}\n// c").unwrap();
            let out = tokenize_document_ffi(src.as_ptr());
            let json = CStr::from_ptr(out).to_str().unwrap().to_string();
            free_string(out);
            assert_eq!(
                json,
                r#"[[{"text":"{","kind":"normal"},{"text":"\"a\"","kind":"property"},{"text":": ","kind":"normal"},{"text":"1","kind":"number"},{"text":"}","kind":"normal"}],[{"text":"// c","kind":"comment"}]]"#
            );

            let line = CString::new("\"k\": \"v\"").unwrap();
            let out = tokenize_line_ffi(line.as_ptr());
            assert!(CStr::from_ptr(out)
                .to_str()
                .unwrap()
                .contains(r#""kind":"property""#));
            free_string(out);

            let word = CString::new("true").unwrap();
            let out = hover_info_ffi(word.as_ptr(), std::ptr::null());
            assert!(!out.is_null());
            free_string(out);
            let word = CString::new("zzz").unwrap();
            assert!(hover_info_ffi(word.as_ptr(), std::ptr::null()).is_null());

            assert_eq!(CStr::from_ptr(language_id()).to_str().unwrap(), "json");
            assert!(CStr::from_ptr(file_extensions())
                .to_str()
                .unwrap()
                .contains("jsonc"));
            free_string(std::ptr::null_mut());
        }
    }

    #[test]
    fn control_characters_are_escaped() {
        assert_eq!(json_escape("a\u{1}\"\\"), r#""a\u0001\"\\""#);
    }
}
