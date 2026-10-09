//! Docker Compose language module: YAML highlighting for Compose files
//! (`compose.yaml`, `docker-compose.yml`…) plus hover docs for Compose keys.
//!
//! Mapping keys are reported as `property`, values as `string` (`number`,
//! `keyword` for booleans and null), `${VAR}` interpolation as `property`,
//! anchors, aliases and merge keys as `macro`, tags as `type`, block scalar
//! bodies (`|`, `>`) as `string` and comments as `comment`.

/// Token kinds understood by the IDE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Property,
    String,
    Number,
    Keyword,
    Macro,
    Type,
    Comment,
    Normal,
}

impl Kind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Kind::Property => "property",
            Kind::String => "string",
            Kind::Number => "number",
            Kind::Keyword => "keyword",
            Kind::Macro => "macro",
            Kind::Type => "type",
            Kind::Comment => "comment",
            Kind::Normal => "normal",
        }
    }
}

/// Tokens of each line of `source` (one vector per line, line breaks
/// excluded).
pub fn tokenize(source: &str) -> Vec<Vec<(String, Kind)>> {
    // Indentation of the line that opened a block scalar (`key: |`): more
    // indented (and blank) lines below are its content.
    let mut block: Option<usize> = None;
    source
        .split('\n')
        .map(|line| {
            let line = line.strip_suffix('\r').unwrap_or(line);
            let indent = line.len() - line.trim_start().len();
            if let Some(parent) = block {
                if line.trim().is_empty() || indent > parent {
                    return if line.is_empty() {
                        Vec::new()
                    } else {
                        vec![(line.to_string(), Kind::String)]
                    };
                }
                block = None;
            }
            let (tokens, opens_block) = tokenize_line(line);
            if opens_block {
                block = Some(indent);
            }
            tokens
        })
        .collect()
}

/// Tokens of one line, and whether it opens a block scalar.
fn tokenize_line(line: &str) -> (Vec<(String, Kind)>, bool) {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut opens_block = false;
    let mut flow_depth = 0usize;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let start = i;
        let after_space = i == 0 || chars[i - 1].is_whitespace();
        if c.is_whitespace() {
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            push(&mut out, collect(&chars[start..i]), Kind::Normal);
        } else if c == '#' && after_space {
            push(&mut out, collect(&chars[start..]), Kind::Comment);
            break;
        } else if start == 0 && (line.starts_with("---") || line.starts_with("...")) {
            i = 3;
            push(&mut out, collect(&chars[..3]), Kind::Keyword);
        } else if c == '-' && after_space && chars.get(i + 1).is_none_or(|c| c.is_whitespace()) {
            i += 1;
            push(&mut out, "-".into(), Kind::Normal);
        } else if matches!(c, '[' | '{') {
            flow_depth += 1;
            i += 1;
            push(&mut out, c.to_string(), Kind::Normal);
        } else if matches!(c, ']' | '}' | ',') && flow_depth > 0 {
            if c != ',' {
                flow_depth -= 1;
            }
            i += 1;
            push(&mut out, c.to_string(), Kind::Normal);
        } else if c == ':' {
            i += 1;
            push(&mut out, ":".into(), Kind::Normal);
        } else if matches!(c, '&' | '*' | '!') && after_space {
            while i < chars.len()
                && !chars[i].is_whitespace()
                && !matches!(chars[i], ',' | ']' | '}')
            {
                i += 1;
            }
            let kind = if c == '!' { Kind::Type } else { Kind::Macro };
            push(&mut out, collect(&chars[start..i]), kind);
        } else if matches!(c, '|' | '>') && after_space && is_block_header(&chars[i..]) {
            while i < chars.len() && !chars[i].is_whitespace() {
                i += 1;
            }
            opens_block = true;
            push(&mut out, collect(&chars[start..i]), Kind::Keyword);
        } else if c == '"' || c == '\'' {
            i = scan_quoted(&chars, i);
            let kind = if is_key_end(&chars, i) {
                Kind::Property
            } else {
                Kind::String
            };
            push_scalar(&mut out, &chars[start..i], kind);
        } else {
            i = scan_plain(&chars, i, flow_depth > 0);
            let text = collect(&chars[start..i]);
            let kind = if is_key_end(&chars, i) {
                if text == "<<" {
                    Kind::Macro
                } else {
                    Kind::Property
                }
            } else {
                plain_kind(&text)
            };
            push_scalar(&mut out, &chars[start..i], kind);
        }
    }
    (out, opens_block)
}

fn collect(chars: &[char]) -> String {
    chars.iter().collect()
}

