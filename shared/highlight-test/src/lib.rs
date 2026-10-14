//! Snapshot tests of a module's highlighting.
//!
//! Each module keeps sample files in `tests/samples/<language>/`, where
//! `<language>` is one of the keys its `file_extensions()` returns (the
//! language the IDE asks for). [`check_samples`] highlights every sample
//! through the module's FFI functions, in the order the IDE tries them, and
//! compares the result with the `<sample>.tokens` file next to it.
//!
//! It also checks what must hold for any language:
//! - the tokens of a line put back together give the line, unchanged;
//! - there is one token line per source line;
//! - a sample is not left entirely uncolored (a broken query does that).
//!
//! After an intended change, regenerate the snapshots and review their diff:
//! `UPDATE_HIGHLIGHT=1 cargo test`.

use std::ffi::{c_char, CStr, CString};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

type Str0 = unsafe extern "C" fn() -> *const c_char;
type Str1 = unsafe extern "C" fn(*const c_char) -> *mut c_char;
type Str2 = unsafe extern "C" fn(*const c_char, *const c_char) -> *mut c_char;

/// The module's highlighting exports. Leave out (`None`) the ones it doesn't
/// have, as the IDE then skips them.
pub struct Ffi {
    pub file_extensions: Str0,
    pub free_string: unsafe extern "C" fn(*mut c_char),
    pub tokenize_document_lang: Option<Str2>,
    pub tokenize_document: Option<Str1>,
    /// Used instead of `tokenize_document` for `tsx` and `jsx`.
    pub tokenize_document_tsx: Option<Str1>,
    pub tokenize_line_lang: Option<Str2>,
    pub tokenize_line: Option<Str1>,
    pub reset_tokenizer: Option<unsafe extern "C" fn()>,
}

impl Ffi {
    /// Only the required exports: add the module's tokenizers with
    /// struct update syntax.
    pub fn new(file_extensions: Str0, free_string: unsafe extern "C" fn(*mut c_char)) -> Self {
        Self {
            file_extensions,
            free_string,
            tokenize_document_lang: None,
            tokenize_document: None,
            tokenize_document_tsx: None,
            tokenize_line_lang: None,
            tokenize_line: None,
            reset_tokenizer: None,
        }
    }

    fn languages(&self) -> Vec<String> {
        let raw = unsafe { CStr::from_ptr((self.file_extensions)()) };
        raw.to_string_lossy()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    fn call(&self, out: *mut c_char) -> Option<String> {
        if out.is_null() {
            return None;
        }
        let s = unsafe { CStr::from_ptr(out) }
            .to_string_lossy()
            .into_owned();
        unsafe { (self.free_string)(out) };
        Some(s)
    }

    fn document(&self, lang: &str, text: &str) -> Option<String> {
        let (l, t) = (c(lang), c(text));
        if let Some(f) = self.tokenize_document_lang {
            return self.call(unsafe { f(l.as_ptr(), t.as_ptr()) });
        }
        let tsx = (lang == "tsx" || lang == "jsx")
            .then_some(self.tokenize_document_tsx)
            .flatten();
        let f = tsx.or(self.tokenize_document)?;
        self.call(unsafe { f(t.as_ptr()) })
    }

    fn line(&self, lang: &str, line: &str) -> Option<String> {
        let (l, t) = (c(lang), c(line));
        if let Some(f) = self.tokenize_line_lang {
            return self.call(unsafe { f(l.as_ptr(), t.as_ptr()) });
        }
        let f = self.tokenize_line?;
        self.call(unsafe { f(t.as_ptr()) })
    }

    /// Token lines of `text`, as the editor gets them: the document
    /// tokenizer when it gives something, else line by line after a reset.
    pub fn highlight(&self, lang: &str, text: &str) -> Vec<Vec<Token>> {
        if let Some(json) = self.document(lang, text) {
            let lines = parse_document(&json);
            if !lines.is_empty() {
                return lines;
            }
        }
        if let Some(reset) = self.reset_tokenizer {
            unsafe { reset() };
        }
        text.lines()
            .map(|line| {
                self.line(lang, line)
                    .map(|json| parse_line(&json))
                    .unwrap_or_else(|| vec![Token::normal(line)])
            })
            .collect()
    }
}

fn c(s: &str) -> CString {
    CString::new(s).expect("sample contains a NUL byte")
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub text: String,
    /// The kind the IDE draws: one of its kinds, `normal` for any other.
    pub kind: &'static str,
}

impl Token {
    fn normal(text: &str) -> Self {
        Self {
            text: text.to_string(),
            kind: "normal",
        }
    }
}

/// The IDE's token kinds (`ffi_plugin.rs`); anything else is drawn normal.
fn ide_kind(kind: &str) -> &'static str {
    match kind {
        "keyword" => "keyword",
        "type" => "type",
        "string" => "string",
        "comment" => "comment",
        "number" => "number",
        "function" => "function",
        "macro" => "macro",
        "property" => "property",
        "operator" => "operator",
        "class" => "class",
        _ => "normal",
    }
}

