//! Java language module: tree-sitter highlighting and a hover fallback (the
//! declaration's signature and Javadoc, from the open file) for when the
//! language server (Eclipse JDT LS) is not running yet.

use std::ffi::{c_char, CStr, CString};
use std::sync::OnceLock;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute",
    "comment",
    "comment.documentation",
    "constant",
    "constant.builtin",
    "constructor",
    "function.method",
    "keyword",
    "number",
    "property",
    "string",
    "string.escape",
    "type",
    "type.builtin",
    "variable.builtin",
];

/// IDE token kind of each entry of [`HIGHLIGHT_NAMES`].
fn capture_to_kind(idx: usize) -> &'static str {
    match HIGHLIGHT_NAMES.get(idx).copied() {
        Some("attribute") => "macro",
        Some("comment" | "comment.documentation") => "comment",
        Some("constant" | "constant.builtin" | "keyword" | "variable.builtin") => "keyword",
        Some("constructor" | "type") => "class",
        Some("type.builtin") => "type",
        Some("function.method") => "function",
        Some("number") => "number",
        Some("property") => "property",
        Some("string" | "string.escape") => "string",
        _ => "normal",
    }
}

static CONFIG: OnceLock<HighlightConfiguration> = OnceLock::new();

fn config() -> &'static HighlightConfiguration {
    CONFIG.get_or_init(|| {
        let mut cfg = HighlightConfiguration::new(
            tree_sitter_java::LANGUAGE.into(),
            "java",
            include_str!("../queries/highlights.scm"),
            "",
            "",
        )
        .expect("tree-sitter-java highlight query");
        cfg.configure(HIGHLIGHT_NAMES);
        cfg
    })
}

thread_local! {
    static HIGHLIGHTER: std::cell::RefCell<Highlighter> =
        std::cell::RefCell::new(Highlighter::new());
}

