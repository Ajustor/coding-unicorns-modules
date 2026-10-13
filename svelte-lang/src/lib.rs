#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Keyword,
    KeywordType,
    String,
    Comment,
    Number,
    Function,
    Macro,
    Normal,
}

impl TokenKind {
    pub fn color_category(&self) -> &'static str {
        match self {
            TokenKind::Keyword => "keyword",
            TokenKind::KeywordType => "type",
            TokenKind::String => "string",
            TokenKind::Comment => "comment",
            TokenKind::Number => "number",
            TokenKind::Function => "function",
            TokenKind::Macro => "macro",
            TokenKind::Normal => "normal",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Token {
    pub text: String,
    pub kind: TokenKind,
}

const KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "case", "catch", "class", "const",
    "continue", "default", "delete", "do", "else", "export", "extends",
    "false", "finally", "for", "from", "function", "if", "import", "in",
    "instanceof", "let", "new", "null", "of", "return", "static", "super",
    "switch", "this", "throw", "true", "try", "type", "typeof", "undefined",
    "var", "void", "while", "with", "yield",
];

const TYPE_KEYWORDS: &[&str] = &[
    "any", "boolean", "never", "number", "object", "string", "unknown", "void",
];

// Svelte-specific directives and block tags (highlighted as macros)
const SVELTE_DIRECTIVES: &[&str] = &[
    "each", "if", "else", "await", "then", "catch", "key", "snippet", "render",
    "script", "style", "template",
    // Svelte 5 runes
    "props", "state", "derived", "effect", "bindable",
];

fn char_byte_offset(chars: &[char], char_idx: usize) -> usize {
    chars[..char_idx].iter().map(|c| c.len_utf8()).sum()
}

pub fn tokenize_line(line: &str) -> Vec<Token> {
    let trimmed = line.trim_start();

    // HTML comment
    if trimmed.starts_with("<!--") {
        return vec![Token {
            text: line.to_string(),
            kind: TokenKind::Comment,
        }];
    }
    // JS line comment
    if trimmed.starts_with("//") {
        return vec![Token {
            text: line.to_string(),
            kind: TokenKind::Comment,
        }];
    }

    let mut tokens: Vec<Token> = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let offset = char_byte_offset(&chars, i);

        // Inline JS comment
        if line[offset..].starts_with("//") {
            tokens.push(Token { text: line[offset..].to_string(), kind: TokenKind::Comment });
            break;
        }

        // Inline HTML comment
        if line[offset..].starts_with("<!--") {
            tokens.push(Token { text: line[offset..].to_string(), kind: TokenKind::Comment });
            break;
        }

        // Svelte block tags: {#if}, {#each}, {/if}, {:else}, etc.
        if chars[i] == '{' {
            let rest = &line[offset..];
            if rest.starts_with("{#") || rest.starts_with("{/") || rest.starts_with("{:") || rest.starts_with("{@") {
                let mut s = String::new();
                s.push(chars[i]);
                i += 1;
                // consume the sigil
                if i < len {
                    s.push(chars[i]);
                    i += 1;
                }
                // consume the directive word
                let start = i;
                while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                s.push_str(&word);
                tokens.push(Token { text: s, kind: TokenKind::Macro });
                continue;
            }
        }

        // String literal: ", ', or `
        if chars[i] == '"' || chars[i] == '\'' || chars[i] == '`' {
            let quote = chars[i];
            let mut s = String::new();
            s.push(quote);
            i += 1;
            while i < len {
                let c = chars[i];
                s.push(c);
                i += 1;
                if c == '\\' && i < len {
                    s.push(chars[i]);
                    i += 1;
                } else if c == quote {
                    break;
                }
            }
            tokens.push(Token { text: s, kind: TokenKind::String });
            continue;
        }

        // Number
        if chars[i].is_ascii_digit() {
            let mut s = String::new();
            while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '.' || chars[i] == '_') {
                s.push(chars[i]);
                i += 1;
            }
            tokens.push(Token { text: s, kind: TokenKind::Number });
            continue;
        }

