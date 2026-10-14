#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind { Keyword, String, Comment, Number, Macro, Normal, Property }

impl TokenKind {
    pub fn color_category(&self) -> &'static str {
        match self {
            TokenKind::Keyword => "keyword", TokenKind::String => "string",
            TokenKind::Comment => "comment", TokenKind::Number => "number",
            TokenKind::Macro => "macro", TokenKind::Normal => "normal",
            TokenKind::Property => "property",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Token { pub text: String, pub kind: TokenKind }

/// What a line starts inside of, left open by the line before.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    Text,
    /// A start tag whose attributes go on (`<project` then `xmlns:…` lines).
    Tag,
    Comment,
    ProcessingInstruction,
    CData,
}

pub fn tokenize_line(line: &str) -> Vec<Token> {
    tokenize_line_in(line, State::Text).0
}

/// Token lines of a whole document: tags, comments, processing instructions
/// and CDATA sections may span several lines.
pub fn tokenize_document(source: &str) -> Vec<Vec<Token>> {
    let mut state = State::Text;
    source
        .lines()
        .map(|line| {
            let (tokens, next) = tokenize_line_in(line, state);
            state = next;
            tokens
        })
        .collect()
}

/// Index just past `end` from `i`, or `None` when the line ends first.
fn find_end(chars: &[char], i: usize, end: &str) -> Option<usize> {
    let end: Vec<char> = end.chars().collect();
    (i..=chars.len().saturating_sub(end.len()))
        .find(|&j| chars[j..j + end.len()] == end[..])
        .map(|j| j + end.len())
}

/// A comment, processing instruction or CDATA section from `start` (its
/// opening at `start`, or the line start when it began above): the token,
/// and the state after it.
fn delimited(
    chars: &[char],
    start: usize,
    from: usize,
    end: &str,
    kind: TokenKind,
    open: State,
    tokens: &mut Vec<Token>,
) -> (usize, State) {
    let (stop, state) = match find_end(chars, from, end) {
        Some(stop) => (stop, State::Text),
        None => (chars.len(), open),
    };
    if stop > start {
        tokens.push(Token { text: chars[start..stop].iter().collect(), kind });
    }
    (stop, state)
}

/// The attributes of a start tag from `i`, up to and with its `>` or `/>`.
/// Returns where it stopped and whether the tag is closed.
fn attributes(chars: &[char], mut i: usize, tokens: &mut Vec<Token>) -> (usize, bool) {
    let len = chars.len();
    while i < len && chars[i] != '>' {
        if chars[i].is_whitespace() {
            let ws_start = i;
            while i < len && chars[i].is_whitespace() { i += 1; }
            tokens.push(Token { text: chars[ws_start..i].iter().collect(), kind: TokenKind::Normal });
            continue;
        }
        if chars[i] == '/' && i + 1 < len && chars[i + 1] == '>' {
            tokens.push(Token { text: "/>".to_string(), kind: TokenKind::Keyword });
            return (i + 2, true);
        }
        if chars[i].is_alphabetic() || chars[i] == '_' || chars[i] == ':' {
            let attr_start = i;
            while i < len && (chars[i].is_alphanumeric() || matches!(chars[i], '_' | '-' | ':' | '.')) {
                i += 1;
            }
            tokens.push(Token { text: chars[attr_start..i].iter().collect(), kind: TokenKind::Property });
            continue;
        }
        if chars[i] == '"' || chars[i] == '\'' {
            let quote = chars[i];
            let value_start = i;
            i += 1;
            while i < len && chars[i] != quote { i += 1; }
            if i < len { i += 1; }
            tokens.push(Token { text: chars[value_start..i].iter().collect(), kind: TokenKind::String });
            continue;
        }
        tokens.push(Token { text: chars[i].to_string(), kind: TokenKind::Normal });
        i += 1;
    }
    if i < len {
        tokens.push(Token { text: ">".to_string(), kind: TokenKind::Keyword });
        return (i + 1, true);
    }
    (i, false)
}

/// Tokens of `line`, which starts inside `state`, and the state it leaves
/// open for the next line.
pub fn tokenize_line_in(line: &str, mut state: State) -> (Vec<Token>, State) {
    let mut tokens: Vec<Token> = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0;

    // Carry on with what the line before left open.
    match state {
        State::Text => {}
        State::Tag => {
            let (next, closed) = attributes(&chars, 0, &mut tokens);
            i = next;
            if closed { state = State::Text; }
        }
        State::Comment => (i, state) = delimited(&chars, 0, 0, "-->", TokenKind::Comment, state, &mut tokens),
        State::ProcessingInstruction => {
            (i, state) = delimited(&chars, 0, 0, "?>", TokenKind::Macro, state, &mut tokens)
        }
        State::CData => (i, state) = delimited(&chars, 0, 0, "]]>", TokenKind::Macro, state, &mut tokens),
    }

    while i < len && state == State::Text {
        let rest: String = chars[i..len.min(i + 9)].iter().collect();
        if rest.starts_with("<!--") {
            (i, state) = delimited(&chars, i, i + 4, "-->", TokenKind::Comment, State::Comment, &mut tokens);
            continue;
        }
        if rest.starts_with("<?") {
            (i, state) = delimited(&chars, i, i + 2, "?>", TokenKind::Macro, State::ProcessingInstruction, &mut tokens);
            continue;
        }
        if rest.starts_with("<![CDATA[") {
            (i, state) = delimited(&chars, i, i + 9, "]]>", TokenKind::Macro, State::CData, &mut tokens);
            continue;
        }

        // Tag: <name …>, </name>, <!DOCTYPE …>
        if chars[i] == '<' {
            let mut s = String::from('<');
            i += 1;
            if i < len && chars[i] == '/' { s.push('/'); i += 1; }
            if i < len && chars[i] == '!' {
                while i < len && chars[i] != '>' { s.push(chars[i]); i += 1; }
                if i < len { s.push('>'); i += 1; }
                tokens.push(Token { text: s, kind: TokenKind::Macro });
                continue;
            }
            while i < len && (chars[i].is_alphanumeric() || matches!(chars[i], ':' | '-' | '_' | '.')) {
                s.push(chars[i]);
                i += 1;
            }
            tokens.push(Token { text: s, kind: TokenKind::Keyword });
            let (next, closed) = attributes(&chars, i, &mut tokens);
            i = next;
            if !closed { state = State::Tag; }
            continue;
        }

        // Entity: &amp; &#123;
        if chars[i] == '&' {
            let start = i;
            i += 1;
            while i < len && chars[i] != ';' && !chars[i].is_whitespace() { i += 1; }
            if i < len && chars[i] == ';' { i += 1; }
            tokens.push(Token { text: chars[start..i].iter().collect(), kind: TokenKind::Number });
            continue;
        }

        // Text content
        let start = i;
        while i < len && chars[i] != '<' && chars[i] != '&' { i += 1; }
        if i > start {
            tokens.push(Token { text: chars[start..i].iter().collect(), kind: TokenKind::Normal });
        }
    }

    if tokens.is_empty() && !line.is_empty() {
        tokens.push(Token { text: line.to_string(), kind: TokenKind::Normal });
    }
    (tokens, state)
}

// ── FFI ──────────────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn language_id() -> *const std::ffi::c_char { c"xml".as_ptr() }

#[no_mangle]
pub extern "C" fn file_extensions() -> *const std::ffi::c_char { c"xml,xsl,xsd,svg,xhtml".as_ptr() }

#[no_mangle]
pub unsafe extern "C" fn tokenize_line_ffi(line_ptr: *const std::ffi::c_char) -> *mut std::ffi::c_char {
    let line = unsafe { std::ffi::CStr::from_ptr(line_ptr).to_str().unwrap_or("") };
    let tokens = tokenize_line(line);
    let json = tokens_to_json(&tokens);
    std::ffi::CString::new(json).unwrap().into_raw()
}

/// The whole document (`[[{text,kind},…],…]`): what spans lines keeps its
/// color. Freed with [`free_string`].
///
/// # Safety
/// `text_ptr` must be null or a valid NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn tokenize_document_ffi(text_ptr: *const std::ffi::c_char) -> *mut std::ffi::c_char {
    if text_ptr.is_null() {
        return std::ptr::null_mut();
    }
    let text = unsafe { std::ffi::CStr::from_ptr(text_ptr).to_str().unwrap_or("") };
    let lines: Vec<String> = tokenize_document(text).iter().map(|l| tokens_to_json(l)).collect();
    std::ffi::CString::new(format!("[{}]", lines.join(","))).unwrap().into_raw()
}

