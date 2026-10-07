//! speedster-js components (`.spd`).
//!
//! A `.spd` file is a single-file component: a `<script>` in TypeScript, markup with
//! `{#if}` / `{#each}` / `{#key}` / `{#snippet}` blocks and `on:` / `bind:` … directives,
//! and a `<style>` in CSS. The IDE tokenizes line by line without state, so the three
//! sections share one lexer whose rules don't contradict each other.

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

// TypeScript keywords used inside the <script> block and markup expressions
const KEYWORDS: &[&str] = &[
    "abstract", "as", "async", "await", "break", "case", "catch", "class",
    "const", "continue", "declare", "default", "delete", "do", "else", "enum",
    "export", "extends", "false", "finally", "for", "from", "function", "if",
    "implements", "import", "in", "instanceof", "interface", "keyof", "let",
    "new", "null", "of", "private", "protected", "public", "readonly",
    "return", "satisfies", "static", "super", "switch", "this", "throw",
    "true", "try", "type", "typeof", "undefined", "var", "void", "while",
    "yield",
];

const TYPE_KEYWORDS: &[&str] = &[
    "any", "bigint", "boolean", "never", "number", "object", "string",
    "symbol", "unknown",
];

// The eight directive prefixes the compiler knows (`on:click|preventDefault`, …)
const DIRECTIVES: &[&str] = &[
    "on", "bind", "class", "style", "use", "transition", "in", "out",
];

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// Pushes a token, merging consecutive plain text so punctuation doesn't explode
/// into one token per character.
fn push(tokens: &mut Vec<Token>, text: String, kind: TokenKind) {
    if kind == TokenKind::Normal {
        if let Some(last) = tokens.last_mut() {
            if last.kind == TokenKind::Normal {
                last.text.push_str(&text);
                return;
            }
        }
    }
    tokens.push(Token { text, kind });
}

fn collect(chars: &[char], from: usize, to: usize) -> String {
    chars[from..to].iter().collect()
}

fn starts_with(chars: &[char], i: usize, pat: &str) -> bool {
    (i..).zip(pat.chars()).all(|(j, p)| chars.get(j) == Some(&p))
}

/// Index just past `end`, or the end of the line when it isn't closed there.
fn find_end(chars: &[char], from: usize, end: &str) -> usize {
    let mut j = from;
    while j < chars.len() {
        if starts_with(chars, j, end) {
            return j + end.chars().count();
        }
        j += 1;
    }
    chars.len()
}

pub fn tokenize_line(line: &str) -> Vec<Token> {
    let mut tokens: Vec<Token> = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let c = chars[i];
        let prev = if i > 0 { Some(chars[i - 1]) } else { None };

        // Comments: `<!-- -->` in markup, `//` and `/* */` in script and style.
        // `//` right after `:` is a URL in text, not a comment.
        if starts_with(&chars, i, "<!--") {
            let end = find_end(&chars, i + 4, "-->");
            push(&mut tokens, collect(&chars, i, end), TokenKind::Comment);
            i = end;
            continue;
        }
        if starts_with(&chars, i, "/*") {
            let end = find_end(&chars, i + 2, "*/");
            push(&mut tokens, collect(&chars, i, end), TokenKind::Comment);
            i = end;
            continue;
        }
        if starts_with(&chars, i, "//") && prev != Some(':') {
            push(&mut tokens, collect(&chars, i, len), TokenKind::Comment);
            break;
        }

        // Block tags: {#if}, {:else}, {/each}, {@html}, …
        if c == '{' && i + 1 < len && matches!(chars[i + 1], '#' | ':' | '/' | '@') {
            let mut j = i + 2;
            while j < len && chars[j].is_alphanumeric() {
                j += 1;
            }
            if j > i + 2 {
                push(&mut tokens, collect(&chars, i, j), TokenKind::Macro);
                i = j;
                continue;
            }
        }

        // Tag name. `<` must open the line or follow a space or a delimiter, so that
        // `Array<string>` in the script isn't read as markup. A capitalized name is a
        // component, exactly as the compiler decides.
        if c == '<' && prev.is_none_or(|p| p.is_whitespace() || matches!(p, '>' | '}' | '(' | '=')) {
            let name_start = if i + 1 < len && chars[i + 1] == '/' { i + 2 } else { i + 1 };
            if name_start < len && chars[name_start].is_ascii_alphabetic() {
                let mut j = name_start;
                while j < len && (chars[j].is_ascii_alphanumeric() || matches!(chars[j], '-' | '.' | '_')) {
                    j += 1;
                }
                push(&mut tokens, collect(&chars, i, name_start), TokenKind::Normal);
                let kind = if chars[name_start].is_ascii_uppercase() {
                    TokenKind::KeywordType
                } else {
                    TokenKind::Keyword
                };
                push(&mut tokens, collect(&chars, name_start, j), kind);
                i = j;
                continue;
            }
        }

        // String literal. An apostrophe between two letters is French text in the
        // markup (`n'est`, `l'accès`), not the start of a string.
        if c == '"' || c == '`' || (c == '\''
            && !(prev.is_some_and(char::is_alphabetic)
                && i + 1 < len
                && chars[i + 1].is_alphabetic()))
        {
            let mut j = i + 1;
            while j < len {
                if chars[j] == '\\' {
                    j += 2;
                    continue;
                }
                j += 1;
                if chars[j - 1] == c {
                    break;
                }
            }
            let j = j.min(len);
            push(&mut tokens, collect(&chars, i, j), TokenKind::String);
            i = j;
            continue;
        }

        // Number
        if c.is_ascii_digit() {
            let mut j = i;
            while j < len && (chars[j].is_ascii_alphanumeric() || chars[j] == '.' || chars[j] == '_') {
                j += 1;
            }
            push(&mut tokens, collect(&chars, i, j), TokenKind::Number);
            i = j;
            continue;
        }

        // Word: directive, keyword, type, function call or plain identifier
        if c.is_alphabetic() || c == '_' || c == '$' {
            let mut j = i;
            while j < len && is_ident_char(chars[j]) {
                j += 1;
            }
            let word = collect(&chars, i, j);

            // Directive: `on:click|preventDefault`, `bind:value`, `class:actif`, …
            // The name must follow the colon directly, which keeps `transition: x`
            // in CSS and `{ in: 1 }` in TypeScript out of it.
            if DIRECTIVES.contains(&word.as_str())
                && j + 1 < len
                && chars[j] == ':'
                && (is_ident_char(chars[j + 1]) || chars[j + 1] == '-')
            {
                let mut k = j + 1;
                while k < len && (is_ident_char(chars[k]) || chars[k] == '-' || chars[k] == '|') {
                    k += 1;
                }
                push(&mut tokens, collect(&chars, i, k), TokenKind::Macro);
                i = k;
                continue;
            }

            let kind = if KEYWORDS.contains(&word.as_str()) {
                TokenKind::Keyword
            } else if TYPE_KEYWORDS.contains(&word.as_str()) {
                TokenKind::KeywordType
            } else if j < len && chars[j] == '(' {
                TokenKind::Function
            } else {
                TokenKind::Normal
            };
            push(&mut tokens, word, kind);
            i = j;
            continue;
        }

        push(&mut tokens, c.to_string(), TokenKind::Normal);
        i += 1;
    }

    tokens
}