/// `|`, `>` and their indicators (`|-`, `>+`, `|2`), then a comment or nothing.
fn is_block_header(chars: &[char]) -> bool {
    let end = chars
        .iter()
        .position(|c| c.is_whitespace())
        .unwrap_or(chars.len());
    chars[1..end]
        .iter()
        .all(|c| matches!(c, '-' | '+' | '1'..='9'))
        && collect(&chars[end..])
            .trim_start()
            .chars()
            .next()
            .is_none_or(|c| c == '#')
}

/// A `:` followed by a space or the end of the line comes at `i`: the
/// scalar before it is a mapping key.
fn is_key_end(chars: &[char], i: usize) -> bool {
    let mut j = i;
    while j < chars.len() && chars[j] == ' ' {
        j += 1;
    }
    chars.get(j) == Some(&':') && chars.get(j + 1).is_none_or(|c| c.is_whitespace())
}

/// End of a quoted scalar (unterminated: end of line).
fn scan_quoted(chars: &[char], start: usize) -> usize {
    let quote = chars[start];
    let mut i = start + 1;
    while i < chars.len() {
        match chars[i] {
            '\\' if quote == '"' => i += 2,
            // `''` is an escaped quote in single-quoted scalars.
            '\'' if quote == '\'' && chars.get(i + 1) == Some(&'\'') => i += 2,
            c if c == quote => return i + 1,
            _ => i += 1,
        }
    }
    chars.len()
}

/// End of a plain scalar: before `: `, ` #`, the end of the line, or a flow
/// indicator inside `[]` / `{}`.
fn scan_plain(chars: &[char], start: usize, in_flow: bool) -> usize {
    let mut i = start;
    while i < chars.len() {
        let c = chars[i];
        if c == ':'
            && chars
                .get(i + 1)
                .is_none_or(|c| c.is_whitespace() || (in_flow && matches!(c, ',' | ']' | '}')))
        {
            break;
        }
        if c == '#' && i > start && chars[i - 1].is_whitespace() {
            break;
        }
        if in_flow && matches!(c, ',' | '[' | ']' | '{' | '}') {
            break;
        }
        i += 1;
    }
    // Trailing spaces belong to the separator.
    while i > start + 1 && chars[i - 1].is_whitespace() {
        i -= 1;
    }
    i.max(start + 1)
}

fn plain_kind(text: &str) -> Kind {
    const LITERALS: &[&str] = &["true", "false", "yes", "no", "on", "off", "null", "~"];
    if LITERALS.iter().any(|l| l.eq_ignore_ascii_case(text)) {
        Kind::Keyword
    } else if is_number(text) {
        Kind::Number
    } else {
        Kind::String
    }
}

fn is_number(text: &str) -> bool {
    let t = text.strip_prefix(['-', '+']).unwrap_or(text);
    (!t.is_empty()
        && t.chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == '_')
        && t.chars().any(|c| c.is_ascii_digit())
        && t.matches('.').count() <= 1)
        || t.strip_prefix("0x")
            .is_some_and(|h| !h.is_empty() && h.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Push a string scalar, with its `${VAR}` / `$VAR` interpolations as
/// `property` (`$$` is a literal `$`).
fn push_scalar(out: &mut Vec<(String, Kind)>, chars: &[char], kind: Kind) {
    if kind != Kind::String {
        push(out, collect(chars), kind);
        return;
    }
    let mut start = 0;
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '$' {
            i += 1;
            continue;
        }
        if chars.get(i + 1) == Some(&'$') {
            i += 2;
            continue;
        }
        let end = if chars.get(i + 1) == Some(&'{') {
            chars[i..].iter().position(|c| *c == '}').map(|p| i + p + 1)
        } else {
            let n = chars[i + 1..]
                .iter()
                .take_while(|c| c.is_ascii_alphanumeric() || **c == '_')
                .count();
            (n > 0).then_some(i + 1 + n)
        };
        let Some(end) = end else {
            i += 1;
            continue;
        };
        push(out, collect(&chars[start..i]), Kind::String);
        push(out, collect(&chars[i..end]), Kind::Property);
        start = end;
        i = end;
    }
    push(out, collect(&chars[start..]), Kind::String);
}

fn push(out: &mut Vec<(String, Kind)>, text: String, kind: Kind) {
    if !text.is_empty() {
        out.push((text, kind));
    }
}

// ── Hover ─────────────────────────────────────────────────────────────────────