#[no_mangle]
pub unsafe extern "C" fn free_string(ptr: *mut std::ffi::c_char) {
    if !ptr.is_null() { unsafe { drop(std::ffi::CString::from_raw(ptr)) } }
}

pub fn hover_info(_word: &str, _file_content: &str) -> Option<String> { None }

#[no_mangle]
pub unsafe extern "C" fn hover_info_ffi(_w: *const std::ffi::c_char, _c: *const std::ffi::c_char) -> *mut std::ffi::c_char {
    std::ptr::null_mut()
}

pub fn lsp_server_command() -> Option<(String, Vec<String>)> {
    Some(("lemminx".to_string(), vec![]))
}

fn tokens_to_json(tokens: &[Token]) -> String {
    let parts: Vec<String> = tokens.iter().map(|t| {
        format!(r#"{{"text":{},"kind":"{}"}}"#, serde_json_escape(&t.text), t.kind.color_category())
    }).collect();
    format!("[{}]", parts.join(","))
}

fn serde_json_escape(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\r', "\\r").replace('\t', "\\t");
    format!("\"{}\"", escaped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_comment() {
        let tokens = tokenize_line("<!-- comment -->");
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Comment));
    }

    #[test]
    fn test_tag() {
        let tokens = tokenize_line("<div>");
        assert!(tokens.iter().any(|t| t.text.contains("div") && t.kind == TokenKind::Keyword));
    }

    #[test]
    fn test_attribute() {
        let tokens = tokenize_line(r#"<div class="main">"#);
        assert!(tokens.iter().any(|t| t.text == "class" && t.kind == TokenKind::Property));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::String));
    }

    #[test]
    fn test_processing_instruction() {
        let tokens = tokenize_line("<?xml version=\"1.0\"?>");
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Macro));
    }

    #[test]
    fn test_entity() {
        let tokens = tokenize_line("&amp;");
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Number));
    }

    #[test]
    fn spans_lines() {
        let doc = tokenize_document("<project a=\"1\"
  b='2'>
<!-- one
two -->
<x/>");
        assert!(doc[1].iter().any(|t| t.text == "b" && t.kind == TokenKind::Property));
        assert!(doc[1].iter().any(|t| t.text == ">" && t.kind == TokenKind::Keyword));
        assert_eq!(doc[3][0].kind, TokenKind::Comment);
        assert_eq!(doc[3][0].text, "two -->");
        assert_eq!(doc[4][0].text, "<x");
    }

    #[test]
    fn test_self_closing() {
        let tokens = tokenize_line("<br/>");
        assert!(tokens.iter().any(|t| t.text == "/>" && t.kind == TokenKind::Keyword));
    }
}
