//! Configuration files: INI and its relatives with tree-sitter-ini (`.ini`,
//! `.cfg`, `.desktop`, `.editorconfig`, `.gitconfig`, `.gitmodules`,
//! `.npmrc`), and small hand-written highlighters for ignore files
//! (`.gitignore`, `.dockerignore`…), `.gitattributes` and `.env` files,
//! which have no published grammar.

use std::ffi::{c_char, CStr, CString};
use std::ops::Range;
use std::sync::OnceLock;
use tree_sitter_highlight::HighlightConfiguration;
use unicorns_highlight::{Kind, Paint};

fn ini() -> &'static HighlightConfiguration {
    static CONFIG: OnceLock<HighlightConfiguration> = OnceLock::new();
    CONFIG.get_or_init(|| {
        // Written for Neovim: a second `@spell` capture hides the comments.
        // The grammar has a value node but its query leaves it uncolored.
        let query = tree_sitter_ini::HIGHLIGHTS_QUERY.replace(" @spell", "")
            + "\n(setting_value) @string\n";
        unicorns_highlight::configure(tree_sitter_ini::LANGUAGE.into(), "ini", &query, "")
    })
}

/// The document in language `lang` as the IDE's token lines.
pub fn document_to_json(lang: &str, source: &str) -> String {
    let mut paint = Paint::new(source);
    match lang {
        "gitignore" => each_line(source, &mut paint, ignore_line),
        "gitattributes" => each_line(source, &mut paint, attributes_line),
        "env" => each_line(source, &mut paint, env_line),
        _ => paint.highlight(ini(), 0..source.len()),
    }
    paint.to_json()
}

/// Paint each line of `source` with `line`, given the line and its offset.
fn each_line(source: &str, paint: &mut Paint, line: fn(&str, usize, &mut Paint)) {
    let mut start = 0;
    for text in source.split('\n') {
        line(text.strip_suffix('\r').unwrap_or(text), start, paint);
        start += text.len() + 1;
    }
}

/// `range` of the line at `offset`, in the document.
fn at(offset: usize, range: Range<usize>) -> Range<usize> {
    offset + range.start..offset + range.end
}

/// A full-line `#` comment: true when the line was one.
fn comment(line: &str, offset: usize, paint: &mut Paint) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') {
        let start = line.len() - trimmed.len();
        paint.fill(at(offset, start..line.len()), Kind::Comment);
        return true;
    }
    false
}

/// An ignore pattern: `!` (re-include) as a keyword, the pattern as a
/// string, its wildcards (`*`, `**`, `?`, `[…]`) as keywords.
fn ignore_line(line: &str, offset: usize, paint: &mut Paint) {
    if comment(line, offset, paint) || line.trim().is_empty() {
        return;
    }
    let mut start = line.len() - line.trim_start().len();
    if line[start..].starts_with('!') {
        paint.fill(at(offset, start..start + 1), Kind::Keyword);
        start += 1;
    }
    let end = line.trim_end().len().max(start);
    paint.fill(at(offset, start..end), Kind::String);
    wildcards(&line[start..end], offset + start, paint);
}

fn wildcards(pattern: &str, offset: usize, paint: &mut Paint) {
    let mut chars = pattern.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => {
                chars.next(); // an escaped character is literal
            }
            '*' | '?' => paint.fill(offset + i..offset + i + 1, Kind::Keyword),
            '[' => {
                let end = pattern[i..].find(']').map_or(pattern.len(), |e| i + e + 1);
                paint.fill(offset + i..offset + end, Kind::Keyword);
                while chars.peek().is_some_and(|(j, _)| *j < end) {
                    chars.next();
                }
            }
            _ => {}
        }
    }
}

/// `pattern attr -attr !attr attr=value`: the pattern like an ignore
/// pattern, attribute names as properties, `-` / `!` as keywords, values as
/// strings.
fn attributes_line(line: &str, offset: usize, paint: &mut Paint) {
    if comment(line, offset, paint) || line.trim().is_empty() {
        return;
    }
    let mut words = words(line);
    if let Some(pattern) = words.next() {
        paint.fill(at(offset, pattern.clone()), Kind::String);
        wildcards(&line[pattern.clone()], offset + pattern.start, paint);
    }
    for word in words {
        let text = &line[word.clone()];
        let mut name = word.clone();
        if text.starts_with(['-', '!']) {
            paint.fill(at(offset, word.start..word.start + 1), Kind::Keyword);
            name.start += 1;
        }
        match text.find('=') {
            Some(eq) => {
                paint.fill(at(offset, name.start..word.start + eq), Kind::Property);
                paint.fill(at(offset, word.start + eq + 1..word.end), Kind::String);
            }
            None => paint.fill(at(offset, name), Kind::Property),
        }
    }
}