/// Compose keys with their hover documentation.
const KEYS: &[(&str, &str)] = &[
    ("services", "The containers of the application, one entry per service."),
    ("networks", "Networks the services join (top level: networks to create; in a service: networks it is attached to)."),
    ("volumes", "Top level: named volumes to create. In a service: mounts, `<source>:<target>[:<mode>]` or the long syntax (`type`, `source`, `target`, `read_only`)."),
    ("configs", "Configuration files granted to services, mounted read-only in the container."),
    ("secrets", "Sensitive data granted to services, mounted under `/run/secrets/<name>`."),
    ("include", "Other Compose files to include in this application."),
    ("name", "Project name (top level), used to prefix containers, networks and volumes."),
    ("version", "Obsolete: ignored by Compose v2, which always uses the latest Compose Specification."),
    ("image", "Image to start the container from: `name[:tag|@digest]`. Built from `build` when both are set."),
    ("build", "How to build the service image: a context path, or `context`, `dockerfile`, `args`, `target`, `ssh`, `secrets`…"),
    ("context", "Build context: a directory or Git repository URL sent to the builder."),
    ("dockerfile", "Dockerfile to build, relative to the build context."),
    ("dockerfile_inline", "Dockerfile content given inline instead of a file."),
    ("args", "Build arguments (`ARG` values in the Dockerfile)."),
    ("target", "Build stage to build in a multi-stage Dockerfile; in a mount, path inside the container."),
    ("command", "Overrides the image's `CMD`: a string or a list of arguments."),
    ("entrypoint", "Overrides the image's `ENTRYPOINT`."),
    ("container_name", "Fixed container name instead of the generated one; prevents scaling the service."),
    ("ports", "Published ports: `[HOST:]CONTAINER[/PROTOCOL]`, or the long syntax (`target`, `published`, `protocol`, `host_ip`)."),
    ("expose", "Ports exposed to the other services only, not published on the host."),
    ("environment", "Environment variables of the container, as a map or a list of `KEY=VALUE`."),
    ("env_file", "Files of `KEY=VALUE` lines added to the container's environment."),
    ("depends_on", "Services to start first. Long syntax: `condition` (`service_started`, `service_healthy`, `service_completed_successfully`), `restart`, `required`."),
    ("condition", "When the dependency is ready: `service_started`, `service_healthy` or `service_completed_successfully`."),
    ("restart", "Restart policy: `no`, `always`, `on-failure[:max-retries]` or `unless-stopped`."),
    ("healthcheck", "Command checking the container is healthy: `test`, `interval`, `timeout`, `retries`, `start_period`, `start_interval`, `disable`."),
    ("test", "Health check command: `[\"CMD\", ...]`, `[\"CMD-SHELL\", \"...\"]` or `[\"NONE\"]`."),
    ("profiles", "Profiles the service belongs to; it only starts when one of them is enabled (`--profile`)."),
    ("deploy", "Deployment settings: `replicas`, `resources` (limits / reservations), `restart_policy`, `placement`…"),
    ("resources", "Resource constraints: `limits` and `reservations` (`cpus`, `memory`, `devices`)."),
    ("labels", "Metadata labels added to the container (or network / volume)."),
    ("working_dir", "Working directory of the container, overriding the image's `WORKDIR`."),
    ("user", "User running the container process, overriding the image's `USER`."),
    ("extends", "Reuses the configuration of another service, from this file or `file`."),
    ("network_mode", "Network mode: `bridge`, `host`, `none`, `service:<name>` or `container:<name>`."),
    ("hostname", "Hostname of the container."),
    ("extra_hosts", "Extra `/etc/hosts` entries: `hostname:IP` (`host-gateway` for the host)."),
    ("stdin_open", "Keeps stdin open (`docker run -i`)."),
    ("tty", "Allocates a pseudo-TTY (`docker run -t`)."),
    ("privileged", "Runs the container with extended privileges."),
    ("cap_add", "Linux capabilities added to the container."),
    ("cap_drop", "Linux capabilities removed from the container."),
    ("platform", "Target platform of the service, e.g. `linux/amd64`."),
    ("pull_policy", "When to pull the image: `always`, `never`, `missing`, `build` or `daily`."),
    ("logging", "Logging driver and options of the container."),
    ("external", "The resource is created outside Compose; it is not created or removed with the project."),
    ("driver", "Driver of the network or volume (`bridge`, `overlay`, `local`…)."),
    ("develop", "Development settings, such as `watch` rules for `docker compose watch`."),
    ("watch", "Files to watch with `docker compose watch`: `path`, `action` (`sync`, `rebuild`, `sync+restart`), `target`, `ignore`."),
];

