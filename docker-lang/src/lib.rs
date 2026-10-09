//! Dockerfile language module: highlighting for Dockerfiles and
//! Containerfiles, plus hover docs for the instructions.
//!
//! Instructions are reported as `keyword`, parser directives (`# syntax=`)
//! as `macro`, `--flags` as `type`, `$VAR` / `${VAR}` as `property`, quoted
//! strings and heredoc bodies as `string`. Line continuations (`\`, or the
//! character set by `# escape=`) keep the following lines as arguments.

use std::ffi::{c_char, CStr, CString};

/// Token kinds understood by the IDE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Keyword,
    Macro,
    Type,
    Property,
    String,
    Number,
    Comment,
    Normal,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Keyword => "keyword",
            Kind::Macro => "macro",
            Kind::Type => "type",
            Kind::Property => "property",
            Kind::String => "string",
            Kind::Number => "number",
            Kind::Comment => "comment",
            Kind::Normal => "normal",
        }
    }
}

/// Dockerfile instructions with their hover documentation.
const INSTRUCTIONS: &[(&str, &str)] = &[
    ("FROM", "Starts a build stage from a base image: `FROM [--platform=<platform>] <image>[:<tag>|@<digest>] [AS <name>]`."),
    ("RUN", "Runs a command in a new layer: shell form `RUN <command>` or exec form `RUN [\"executable\", \"arg\"]`. Flags: `--mount`, `--network`, `--security`."),
    ("CMD", "Default command of the container, replaced by the arguments of `docker run`. Only the last `CMD` counts."),
    ("LABEL", "Adds metadata to the image: `LABEL <key>=<value> ...`."),
    ("MAINTAINER", "Deprecated: author of the image. Use `LABEL org.opencontainers.image.authors=...` instead."),
    ("EXPOSE", "Documents the ports the container listens on: `EXPOSE <port>[/<protocol>] ...`. Does not publish them."),
    ("ENV", "Sets environment variables, kept in the image and the containers: `ENV <key>=<value> ...`."),
    ("ADD", "Copies files, directories, remote URLs or Git repositories into the image, unpacking local tar archives: `ADD [--chown=...] <src>... <dest>`."),
    ("COPY", "Copies files and directories from the build context or another stage (`--from=<stage>`) into the image: `COPY [--chown=...] <src>... <dest>`."),
    ("ENTRYPOINT", "Executable run by the container; `CMD` and the `docker run` arguments are appended to it in exec form."),
    ("VOLUME", "Declares mount points for external volumes: `VOLUME [\"/data\"]`."),
    ("USER", "User (and group) that runs the following instructions and the container: `USER <user>[:<group>]`."),
    ("WORKDIR", "Working directory of the following instructions and the container, created if missing: `WORKDIR /path`."),
    ("ARG", "Build-time variable, set with `docker build --build-arg <name>=<value>`: `ARG <name>[=<default>]`. Not kept in the image."),
    ("ONBUILD", "Instruction run when the image is used as the base of another build: `ONBUILD <INSTRUCTION>`."),
    ("STOPSIGNAL", "System call signal sent to stop the container: `STOPSIGNAL SIGTERM`."),
    ("HEALTHCHECK", "Command checking the container is healthy: `HEALTHCHECK [--interval=30s --timeout=30s --retries=3] CMD <command>`, or `HEALTHCHECK NONE`."),
    ("SHELL", "Shell of the shell form of the following instructions: `SHELL [\"powershell\", \"-Command\"]`."),
];

/// Parser directives, valid as `# <name>=<value>` before the first instruction.
const DIRECTIVES: &[&str] = &["syntax", "escape", "check"];

fn instruction(word: &str) -> Option<&'static str> {
    INSTRUCTIONS
        .iter()
        .map(|(name, _)| *name)
        .find(|name| name.eq_ignore_ascii_case(word))
}

/// Tokenizer state carried from one line to the next.
struct State {
    escape: char,
    /// The previous line ended with the escape character.
    continuation: bool,
    /// Heredoc terminators still open, in order, with whether leading tabs
    /// are stripped (`<<-`).
    heredocs: Vec<(String, bool)>,
    /// Still in the parser directive header.
    header: bool,
    /// Instruction of the current logical line.
    current: Option<&'static str>,
}