/// Tokens of each line of `source` (one vector per line, line breaks excluded).
pub fn tokenize(source: &str) -> Vec<Vec<(String, &'static str)>> {
    let line_count = source.split('\n').count();
    let mut lines: Vec<Vec<(String, &'static str)>> = vec![Vec::new(); line_count];
    let events = HIGHLIGHTER.with(|cell| {
        let mut hl = cell.borrow_mut();
        hl.highlight(config(), source.as_bytes(), None, |_| None)
            .and_then(|events| events.collect::<Result<Vec<_>, _>>())
    });
    let Ok(events) = events else {
        // Unparsable (cancelled, too deep): plain text.
        for (line, text) in lines.iter_mut().zip(source.split('\n')) {
            push(line, text.trim_end_matches('\r'), "normal");
        }
        return lines;
    };

    let mut stack: Vec<&'static str> = Vec::new();
    let mut line = 0;
    for event in events {
        match event {
            HighlightEvent::HighlightStart(h) => stack.push(capture_to_kind(h.0)),
            HighlightEvent::HighlightEnd => {
                stack.pop();
            }
            HighlightEvent::Source { start, end } => {
                let kind = stack.last().copied().unwrap_or("normal");
                let Some(text) = source.get(start..end) else {
                    continue;
                };
                for (n, piece) in text.split('\n').enumerate() {
                    if n > 0 {
                        line += 1;
                    }
                    if let Some(tokens) = lines.get_mut(line) {
                        push(tokens, piece.trim_end_matches('\r'), kind);
                    }
                }
            }
        }
    }
    lines
}

/// Add a token, merged with the previous one of the same kind.
fn push(line: &mut Vec<(String, &'static str)>, text: &str, kind: &'static str) {
    if text.is_empty() {
        return;
    }
    match line.last_mut() {
        Some((prev, k)) if *k == kind => prev.push_str(text),
        _ => line.push((text.to_string(), kind)),
    }
}

// ── Hover ─────────────────────────────────────────────────────────────────────

/// Signature and Javadoc of the type or method named `word` declared in
/// `content`.
pub fn hover(word: &str, content: &str) -> Option<String> {
    if word.is_empty()
        || !word
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
    {
        return None;
    }
    let lines: Vec<&str> = content.lines().collect();
    let (n, signature) = lines
        .iter()
        .enumerate()
        .find_map(|(n, l)| declaration(l.trim(), word).map(|sig| (n, sig)))?;
    let mut doc = format!("```java\n{signature}\n```");
    if let Some(javadoc) = javadoc_above(&lines, n) {
        doc.push_str("\n\n");
        doc.push_str(&javadoc);
    }
    Some(doc)
}

/// The signature declared by `line` when it declares the type or method
/// `word`.
fn declaration(line: &str, word: &str) -> Option<String> {
    if line.starts_with("//") || line.starts_with('*') || line.starts_with("/*") {
        return None;
    }
    let ident = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let declares = line.match_indices(word).any(|(i, _)| {
        let before = line[..i].chars().next_back();
        let after = line[i + word.len()..].chars().next();
        if before.is_some_and(ident) || after.is_some_and(ident) {
            return false; // part of a longer identifier
        }
        let head = line[..i].trim_end();
        let rest = line[i + word.len()..].trim_start();
        let last_word = head
            .rsplit(|c: char| !ident(c) && c != '@')
            .next()
            .unwrap_or("");
        let is_type = matches!(
            last_word,
            "class" | "interface" | "enum" | "record" | "@interface"
        );
        // A method declaration: a return type (or modifiers) before the
        // name, and not a call (`x.f(`, `= f(`, `return f(`, `new F(`…).
        let is_method = rest.starts_with('(')
            && !head.is_empty()
            && !head.ends_with(['.', '=', '(', ',', '!', '&', '|', '+', '-', '?', ':'])
            && !head.contains('=')
            && !matches!(
                last_word,
                "new"
                    | "return"
                    | "throw"
                    | "else"
                    | "if"
                    | "while"
                    | "for"
                    | "switch"
                    | "catch"
                    | "synchronized"
                    | "yield"
                    | "assert"
            );
        is_type || is_method
    });
    declares.then(|| {
        line.trim_end_matches('{')
            .trim_end_matches(';')
            .trim_end()
            .to_string()
    })
}

/// The `/** … */` comment ending right above line `n` (annotations in
/// between are skipped), as Markdown.
fn javadoc_above(lines: &[&str], n: usize) -> Option<String> {
    let mut end = n;
    while end > 0 && lines[end - 1].trim().starts_with('@') {
        end -= 1;
    }
    if end == 0 || !lines[end - 1].trim().ends_with("*/") {
        return None;
    }
    let start = (0..end)
        .rev()
        .find(|&i| lines[i].trim().starts_with("/**"))?;
    let text: Vec<String> = lines[start..end]
        .iter()
        .map(|raw| {
            let l = raw.trim();
            let l = l.strip_prefix("/**").unwrap_or(l);
            let l = l.strip_suffix("*/").unwrap_or(l).trim();
            javadoc_line(l.strip_prefix('*').unwrap_or(l).trim())
        })
        .collect();
    let doc = text.join("\n").trim().to_string();
    (!doc.is_empty()).then_some(doc)
}

/// One Javadoc line as Markdown: block tags as labels, `{@code x}` as code.
fn javadoc_line(line: &str) -> String {
    let line = inline_tags(line);
    let tag = |name: &str| line.strip_prefix(name).map(str::trim_start);
    let named = |rest: &str| {
        let (name, desc) = rest.split_once(' ').unwrap_or((rest, ""));
        format!("`{name}` {}", desc.trim())
    };
    if let Some(rest) = tag("@param") {
        format!("- **param** {}", named(rest))
    } else if let Some(rest) = tag("@return") {
        format!("- **returns** {rest}")
    } else if let Some(rest) = tag("@throws").or_else(|| tag("@exception")) {
        format!("- **throws** {}", named(rest))
    } else if let Some(rest) = tag("@see") {
        format!("- **see** {rest}")
    } else if let Some(rest) = tag("@deprecated") {
        format!("- **deprecated** {rest}")
    } else if let Some(rest) = tag("@since") {
        format!("- **since** {rest}")
    } else {
        line
    }
}

/// `{@code x}` / `{@link X}` / `{@literal x}` → `` `x` ``; `<p>` dropped.
fn inline_tags(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(start) = rest.find("{@") {
        let Some(end) = rest[start..].find('}') else {
            break;
        };
        out.push_str(&rest[..start]);
        let inner = &rest[start + 2..start + end];
        let body = inner.split_once(' ').map_or("", |(_, b)| b).trim();
        out.push_str(&format!("`{body}`"));
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    out.replace("<p>", "").replace("</p>", "")
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

fn line_to_json(tokens: &[(String, &'static str)]) -> String {
    let toks: Vec<String> = tokens
        .iter()
        .map(|(text, kind)| format!(r#"{{"text":{},"kind":"{kind}"}}"#, json_escape(text)))
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
    c"java".as_ptr()
}

#[no_mangle]
pub extern "C" fn file_extensions() -> *const c_char {
    c"java".as_ptr()
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

    type Lines = Vec<Vec<(String, &'static str)>>;

    /// Kind of the first token equal to `text` on line `n`.
    fn kind_of(lines: &Lines, n: usize, text: &str) -> &'static str {
        lines[n]
            .iter()
            .find(|(t, _)| t.trim() == text)
            .map(|(_, k)| *k)
            .unwrap_or_else(|| panic!("{text:?} not a token of line {n}: {:?}", lines[n]))
    }

    const SAMPLE: &str = r#"package com.acme;

import java.util.List;

/** A greeter. */
@Deprecated
public final class Greeter<T> implements Runnable {
    private static final int MAX_COUNT = 3;
    private final String name;

    public Greeter(String name) { this.name = name; }

    @Override
    public void run() {
        var list = new ArrayList<String>();
        System.out.println("hi\n" + name + 'c' + 0x1F + 2.5f);
        list.add(null); // done
        boolean ok = true;
    }
}

record Point(int x, int y) {}
enum Color { RED, GREEN }
"#;

    #[test]
    fn lines_round_trip_their_text() {
        let src = "class A {\r\n  /* a\r\n  b */ int x;\r\n}";
        let lines = tokenize(src);
        let expected: Vec<&str> = src.split('\n').map(|l| l.trim_end_matches('\r')).collect();
        assert_eq!(lines.len(), expected.len());
        for (line, text) in lines.iter().zip(expected) {
            let joined: String = line.iter().map(|(t, _)| t.as_str()).collect();
            assert_eq!(joined, text);
        }
    }

    #[test]
    fn highlights_declarations_and_expressions() {
        let l = tokenize(SAMPLE);
        assert_eq!(kind_of(&l, 0, "package"), "keyword");
        assert_eq!(kind_of(&l, 4, "/** A greeter. */"), "comment");
        assert_eq!(kind_of(&l, 5, "@Deprecated"), "macro");
        assert_eq!(kind_of(&l, 6, "Greeter"), "class");
        assert_eq!(kind_of(&l, 6, "Runnable"), "class");
        assert_eq!(kind_of(&l, 7, "int"), "type");
        assert_eq!(
            kind_of(&l, 7, "MAX_COUNT"),
            "keyword",
            "constants look the same everywhere"
        );
        assert_eq!(kind_of(&l, 7, "3"), "number");
        assert_eq!(kind_of(&l, 8, "name"), "property");
        assert_eq!(kind_of(&l, 10, "Greeter"), "class", "constructor");
        assert_eq!(kind_of(&l, 10, "this"), "keyword");
        assert_eq!(kind_of(&l, 13, "run"), "function");
        assert_eq!(kind_of(&l, 14, "var"), "keyword");
        assert_eq!(kind_of(&l, 14, "ArrayList"), "class");
        assert_eq!(kind_of(&l, 15, "System"), "class");
        assert_eq!(kind_of(&l, 15, "out"), "property");
        assert_eq!(kind_of(&l, 15, "println"), "function");
        assert_eq!(kind_of(&l, 15, "'c'"), "string");
        assert_eq!(kind_of(&l, 15, "0x1F"), "number");
        assert_eq!(kind_of(&l, 16, "null"), "keyword");
        assert_eq!(kind_of(&l, 16, "// done"), "comment");
        assert_eq!(kind_of(&l, 17, "boolean"), "type");
        assert_eq!(kind_of(&l, 17, "true"), "keyword");
        assert_eq!(kind_of(&l, 21, "Point"), "class");
        assert_eq!(kind_of(&l, 22, "RED"), "keyword");
    }

    #[test]
    fn string_escapes_stay_strings() {
        let l = tokenize(r#"String s = "a\tb";"#);
        let joined: String = l[0]
            .iter()
            .filter(|(_, k)| *k == "string")
            .map(|(t, _)| t.as_str())
            .collect();
        assert_eq!(joined, r#""a\tb""#);
    }

    #[test]
    fn multi_line_comments_and_text_blocks() {
        let l = tokenize("/*\n * x\n */\nString s = \"\"\"\n  text\n  \"\"\";");
        assert_eq!(kind_of(&l, 1, "* x"), "comment");
        assert_eq!(kind_of(&l, 4, "text"), "string");
    }

    #[test]
    fn hover_shows_signature_and_javadoc() {
        let src = r#"
/**
 * Says hello.
 * <p>Uses {@code name}.
 * @param name who to greet
 * @return the greeting
 * @throws IllegalStateException when closed
 */
@Override
public String greet(String name) throws IllegalStateException {
    return greet(name);
}

/** Shapes. */
public sealed interface Shape permits Circle {}
public class Circle implements Shape {}
"#;
        let h = hover("greet", src).unwrap();
        assert!(h.starts_with(
            "```java\npublic String greet(String name) throws IllegalStateException\n```"
        ));
        assert!(h.contains("Says hello."), "{h}");
        assert!(h.contains("Uses `name`."), "{h}");
        assert!(h.contains("- **param** `name` who to greet"), "{h}");
        assert!(h.contains("- **returns** the greeting"), "{h}");
        assert!(
            h.contains("- **throws** `IllegalStateException` when closed"),
            "{h}"
        );

        let shape = hover("Shape", src).unwrap();
        assert!(
            shape.contains("public sealed interface Shape permits Circle {}"),
            "{shape}"
        );
        assert!(shape.ends_with("Shapes."), "{shape}");
        assert_eq!(
            hover("Circle", src).unwrap(),
            "```java\npublic class Circle implements Shape {}\n```"
        );
    }

    #[test]
    fn hover_ignores_calls_and_unknown_words() {
        let src = "class A {\n  void f() {\n    g(1);\n    x = h(2);\n    return k();\n    new B();\n  }\n}";
        assert!(hover("f", src).is_some());
        for w in ["g", "h", "k", "B", "missing", "", "a b"] {
            assert!(hover(w, src).is_none(), "{w:?}");
        }
    }

    #[test]
    fn ffi_round_trip() {
        unsafe {
            let src = CString::new("int x = 1;").unwrap();
            let out = tokenize_document_ffi(src.as_ptr());
            let json = CStr::from_ptr(out).to_str().unwrap().to_string();
            free_string(out);
            assert_eq!(
                json,
                r#"[[{"text":"int","kind":"type"},{"text":" x = ","kind":"normal"},{"text":"1","kind":"number"},{"text":";","kind":"normal"}]]"#
            );

            let line = CString::new("return \"q\";").unwrap();
            let out = tokenize_line_ffi(line.as_ptr());
            assert!(CStr::from_ptr(out)
                .to_str()
                .unwrap()
                .contains(r#"{"text":"\"q\"","kind":"string"}"#));
            free_string(out);

            let word = CString::new("A").unwrap();
            let doc = CString::new("class A {}").unwrap();
            let out = hover_info_ffi(word.as_ptr(), doc.as_ptr());
            assert!(!out.is_null());
            free_string(out);
            assert!(hover_info_ffi(word.as_ptr(), std::ptr::null()).is_null());

            assert_eq!(CStr::from_ptr(language_id()).to_str().unwrap(), "java");
            assert_eq!(CStr::from_ptr(file_extensions()).to_str().unwrap(), "java");
            free_string(std::ptr::null_mut());
        }
    }
}