        // Svelte 5 rune: $state, $derived, $effect, $props, $bindable
        if chars[i] == '$' && i + 1 < len && chars[i + 1].is_alphabetic() {
            let mut s = String::new();
            s.push('$');
            i += 1;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                s.push(chars[i]);
                i += 1;
            }
            tokens.push(Token { text: s, kind: TokenKind::Macro });
            continue;
        }

        // Word (identifier, keyword, directive)
        if chars[i].is_alphabetic() || chars[i] == '_' {
            let start = i;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let kind = if SVELTE_DIRECTIVES.contains(&word.as_str()) && i < len && chars[i] != '(' {
                TokenKind::Macro
            } else if TYPE_KEYWORDS.contains(&word.as_str()) {
                TokenKind::KeywordType
            } else if KEYWORDS.contains(&word.as_str()) {
                TokenKind::Keyword
            } else if i < len && chars[i] == '(' {
                TokenKind::Function
            } else {
                TokenKind::Normal
            };
            tokens.push(Token { text: word, kind });
            continue;
        }

        // Punctuation
        let start = i;
        while i < len
            && !chars[i].is_alphabetic()
            && chars[i] != '_'
            && !chars[i].is_ascii_digit()
            && chars[i] != '"'
            && chars[i] != '\''
            && chars[i] != '`'
            && chars[i] != '$'
            && !line[char_byte_offset(&chars, i)..].starts_with("//")
            && !line[char_byte_offset(&chars, i)..].starts_with("<!--")
        {
            i += 1;
        }
        if i > start {
            let punct: String = chars[start..i].iter().collect();
            if !punct.is_empty() {
                tokens.push(Token { text: punct, kind: TokenKind::Normal });
            }
        }
    }

    tokens
}

// ── FFI interface ─────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn language_id() -> *const std::ffi::c_char {
    c"svelte".as_ptr()
}

#[no_mangle]
pub extern "C" fn file_extensions() -> *const std::ffi::c_char {
    c"svelte".as_ptr()
}

#[no_mangle]
pub extern "C" fn reset_tokenizer() {}

/// Highlight the whole component: markup as HTML, `<script>` as TypeScript,
/// `<style>` as CSS, template expressions as TypeScript. Preferred by the IDE
/// over [`tokenize_line_ffi`], kept for older versions.
///
/// # Safety
/// `text_ptr` must be a valid, nul-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn tokenize_document_ffi(
    text_ptr: *const std::ffi::c_char,
) -> *mut std::ffi::c_char {
    let text = unsafe { std::ffi::CStr::from_ptr(text_ptr) }.to_str().unwrap_or("");
    let json = unicorns_highlight::component(text, unicorns_highlight::Component::Svelte);
    std::ffi::CString::new(json).unwrap_or_default().into_raw()
}

/// # Safety
/// `line_ptr` must be a valid, nul-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn tokenize_line_ffi(
    line_ptr: *const std::ffi::c_char,
) -> *mut std::ffi::c_char {
    let line = unsafe { std::ffi::CStr::from_ptr(line_ptr).to_str().unwrap_or("") };
    let tokens = tokenize_line(line);
    let json = tokens_to_json(&tokens);
    let c_str = std::ffi::CString::new(json).unwrap();
    c_str.into_raw()
}

/// # Safety
/// `ptr` must have been returned by `tokenize_line_ffi`.
#[no_mangle]
pub unsafe extern "C" fn free_string(ptr: *mut std::ffi::c_char) {
    if !ptr.is_null() {
        unsafe { drop(std::ffi::CString::from_raw(ptr)) }
    }
}