/// Tokens of each line of `source` (one vector per line, line breaks
/// excluded).
pub fn tokenize(source: &str) -> Vec<Vec<(String, Kind)>> {
    let mut state = State {
        escape: '\\',
        continuation: false,
        heredocs: Vec::new(),
        header: true,
        current: None,
    };
    source
        .split('\n')
        .map(|line| tokenize_line(line.strip_suffix('\r').unwrap_or(line), &mut state))
        .collect()
}

fn tokenize_line(line: &str, state: &mut State) -> Vec<(String, Kind)> {
    // Heredoc body, up to its terminator line.
    if let Some((end, strip_tabs)) = state.heredocs.first() {
        let candidate = if *strip_tabs {
            line.trim_start_matches('\t')
        } else {
            line
        };
        let kind = if candidate == end {
            state.heredocs.remove(0);
            Kind::Keyword
        } else {
            Kind::String
        };
        return vec![(line.to_string(), kind)];
    }

    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return if line.is_empty() {
            Vec::new()
        } else {
            vec![(line.to_string(), Kind::Normal)]
        };
    }
    if let Some(comment) = trimmed.strip_prefix('#') {
        let kind = match directive(comment) {
            Some((name, value)) if state.header => {
                if name == "escape" {
                    state.escape = value.chars().next().unwrap_or('\\');
                }
                Kind::Macro
            }
            _ => {
                state.header = false;
                Kind::Comment
            }
        };
        return vec![(line.to_string(), kind)];
    }
    state.header = false;

    let mut out = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    let mut first_word = !state.continuation;
    while i < chars.len() {
        let c = chars[i];
        let start = i;
        let at_word_start = i == 0 || chars[i - 1].is_whitespace();
        let kind = if c.is_whitespace() {
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            Kind::Normal
        } else if c == '"' || c == '\'' {
            i = scan_string(&chars, i, state.escape);
            Kind::String
        } else if c == '$' {
            i = scan_variable(&chars, i);
            if i == start + 1 {
                Kind::Normal
            } else {
                Kind::Property
            }
        } else if c == '<' && chars.get(i + 1) == Some(&'<') && at_word_start {
            match scan_heredoc(&chars, i) {
                Some((end, terminator, strip_tabs)) => {
                    i = end;
                    state.heredocs.push((terminator, strip_tabs));
                    Kind::Keyword
                }
                None => {
                    i += 2;
                    Kind::Normal
                }
            }
        } else if c == state.escape && chars[i + 1..].iter().all(|c| c.is_whitespace()) {
            i += 1;
            Kind::Normal
        } else {
            while i < chars.len() && !is_word_break(chars[i]) {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if first_word {
                first_word = false;
                match instruction(&word) {
                    Some(name) => {
                        // `ONBUILD <INSTRUCTION>`: the next word is one too.
                        first_word = name == "ONBUILD";
                        state.current = Some(name);
                        Kind::Keyword
                    }
                    None => Kind::Normal,
                }
            } else if at_word_start && word.starts_with("--") {
                // `--flag=value`: only the name is a flag.
                if let Some(eq) = word.find('=') {
                    i = start + word[..=eq].chars().count();
                }
                Kind::Type
            } else if state.current == Some("FROM") && word.eq_ignore_ascii_case("AS") {
                Kind::Keyword
            } else if is_number(&word) {
                Kind::Number
            } else {
                Kind::Normal
            }
        };
        push(&mut out, chars[start..i].iter().collect(), kind);
    }
    state.continuation = line.trim_end().ends_with(state.escape);
    out
}

/// `# name=value` parser directive: lowercase name and value.
fn directive(comment: &str) -> Option<(String, &str)> {
    let (name, value) = comment.split_once('=')?;
    let name = name.trim().to_ascii_lowercase();
    DIRECTIVES
        .contains(&name.as_str())
        .then(|| (name, value.trim()))
}

fn is_word_break(c: char) -> bool {
    c.is_whitespace() || c == '"' || c == '\'' || c == '$'
}