/// Byte ranges of the whitespace-separated words of `line`.
fn words(line: &str) -> impl Iterator<Item = Range<usize>> + '_ {
    let mut pos = 0;
    std::iter::from_fn(move || {
        let rest = &line[pos..];
        let start = pos + (rest.len() - rest.trim_start().len());
        if start >= line.len() {
            return None;
        }
        let end = line[start..]
            .find(char::is_whitespace)
            .map_or(line.len(), |e| start + e);
        pos = end;
        Some(start..end)
    })
}

/// `export KEY=value # comment`: `export` as a keyword, the key as a
/// property, the value as a string with its `$VAR` / `${VAR}` references as
/// keywords.
fn env_line(line: &str, offset: usize, paint: &mut Paint) {
    if comment(line, offset, paint) || line.trim().is_empty() {
        return;
    }
    let mut start = line.len() - line.trim_start().len();
    if let Some(rest) = line[start..].strip_prefix("export") {
        if rest.starts_with(char::is_whitespace) {
            paint.fill(at(offset, start..start + 6), Kind::Keyword);
            start = line.len() - rest.trim_start().len();
        }
    }
    let Some(eq) = line[start..].find('=').map(|e| start + e) else {
        return;
    };
    paint.fill(
        at(offset, start..line[..eq].trim_end().len()),
        Kind::Property,
    );
    let value_start = eq + 1;
    let quoted = line[value_start..].trim_start().starts_with(['"', '\'']);
    // An unquoted value ends at ` #` (an inline comment).
    let value_end = if quoted {
        line.trim_end().len()
    } else {
        line[value_start..]
            .find(" #")
            .map_or(line.trim_end().len(), |c| value_start + c)
    };
    if value_end > value_start {
        paint.fill(at(offset, value_start..value_end), Kind::String);
    }
    if value_end < line.trim_end().len() {
        paint.fill(at(offset, value_end..line.len()), Kind::Comment);
    }
    if !line[value_start..].trim_start().starts_with('\'') {
        references(&line[value_start..value_end], offset + value_start, paint);
    }
}

fn references(value: &str, offset: usize, paint: &mut Paint) {
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            let end = if bytes.get(i + 1) == Some(&b'{') {
                value[i..].find('}').map_or(value.len(), |e| i + e + 1)
            } else {
                i + 1
                    + value[i + 1..]
                        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                        .unwrap_or(value.len() - i - 1)
            };
            if end > i + 1 {
                paint.fill(offset + i..offset + end, Kind::Keyword);
            }
            i = end.max(i + 1);
        } else {
            i += 1;
        }
    }
}

// ── FFI ───────────────────────────────────────────────────────────────────────

/// # Safety
/// `ptr` must be null or a valid, nul-terminated C string.
unsafe fn from_c<'a>(ptr: *const c_char) -> &'a str {
    if ptr.is_null() {
        return "";
    }
    unsafe { CStr::from_ptr(ptr) }.to_str().unwrap_or("")
}

fn into_c(s: String) -> *mut c_char {
    CString::new(s).unwrap_or_default().into_raw()
}

#[no_mangle]
pub extern "C" fn language_id() -> *const c_char {
    c"ini".as_ptr()
}

#[no_mangle]
pub extern "C" fn file_extensions() -> *const c_char {
    c"ini,cfg,desktop,editorconfig,gitconfig,gitignore,gitattributes,env".as_ptr()
}

#[no_mangle]
pub extern "C" fn reset_tokenizer() {}

/// The whole document in language `lang`: `[[{text,kind},…],…]`, freed with
/// [`free_string`].
///
/// # Safety
/// Both pointers must be null or valid, nul-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn tokenize_document_lang_ffi(
    lang_ptr: *const c_char,
    text_ptr: *const c_char,
) -> *mut c_char {
    let (lang, text) = unsafe { (from_c(lang_ptr), from_c(text_ptr)) };
    into_c(document_to_json(lang, text))
}

/// The whole document as INI, for IDEs without the language-aware export.
///
/// # Safety
/// `text_ptr` must be null or a valid, nul-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn tokenize_document_ffi(text_ptr: *const c_char) -> *mut c_char {
    into_c(document_to_json("ini", unsafe { from_c(text_ptr) }))
}

/// Line tokenizing is not offered: the IDE uses the document tokenizers.
#[no_mangle]
pub extern "C" fn tokenize_line_ffi(_line_ptr: *const c_char) -> *mut c_char {
    std::ptr::null_mut()
}

/// # Safety
/// `ptr` must come from this module and not be freed yet.
#[no_mangle]
pub unsafe extern "C" fn free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(unsafe { CString::from_raw(ptr) });
    }
}