pub fn hover_info(word: &str, file_content: &str) -> Option<String> {
    let fn_patterns = [
        format!("function {}(", word),
        format!("function {} (", word),
        format!("async function {}(", word),
        format!("const {} = (", word),
        format!("const {} = async", word),
    ];
    let type_patterns = [
        format!("interface {} ", word),
        format!("interface {}{}", word, '{'),
        format!("type {} =", word),
        format!("class {} ", word),
        format!("class {}{}", word, '{'),
    ];
    let var_patterns = [
        format!("const {}", word),
        format!("let {}", word),
        format!("var {}", word),
    ];

    for line in file_content.lines() {
        let trimmed = line.trim();
        for pat in &fn_patterns {
            if trimmed.contains(pat.as_str()) {
                let sig = trimmed.trim_end_matches('{').trim_end();
                return Some(format!("```svelte\n{sig}\n```"));
            }
        }
        for pat in &type_patterns {
            if trimmed.contains(pat.as_str()) {
                let sig = trimmed.trim_end_matches('{').trim_end();
                return Some(format!("```svelte\n{sig}\n```"));
            }
        }
        for pat in &var_patterns {
            if trimmed.starts_with(pat.as_str()) {
                let end = trimmed.find('=').unwrap_or(trimmed.len());
                let sig = trimmed[..end].trim_end();
                return Some(format!("```svelte\n{sig}\n```"));
            }
        }
    }
    None
}

/// # Safety
/// Both pointer arguments must be valid nul-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn hover_info_ffi(
    word_ptr: *const std::ffi::c_char,
    file_content_ptr: *const std::ffi::c_char,
) -> *mut std::ffi::c_char {
    let word = unsafe { std::ffi::CStr::from_ptr(word_ptr).to_str().unwrap_or("") };
    let content = unsafe { std::ffi::CStr::from_ptr(file_content_ptr).to_str().unwrap_or("") };
    match hover_info(word, content) {
        Some(s) => std::ffi::CString::new(s).map(|c| c.into_raw()).unwrap_or(std::ptr::null_mut()),
        None => std::ptr::null_mut(),
    }
}

fn tokens_to_json(tokens: &[Token]) -> String {
    let parts: Vec<String> = tokens
        .iter()
        .map(|t| {
            format!(
                r#"{{"text":{},"kind":"{}"}}"#,
                serde_json_escape(&t.text),
                t.kind.color_category()
            )
        })
        .collect();
    format!("[{}]", parts.join(","))
}

fn serde_json_escape(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    format!("\"{}\"", escaped)
}

pub fn lsp_server_command() -> Option<(String, Vec<String>)> {
    Some(("svelte-language-server".to_string(), vec!["--stdio".to_string()]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyword() {
        let tokens = tokenize_line("const x = 1;");
        assert!(tokens.iter().any(|t| t.text == "const" && t.kind == TokenKind::Keyword));
    }

    #[test]
    fn test_html_comment() {
        let tokens = tokenize_line("<!-- comment -->");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Comment);
    }

    #[test]
    fn test_js_comment() {
        let tokens = tokenize_line("// js comment");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Comment);
    }

    #[test]
    fn test_svelte_block_tag() {
        let tokens = tokenize_line("{#if condition}");
        assert!(tokens.iter().any(|t| t.text == "{#if" && t.kind == TokenKind::Macro));
    }

    #[test]
    fn test_svelte_rune() {
        let tokens = tokenize_line("let count = $state(0);");
        assert!(tokens.iter().any(|t| t.text == "$state" && t.kind == TokenKind::Macro));
    }

    #[test]
    fn test_string_backtick() {
        let tokens = tokenize_line("const s = `hello`");
        assert!(tokens.iter().any(|t| t.text == "`hello`" && t.kind == TokenKind::String));
    }

    #[test]
    fn test_number() {
        let tokens = tokenize_line("let x = 42;");
        assert!(tokens.iter().any(|t| t.text == "42" && t.kind == TokenKind::Number));
    }
}