fn is_number(word: &str) -> bool {
    // Ports (`8080`, `53/udp`, `8000-8010`) and plain numbers.
    let digits = word.split(['/', '-']).next().unwrap_or("");
    !digits.is_empty()
        && digits.chars().all(|c| c.is_ascii_digit())
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '/' || c == '-')
}

/// End (exclusive) of the string starting at `start`; strings stop at the
/// end of the line when unterminated.
fn scan_string(chars: &[char], start: usize, escape: char) -> usize {
    let quote = chars[start];
    let mut i = start + 1;
    while i < chars.len() {
        if chars[i] == escape && quote == '"' {
            i += 2;
            continue;
        }
        if chars[i] == quote {
            return i + 1;
        }
        i += 1;
    }
    chars.len()
}

/// End of `$NAME` / `${NAME...}` starting at `start` (`start + 1` when the
/// `$` starts no variable).
fn scan_variable(chars: &[char], start: usize) -> usize {
    let mut i = start + 1;
    if chars.get(i) == Some(&'{') {
        while i < chars.len() && chars[i] != '}' {
            i += 1;
        }
        return (i + 1).min(chars.len());
    }
    while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
        i += 1;
    }
    i
}

/// `<<EOF`, `<<-EOF`, `<<"EOF"`: end of the marker, terminator, strip tabs.
fn scan_heredoc(chars: &[char], start: usize) -> Option<(usize, String, bool)> {
    let mut i = start + 2;
    let strip_tabs = chars.get(i) == Some(&'-');
    if strip_tabs {
        i += 1;
    }
    let quote = chars.get(i).copied().filter(|c| *c == '"' || *c == '\'');
    if quote.is_some() {
        i += 1;
    }
    let name_start = i;
    while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
        i += 1;
    }
    if i == name_start {
        return None;
    }
    let name: String = chars[name_start..i].iter().collect();
    if let Some(q) = quote {
        if chars.get(i) != Some(&q) {
            return None;
        }
        i += 1;
    }
    Some((i, name, strip_tabs))
}

fn push(out: &mut Vec<(String, Kind)>, text: String, kind: Kind) {
    if text.is_empty() {
        return;
    }
    match out.last_mut() {
        Some((prev, k)) if *k == kind && kind == Kind::Normal => prev.push_str(&text),
        _ => out.push((text, kind)),
    }
}

// ── Hover ─────────────────────────────────────────────────────────────────────

/// Hover documentation for `word`: Dockerfile instructions.
pub fn hover(word: &str, _content: &str) -> Option<String> {
    INSTRUCTIONS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(word))
        .map(|(name, doc)| format!("**`{name}`** — {doc}"))
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
    c"dockerfile".as_ptr()
}