// ── FFI interface ─────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn language_id() -> *const std::ffi::c_char {
    c"spd".as_ptr()
}

#[no_mangle]
pub extern "C" fn file_extensions() -> *const std::ffi::c_char {
    c"spd".as_ptr()
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
/// `ptr` must have been returned by `tokenize_line_ffi` or `hover_info_ffi`.
#[no_mangle]
pub unsafe extern "C" fn free_string(ptr: *mut std::ffi::c_char) {
    if !ptr.is_null() {
        unsafe { drop(std::ffi::CString::from_raw(ptr)) }
    }
}

/// Fallback hover, used while the language server isn't running: the declaration
/// of `word` in the component, from the `<script>` or a `{#snippet}`.
pub fn hover_info(word: &str, file_content: &str) -> Option<String> {
    let fn_patterns = [
        format!("function {}(", word),
        format!("function {} (", word),
        format!("function {}<", word),
        format!("const {} = (", word),
        format!("const {} = async", word),
        format!("{{#snippet {}(", word),
    ];
    let type_patterns = [
        format!("interface {} ", word),
        format!("interface {}{{", word),
        format!("interface {}<", word),
        format!("type {} =", word),
        format!("type {}<", word),
        format!("class {} ", word),
        format!("class {}{{", word),
        format!("enum {} ", word),
    ];
    let var_patterns = [
        format!("const {}", word),
        format!("let {}", word),
        format!("var {}", word),
    ];
    let is_word_end = |rest: &str| !rest.starts_with(is_ident_char);

    for line in file_content.lines() {
        let trimmed = line.trim();
        let decl = trimmed.strip_prefix("export ").unwrap_or(trimmed);
        let decl = decl.strip_prefix("async ").unwrap_or(decl);
        for pat in fn_patterns.iter().chain(type_patterns.iter()) {
            if decl.starts_with(pat.as_str()) {
                let sig = trimmed.trim_end_matches(['{', '}']).trim_end();
                return Some(format!("```ts\n{sig}\n```"));
            }
        }
        for pat in &var_patterns {
            if let Some(rest) = decl.strip_prefix(pat.as_str()) {
                if is_word_end(rest) {
                    let end = trimmed.find('=').unwrap_or(trimmed.len());
                    let sig = trimmed[..end].trim_end();
                    return Some(format!("```ts\n{sig}\n```"));
                }
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
                json_escape(&t.text),
                t.kind.color_category()
            )
        })
        .collect();
    format!("[{}]", parts.join(","))
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
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

pub fn lsp_server_command() -> Option<(String, Vec<String>)> {
    Some(("speedster-language-server".to_string(), vec!["--stdio".to_string()]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has(tokens: &[Token], text: &str, kind: TokenKind) -> bool {
        tokens.iter().any(|t| t.text == text && t.kind == kind)
    }

    #[test]
    fn tokens_rebuild_the_line() {
        for line in [
            "<button type=\"button\" on:click|preventDefault={compter}>",
            "  {t.demo.clics({ n: clics })}",
            "{#each items as item, i (item.id)}",
            "    font-weight: 600; /* gras */",
            "<p>Ce n'est pas l'accès — voir https://exemple.fr</p>",
            "const é = `x${y}` // fin",
        ] {
            let rebuilt: String = tokenize_line(line).iter().map(|t| t.text.as_str()).collect();
            assert_eq!(rebuilt, line);
        }
    }

    #[test]
    fn script_keywords_and_types() {
        let tokens = tokenize_line("let clics: number = 0");
        assert!(has(&tokens, "let", TokenKind::Keyword));
        assert!(has(&tokens, "number", TokenKind::KeywordType));
        assert!(has(&tokens, "0", TokenKind::Number));
    }

    #[test]
    fn function_call() {
        let tokens = tokenize_line("const compter = () => increment(1)");
        assert!(has(&tokens, "increment", TokenKind::Function));
    }

    #[test]
    fn block_tags() {
        assert!(has(&tokenize_line("{#if ouvert}"), "{#if", TokenKind::Macro));
        assert!(has(&tokenize_line("{:else if autre}"), "{:else", TokenKind::Macro));
        assert!(has(&tokenize_line("{/each}"), "{/each", TokenKind::Macro));
        assert!(has(&tokenize_line("{@render enfant()}"), "{@render", TokenKind::Macro));
        assert!(has(&tokenize_line("{#snippet ligne(item)}"), "{#snippet", TokenKind::Macro));
    }

    #[test]
    fn directives_with_modifiers() {
        let tokens = tokenize_line("<form on:submit|preventDefault={envoyer} bind:this={el}>");
        assert!(has(&tokens, "on:submit|preventDefault", TokenKind::Macro));
        assert!(has(&tokens, "bind:this", TokenKind::Macro));
    }

    #[test]
    fn css_property_is_not_a_directive() {
        let tokens = tokenize_line("  transition: opacity 0.2s;");
        assert!(!tokens.iter().any(|t| t.kind == TokenKind::Macro));
    }

    #[test]
    fn elements_and_components() {
        let tokens = tokenize_line("<div><Compteur /></div>");
        assert!(has(&tokens, "div", TokenKind::Keyword));
        assert!(has(&tokens, "Compteur", TokenKind::KeywordType));
    }

    #[test]
    fn generics_are_not_markup() {
        let tokens = tokenize_line("const m = new Map<Item, string>()");
        assert!(!has(&tokens, "Item", TokenKind::KeywordType));
        assert!(has(&tokens, "string", TokenKind::KeywordType));
    }

    #[test]
    fn french_apostrophe_is_not_a_string() {
        let tokens = tokenize_line("<p>Ce n'est pas l'accès</p>");
        assert!(!tokens.iter().any(|t| t.kind == TokenKind::String));
        assert!(has(&tokens, "p", TokenKind::Keyword));
    }

    #[test]
    fn strings() {
        assert!(has(&tokenize_line("import x from './x.spd'"), "'./x.spd'", TokenKind::String));
        assert!(has(&tokenize_line("<a href=\"/aide\">"), "\"/aide\"", TokenKind::String));
    }

    #[test]
    fn comments() {
        assert!(has(&tokenize_line("<!-- note --> <b>"), "<!-- note -->", TokenKind::Comment));
        assert!(has(&tokenize_line("x = 1 // fin"), "// fin", TokenKind::Comment));
        assert!(has(&tokenize_line("a { /* css */ }"), "/* css */", TokenKind::Comment));
        assert!(!tokenize_line("voir https://exemple.fr").iter().any(|t| t.kind == TokenKind::Comment));
    }

    #[test]
    fn hover_finds_declarations() {
        let src = "<script lang=\"ts\">\n  let clics = 0\n  export function compter(n: number): void {\n</script>\n{#snippet ligne(item: Item)}";
        assert_eq!(hover_info("clics", src).as_deref(), Some("```ts\nlet clics\n```"));
        assert_eq!(
            hover_info("compter", src).as_deref(),
            Some("```ts\nexport function compter(n: number): void\n```")
        );
        assert_eq!(
            hover_info("ligne", src).as_deref(),
            Some("```ts\n{#snippet ligne(item: Item)\n```")
        );
        assert!(hover_info("clic", src).is_none());
    }

    #[test]
    fn json_output() {
        let json = tokens_to_json(&tokenize_line("a \"b\""));
        assert_eq!(json, r#"[{"text":"a ","kind":"normal"},{"text":"\"b\"","kind":"string"}]"#);
    }
}