/// Hover documentation for `word` when it is a key of the document.
pub fn hover(word: &str, content: &str) -> Option<String> {
    let (key, doc) = KEYS.iter().find(|(k, _)| *k == word)?;
    let is_key = content.lines().any(|l| {
        let l = l.trim_start().trim_start_matches("- ");
        l.strip_prefix(word)
            .is_some_and(|rest| rest.trim_start().starts_with(':'))
    });
    is_key.then(|| format!("**`{key}`** — {doc}"))
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
        let lines = tokenize(
            "services:\n  web:\n    image: nginx:1.27 # pinned\n    tty: true\n    scale: 2",
        );
        assert_eq!(
            kinds(&lines[0]),
            [("services", Kind::Property), (":", Kind::Normal)]
        );
        assert_eq!(
            kinds(&lines[2]),
            [
                ("image", Kind::Property),
                (":", Kind::Normal),
                ("nginx:1.27", Kind::String),
                ("# pinned", Kind::Comment),
            ]
        );
        assert_eq!(kinds(&lines[3])[2], ("true", Kind::Keyword));
        assert_eq!(kinds(&lines[4])[2], ("2", Kind::Number));
    }

    #[test]
    fn list_items_and_quoted_values() {
        let lines = tokenize("    ports:\n      - \"8080:80\"\n      - 443:443\n      - 'it''s'");
        assert_eq!(
            kinds(&lines[1]),
            [("-", Kind::Normal), ("\"8080:80\"", Kind::String)]
        );
        assert_eq!(
            kinds(&lines[2]),
            [("-", Kind::Normal), ("443:443", Kind::String)]
        );
        assert_eq!(
            kinds(&lines[3]),
            [("-", Kind::Normal), ("'it''s'", Kind::String)]
        );
    }

    #[test]
    fn interpolation_inside_values() {
        let lines = tokenize("    image: \"app:${TAG:-latest}\"\n    command: echo $$HOME $USER");
        assert_eq!(
            kinds(&lines[0]),
            [
                ("image", Kind::Property),
                (":", Kind::Normal),
                ("\"app:", Kind::String),
                ("${TAG:-latest}", Kind::Property),
                ("\"", Kind::String),
            ]
        );
        assert_eq!(
            kinds(&lines[1])[2..],
            [("echo $$HOME", Kind::String), ("$USER", Kind::Property)]
        );
    }

    #[test]
    fn anchors_aliases_merge_keys_and_tags() {
        let lines =
            tokenize("x-common: &common\n  restart: always\nweb:\n  <<: *common\n  t: !reset []");
        assert_eq!(kinds(&lines[0])[2], ("&common", Kind::Macro));
        assert_eq!(
            kinds(&lines[3]),
            [
                ("<<", Kind::Macro),
                (":", Kind::Normal),
                ("*common", Kind::Macro)
            ]
        );
        assert_eq!(kinds(&lines[4])[2], ("!reset", Kind::Type));
    }

    #[test]
    fn block_scalars_until_dedent() {
        let src =
            "  app:\n    command: |-\n      echo a: b\n\n      # not a comment\n    tty: true";
        let lines = tokenize(src);
        assert_eq!(kinds(&lines[1])[2], ("|-", Kind::Keyword));
        assert_eq!(lines[2], [("      echo a: b".to_string(), Kind::String)]);
        assert!(lines[3].is_empty());
        assert_eq!(lines[4][0].1, Kind::String);
        assert_eq!(kinds(&lines[5])[0], ("tty", Kind::Property));
    }

    #[test]
    fn flow_collections() {
        let lines = tokenize("test: [\"CMD\", curl, -f]\nlabels: {a: 1, b: x}");
        assert_eq!(
            kinds(&lines[0]),
            [
                ("test", Kind::Property),
                (":", Kind::Normal),
                ("[", Kind::Normal),
                ("\"CMD\"", Kind::String),
                (",", Kind::Normal),
                ("curl", Kind::String),
                (",", Kind::Normal),
                ("-f", Kind::String),
                ("]", Kind::Normal),
            ]
        );
        assert_eq!(
            kinds(&lines[1])[3..7],
            [
                ("a", Kind::Property),
                (":", Kind::Normal),
                ("1", Kind::Number),
                (",", Kind::Normal)
            ]
        );
    }

    #[test]
    fn tokens_cover_the_whole_line() {
        for line in [
            "  a: b # c",
            "- x: 'y",
            "--- # doc",
            "k: v:w  ",
            "url: http://x#y",
            "  -",
        ] {
            let text: String = tokenize(line)[0].iter().map(|(t, _)| t.as_str()).collect();
            assert_eq!(text, line);
        }
    }

    #[test]
    fn hover_only_for_keys_of_the_document() {
        let doc = "services:\n  web:\n    depends_on:\n      - db\n";
        assert!(hover("depends_on", doc)
            .unwrap()
            .starts_with("**`depends_on`**"));
        assert!(hover("image", doc).is_none());
        assert!(hover("db", doc).is_none());
    }
}