#[no_mangle]
pub extern "C" fn file_extensions() -> *const c_char {
    c"dockerfile,containerfile".as_ptr()
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
    fn from_with_flags_stage_name_and_variables() {
        let lines = tokenize("FROM --platform=$BUILDPLATFORM rust:${RUST_VERSION} AS build");
        assert_eq!(
            kinds(&lines[0]),
            [
                ("FROM", Kind::Keyword),
                ("--platform=", Kind::Type),
                ("$BUILDPLATFORM", Kind::Property),
                ("rust:", Kind::Normal),
                ("${RUST_VERSION}", Kind::Property),
                ("AS", Kind::Keyword),
                ("build", Kind::Normal),
            ]
        );
    }

    #[test]
    fn instructions_are_case_insensitive_and_only_first_word() {
        let lines = tokenize("run echo from copy\ncopy --from=build /app /app");
        assert_eq!(
            kinds(&lines[0]),
            [("run", Kind::Keyword), ("echo from copy", Kind::Normal)]
        );
        assert_eq!(lines[1][0], ("copy".to_string(), Kind::Keyword));
        assert_eq!(lines[1][2], ("--from=".to_string(), Kind::Type));
    }

    #[test]
    fn continuation_lines_are_arguments_and_comments_inside_them() {
        let src = "RUN apt-get update \\\n    # comment\n    && apt-get install -y curl\nUSER app";
        let lines = tokenize(src);
        assert_eq!(kinds(&lines[1]), [("# comment", Kind::Comment)]);
        assert_eq!(
            kinds(&lines[2]),
            [("&& apt-get install -y curl", Kind::Normal)]
        );
        assert_eq!(lines[3][0], ("USER".to_string(), Kind::Keyword));
    }

    #[test]
    fn parser_directives_only_in_the_header() {
        let src = "# syntax=docker/dockerfile:1\n# escape=`\nFROM scratch\n# syntax=late";
        let lines = tokenize(src);
        assert_eq!(lines[0][0].1, Kind::Macro);
        assert_eq!(lines[1][0].1, Kind::Macro);
        assert_eq!(lines[3][0].1, Kind::Comment);
    }

    #[test]
    fn escape_directive_changes_the_continuation_character() {
        let src = "# escape=`\nRUN dir c:\\ `\n    FROM\nRUN x";
        let lines = tokenize(src);
        assert_eq!(kinds(&lines[2]), [("FROM", Kind::Normal)]);
        assert_eq!(lines[3][0], ("RUN".to_string(), Kind::Keyword));
    }

    #[test]
    fn heredocs_are_strings_until_their_terminator() {
        let src = "RUN <<EOF bash\nset -e\n  FROM x\nEOF\nCOPY <<-\"A\" <<B /x\n\tone\n\tA\ntwo\nB\nENV k=v";
        let lines = tokenize(src);
        assert_eq!(lines[0][2], ("<<EOF".to_string(), Kind::Keyword));
        assert_eq!(lines[1], [("set -e".to_string(), Kind::String)]);
        assert_eq!(lines[2], [("  FROM x".to_string(), Kind::String)]);
        assert_eq!(lines[3], [("EOF".to_string(), Kind::Keyword)]);
        assert_eq!(lines[6], [("\tA".to_string(), Kind::Keyword)]);
        assert_eq!(lines[7], [("two".to_string(), Kind::String)]);
        assert_eq!(lines[8], [("B".to_string(), Kind::Keyword)]);
        assert_eq!(lines[9][0], ("ENV".to_string(), Kind::Keyword));
    }

    #[test]
    fn strings_numbers_and_onbuild() {
        let lines = tokenize("CMD [\"run\", \"a b\"]\nEXPOSE 80 53/udp\nONBUILD RUN make");
        assert_eq!(
            kinds(&lines[0]),
            [
                ("CMD", Kind::Keyword),
                ("[", Kind::Normal),
                ("\"run\"", Kind::String),
                (",", Kind::Normal),
                ("\"a b\"", Kind::String),
                ("]", Kind::Normal),
            ]
        );
        assert_eq!(
            kinds(&lines[1]),
            [
                ("EXPOSE", Kind::Keyword),
                ("80", Kind::Number),
                ("53/udp", Kind::Number)
            ]
        );
        assert_eq!(
            kinds(&lines[2]),
            [
                ("ONBUILD", Kind::Keyword),
                ("RUN", Kind::Keyword),
                ("make", Kind::Normal)
            ]
        );
    }

    #[test]
    fn tokens_cover_the_whole_line() {
        for line in [
            "FROM a AS b  # x",
            "RUN echo \"$HOME\" 'x' \\",
            "  ",
            "LABEL a=\"unterminated",
        ] {
            let text: String = tokenize(line)[0].iter().map(|(t, _)| t.as_str()).collect();
            assert_eq!(text, line);
        }
    }

    #[test]
    fn hover_documents_instructions() {
        assert!(hover("workdir", "").unwrap().starts_with("**`WORKDIR`**"));
        assert!(hover("nope", "").is_none());
    }

    #[test]
    fn ffi_round_trip() {
        let src = CString::new("FROM x\nRUN y").unwrap();
        let out = unsafe { tokenize_document_ffi(src.as_ptr()) };
        let json = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_string();
        unsafe { free_string(out) };
        assert!(
            json.starts_with(r#"[[{"text":"FROM","kind":"keyword"}"#),
            "{json}"
        );
    }
}