fn tokens_of(value: &serde_json::Value) -> Vec<Token> {
    value
        .as_array()
        .map(|toks| {
            toks.iter()
                .map(|t| Token {
                    text: t["text"].as_str().unwrap_or_default().to_string(),
                    kind: ide_kind(t["kind"].as_str().unwrap_or_default()),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_document(json: &str) -> Vec<Vec<Token>> {
    let value: serde_json::Value = serde_json::from_str(json)
        .unwrap_or_else(|e| panic!("invalid document JSON ({e}): {json}"));
    value
        .as_array()
        .map(|lines| lines.iter().map(tokens_of).collect())
        .unwrap_or_default()
}

fn parse_line(json: &str) -> Vec<Token> {
    let value: serde_json::Value =
        serde_json::from_str(json).unwrap_or_else(|e| panic!("invalid line JSON ({e}): {json}"));
    tokens_of(&value)
}

/// The snapshot of `lines` for `source`: each source line, then its colored
/// tokens (`kind "text"`), normal text left out.
pub fn render(source: &str, lines: &[Vec<Token>]) -> String {
    let mut out = String::new();
    for (n, (text, tokens)) in source.lines().zip(lines).enumerate() {
        let _ = writeln!(out, "{:>4} | {text}", n + 1);
        let colored: Vec<String> = tokens
            .iter()
            .filter(|t| t.kind != "normal" && !t.text.trim().is_empty())
            .map(|t| format!("{} {:?}", t.kind, t.text))
            .collect();
        if !colored.is_empty() {
            let _ = writeln!(out, "     = {}", colored.join("  "));
        }
    }
    out
}

/// What is wrong with `lines` as the highlighting of `source`, whatever the
/// language.
fn invariant_errors(source: &str, lines: &[Vec<Token>]) -> Vec<String> {
    let mut errors = Vec::new();
    let src: Vec<&str> = source.lines().collect();
    // A trailing empty token line (for the text after the last newline) is
    // harmless: the editor has that line too.
    let trailing_empty = lines.len() == src.len() + 1
        && lines
            .last()
            .is_some_and(|l| l.iter().all(|t| t.text.is_empty()));
    if lines.len() != src.len() && !trailing_empty {
        errors.push(format!(
            "{} token lines for {} source lines",
            lines.len(),
            src.len()
        ));
    }
    for (n, (text, tokens)) in src.iter().zip(lines).enumerate() {
        let joined: String = tokens.iter().map(|t| t.text.as_str()).collect();
        if joined != *text {
            errors.push(format!(
                "line {}: tokens give {joined:?} instead of {text:?}",
                n + 1
            ));
        }
    }
    if !lines.iter().flatten().any(|t| t.kind != "normal") {
        errors.push("nothing is colored".to_string());
    }
    errors
}

fn samples(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let Ok(langs) = std::fs::read_dir(dir) else {
        return out;
    };
    for lang in langs.flatten().filter(|e| e.path().is_dir()) {
        let name = lang.file_name().to_string_lossy().into_owned();
        for file in std::fs::read_dir(lang.path())
            .into_iter()
            .flatten()
            .flatten()
        {
            let path = file.path();
            if path.is_file() && path.extension().is_none_or(|e| e != "tokens") {
                out.push((name.clone(), path));
            }
        }
    }
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out
}

/// Highlights every sample of the module at `manifest_dir`
/// (`env!("CARGO_MANIFEST_DIR")`) and checks it against its snapshot.
/// Panics with every problem found.
pub fn check_samples(manifest_dir: &str, ffi: &Ffi) {
    let dir = Path::new(manifest_dir).join("tests").join("samples");
    let samples = samples(&dir);
    assert!(!samples.is_empty(), "no samples in {}", dir.display());
    let update = std::env::var_os("UPDATE_HIGHLIGHT").is_some();
    let languages = ffi.languages();
    let mut errors = Vec::new();

    for (lang, path) in samples {
        let rel = path
            .strip_prefix(&dir)
            .unwrap_or(&path)
            .display()
            .to_string();
        if !languages.contains(&lang) {
            errors.push(format!(
                "{rel}: `{lang}` is not a language of the module ({})",
                languages.join(",")
            ));
            continue;
        }
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{rel}: {e}"))
            .replace("\r\n", "\n");
        let lines = ffi.highlight(&lang, &source);
        errors.extend(
            invariant_errors(&source, &lines)
                .into_iter()
                .map(|e| format!("{rel}: {e}")),
        );

        let actual = render(&source, &lines);
        let snap = path.with_file_name(format!(
            "{}.tokens",
            path.file_name().unwrap().to_string_lossy()
        ));
        let expected = std::fs::read_to_string(&snap)
            .ok()
            .map(|s| s.replace("\r\n", "\n"));
        if update {
            if expected.as_deref() != Some(&actual) {
                std::fs::write(&snap, &actual).unwrap_or_else(|e| panic!("{rel}: {e}"));
            }
        } else {
            match expected {
                None => errors.push(format!(
                    "{rel}: no snapshot; run `UPDATE_HIGHLIGHT=1 cargo test` and review it"
                )),
                Some(expected) if expected != actual => errors.push(format!(
                    "{rel}: highlighting changed\n{}",
                    diff(&expected, &actual)
                )),
                Some(_) => {}
            }
        }
    }
    assert!(
        errors.is_empty(),
        "\n{}\n\nIf the change is intended: `UPDATE_HIGHLIGHT=1 cargo test`, then review the .tokens diff.",
        errors.join("\n\n")
    );
}

/// The lines that differ, with their line number.
fn diff(expected: &str, actual: &str) -> String {
    let (e, a): (Vec<&str>, Vec<&str>) = (expected.lines().collect(), actual.lines().collect());
    let mut out = String::new();
    for i in 0..e.len().max(a.len()) {
        let (x, y) = (e.get(i), a.get(i));
        if x != y {
            let _ = writeln!(out, "  expected: {}", x.unwrap_or(&"<none>"));
            let _ = writeln!(out, "  actual:   {}", y.unwrap_or(&"<none>"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tok(text: &str, kind: &'static str) -> Token {
        Token {
            text: text.into(),
            kind,
        }
    }

    #[test]
    fn render_lists_colored_tokens_under_their_line() {
        let lines = vec![
            vec![
                tok("let", "keyword"),
                tok(" x = ", "normal"),
                tok("1", "number"),
            ],
            vec![tok("", "normal")],
        ];
        assert_eq!(
            render("let x = 1\n\n", &lines),
            "   1 | let x = 1\n     = keyword \"let\"  number \"1\"\n   2 | \n"
        );
    }

    #[test]
    fn invariants_catch_lost_text_and_missing_color() {
        let ok = vec![vec![tok("a", "keyword"), tok(" b", "normal")]];
        assert!(invariant_errors("a b", &ok).is_empty());
        let lost = vec![vec![tok("a", "keyword")]];
        assert_eq!(invariant_errors("a b", &lost).len(), 1);
        let plain = vec![vec![tok("a b", "normal")]];
        assert_eq!(invariant_errors("a b", &plain), ["nothing is colored"]);
        assert_eq!(
            invariant_errors("a\nb", &ok).len(),
            2,
            "line count and text"
        );
    }

    #[test]
    fn unknown_kinds_are_drawn_normal() {
        let line = parse_line(r#"[{"text":"x","kind":"variable"},{"text":"y","kind":"type"}]"#);
        assert_eq!(line, [tok("x", "normal"), tok("y", "type")]);
    }
}
