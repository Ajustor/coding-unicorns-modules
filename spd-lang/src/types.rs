//! Just enough TypeScript to know which strings an expression can hold:
//! `t.statuts[commande.statut]` shows the messages `commande.statut` can pick,
//! not the whole group.
//!
//! This is no type checker. It reads the declarations of the component's
//! script and of the `.ts` files it imports by a relative path — `type`
//! aliases, interfaces, typed `const` / `let` — follows `{#each … as x}` and
//! `{#snippet name(x: T)}` bindings, and resolves a path `a.b[0].c` to a union
//! of string literals. Anything else (`string`, generics, `keyof`, inference
//! from values) is unknown, and an unknown type restricts nothing.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// How deep imports are followed from the component.
const MAX_IMPORT_DEPTH: usize = 3;
/// How many aliases are followed before giving up (`type A = B`, cycles…).
const MAX_ALIASES: usize = 16;

#[derive(Debug, Clone, PartialEq)]
enum Ty {
    /// String literals: `'a' | 'b'`.
    Literals(Vec<String>),
    /// A named type, declared somewhere (or not: `string`, `Date`…).
    Named(String),
    Object(Vec<(String, Ty)>),
    Array(Box<Ty>),
    Union(Vec<Ty>),
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Ident(String),
    Str(String),
    Punct(char),
    /// `=>`, `?.`, `...`: never part of what is read here, kept apart so they
    /// are not mistaken for `=`, `?` or `.`.
    Other,
}

fn tokenize(source: &str) -> Vec<Token> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c.is_whitespace() {
            i += 1;
        } else if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i += 2;
        } else if c == '\'' || c == '"' || c == '`' {
            let mut text = String::new();
            i += 1;
            while i < chars.len() && chars[i] != c {
                if chars[i] == '\\' {
                    i += 1;
                }
                if let Some(&ch) = chars.get(i) {
                    text.push(ch);
                }
                i += 1;
            }
            i += 1;
            // A template with `${…}` holds no one string.
            tokens.push(if c == '`' && text.contains("${") {
                Token::Other
            } else {
                Token::Str(text)
            });
        } else if c.is_alphabetic() || c == '_' || c == '$' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '$')
            {
                i += 1;
            }
            tokens.push(Token::Ident(chars[start..i].iter().collect()));
        } else if c.is_ascii_digit() {
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '.') {
                i += 1;
            }
            tokens.push(Token::Other);
        } else if (c == '=' && next == Some('>'))
            || (c == '?' && next == Some('.'))
            || (c == '.' && next == Some('.'))
        {
            i += if c == '.' { 3 } else { 2 };
            tokens.push(Token::Other);
        } else {
            tokens.push(Token::Punct(c));
            i += 1;
        }
    }
    tokens
}

/// A recursive-descent reader of type expressions over tokens.
struct Parser<'a> {
    tokens: &'a [Token],
    at: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&'a Token> {
        self.tokens.get(self.at)
    }

    fn is_punct(&self, c: char) -> bool {
        self.peek() == Some(&Token::Punct(c))
    }

    fn eat_punct(&mut self, c: char) -> bool {
        let found = self.is_punct(c);
        if found {
            self.at += 1;
        }
        found
    }

    fn skip_balanced_if(&mut self, open: char) {
        if self.is_punct(open) {
            self.skip_balanced();
        }
    }

    /// Past the bracket closing the one at the current token (`(`, `[`, `{`, `<`).
    fn skip_balanced(&mut self) {
        let (open, close) = match self.peek() {
            Some(Token::Punct('(')) => ('(', ')'),
            Some(Token::Punct('[')) => ('[', ']'),
            Some(Token::Punct('{')) => ('{', '}'),
            Some(Token::Punct('<')) => ('<', '>'),
            _ => return,
        };
        let mut depth = 0;
        while let Some(token) = self.peek() {
            self.at += 1;
            if *token == Token::Punct(open) {
                depth += 1;
            } else if *token == Token::Punct(close) {
                depth -= 1;
                if depth == 0 {
                    return;
                }
            }
        }
    }

    /// `A | B`, `A & B` (unknown), with a leading `|` allowed.
    fn ty(&mut self) -> Ty {
        self.eat_punct('|');
        let mut members = vec![self.postfix()];
        let mut intersection = false;
        loop {
            if self.eat_punct('|') {
                members.push(self.postfix());
            } else if self.eat_punct('&') {
                intersection = true;
                self.postfix();
            } else {
                break;
            }
        }
        if intersection {
            return Ty::Unknown;
        }
        if members.len() == 1 {
            return members.pop().unwrap_or(Ty::Unknown);
        }
        Ty::Union(members)
    }

    /// A primary followed by `[]`s; an indexed access `T['k']` is unknown.
    fn postfix(&mut self) -> Ty {
        let mut ty = self.primary();
        while self.is_punct('[') {
            if self.tokens.get(self.at + 1) == Some(&Token::Punct(']')) {
                self.at += 2;
                ty = Ty::Array(Box::new(ty));
            } else {
                self.skip_balanced();
                ty = Ty::Unknown;
            }
        }
        ty
    }

    fn primary(&mut self) -> Ty {
        match self.peek().cloned() {
            Some(Token::Str(text)) => {
                self.at += 1;
                Ty::Literals(vec![text])
            }
            Some(Token::Ident(word)) => {
                self.at += 1;
                match word.as_str() {
                    "readonly" => self.postfix(),
                    "null" | "undefined" | "never" => Ty::Literals(Vec::new()),
                    "typeof" | "keyof" | "infer" | "unique" => {
                        self.postfix();
                        Ty::Unknown
                    }
                    _ => {
                        // A qualified name `ns.T` is out of reach.
                        let mut qualified = false;
                        while self.eat_punct('.') {
                            qualified = true;
                            self.at += 1;
                        }
                        if self.is_punct('<') {
                            if matches!(word.as_str(), "Array" | "ReadonlyArray") {
                                self.at += 1;
                                let element = self.ty();
                                self.eat_punct('>');
                                return Ty::Array(Box::new(element));
                            }
                            self.skip_balanced();
                            return Ty::Unknown;
                        }
                        if qualified {
                            Ty::Unknown
                        } else {
                            Ty::Named(word)
                        }
                    }
                }
            }
            Some(Token::Punct('{')) => self.object(),
            Some(Token::Punct('(')) => {
                self.at += 1;
                let inner = self.ty();
                if !self.eat_punct(')') {
                    // Parameters of a function type.
                    self.at -= 1;
                    self.skip_to_close_paren();
                }
                if self.peek() == Some(&Token::Other) {
                    // `=> R`
                    self.at += 1;
                    self.ty();
                    return Ty::Unknown;
                }
                inner
            }
            Some(Token::Punct('[')) => {
                self.skip_balanced();
                Ty::Unknown
            }
            _ => {
                self.at += 1;
                Ty::Unknown
            }
        }
    }

    /// From inside parentheses to past the `)` closing them.
    fn skip_to_close_paren(&mut self) {
        let mut depth = 1;
        while let Some(token) = self.peek() {
            self.at += 1;
            match token {
                Token::Punct('(') => depth += 1,
                Token::Punct(')') => {
                    depth -= 1;
                    if depth == 0 {
                        return;
                    }
                }
                _ => {}
            }
        }
    }

    /// `{ a: T; readonly b?: U, m(x): V; [k: string]: W }` from its `{`.
    fn object(&mut self) -> Ty {
        self.at += 1;
        let mut members = Vec::new();
        while let Some(token) = self.peek().cloned() {
            let start = self.at;
            match token {
                Token::Punct('}') => {
                    self.at += 1;
                    break;
                }
                Token::Punct(';') | Token::Punct(',') => self.at += 1,
                Token::Punct('[') => {
                    self.skip_balanced();
                    if self.eat_punct(':') {
                        self.ty();
                    }
                }
                Token::Ident(_) | Token::Str(_) => {
                    let name = match token {
                        Token::Ident(w) if w == "readonly"
                            && matches!(
                                self.tokens.get(self.at + 1),
                                Some(Token::Ident(_)) | Some(Token::Str(_))
                            ) =>
                        {
                            self.at += 1;
                            continue;
                        }
                        Token::Ident(w) | Token::Str(w) => w,
                        _ => unreachable!(),
                    };
                    self.at += 1;
                    self.eat_punct('?');
                    if self.is_punct('(') || self.is_punct('<') {
                        // A method: its signature tells nothing here.
                        self.skip_balanced();
                        if self.is_punct('(') {
                            self.skip_balanced();
                        }
                        if self.eat_punct(':') {
                            self.ty();
                        }
                    } else if self.eat_punct(':') {
                        let ty = self.ty();
                        members.push((name, ty));
                    }
                }
                _ => self.at += 1,
            }
            if self.at == start {
                self.at += 1;
            }
        }
        Ty::Object(members)
    }
}

/// The types and typed variables declared in a set of sources.
#[derive(Debug, Default)]
pub struct Scope {
    types: HashMap<String, Ty>,
    vars: HashMap<String, Ty>,
}

impl Scope {
    /// The declarations of the component at `path` (its scripts, the files
    /// they import by a relative path), and the bindings of its markup.
    pub fn of_component(path: &Path, text: &str) -> Scope {
        let mut scope = Scope::default();
        let mut seen = HashSet::new();
        for script in scripts(text) {
            scope.read(script, path, 0, &mut seen);
        }
        scope
    }

    fn read(&mut self, source: &str, path: &Path, depth: usize, seen: &mut HashSet<PathBuf>) {
        let tokens = tokenize(source);
        let mut parser = Parser {
            tokens: &tokens,
            at: 0,
        };
        while let Some(token) = parser.peek().cloned() {
            parser.at += 1;
            let Token::Ident(word) = token else {
                continue;
            };
            match word.as_str() {
                "type" => {
                    let Some(Token::Ident(name)) = parser.peek().cloned() else {
                        continue;
                    };
                    parser.at += 1;
                    parser.skip_balanced_if('<');
                    if parser.eat_punct('=') {
                        let ty = parser.ty();
                        self.types.insert(name, ty);
                    }
                }
                "interface" => {
                    let Some(Token::Ident(name)) = parser.peek().cloned() else {
                        continue;
                    };
                    parser.at += 1;
                    parser.skip_balanced_if('<');
                    // `extends A, B`: their members are not merged in.
                    while !parser.is_punct('{') && parser.peek().is_some() {
                        parser.at += 1;
                    }
                    let ty = parser.object();
                    self.types.insert(name, ty);
                }
                "const" | "let" | "var" => {
                    let Some(Token::Ident(name)) = parser.peek().cloned() else {
                        continue;
                    };
                    parser.at += 1;
                    if parser.eat_punct(':') {
                        let ty = parser.ty();
                        self.vars.insert(name, ty);
                    } else if parser.eat_punct('=') {
                        // `= value as T`, the value a single name.
                        if matches!(parser.peek(), Some(Token::Ident(_)))
                            && parser.tokens.get(parser.at + 1)
                                == Some(&Token::Ident("as".to_string()))
                        {
                            parser.at += 2;
                            let ty = parser.ty();
                            self.vars.insert(name, ty);
                        }
                    }
                }
                "from" if depth < MAX_IMPORT_DEPTH => {
                    if let Some(Token::Str(specifier)) = parser.peek().cloned() {
                        parser.at += 1;
                        if let Some(file) = resolve_import(path, &specifier) {
                            if seen.insert(file.clone()) {
                                if let Ok(source) = std::fs::read_to_string(&file) {
                                    self.read(&source, &file, depth + 1, seen);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// The strings `expression` can hold at byte `at` of the component
    /// `text`, `None` when its type is not known to be a union of strings.
    pub fn strings_of(&self, text: &str, at: usize, expression: &str) -> Option<Vec<String>> {
        let bindings = self.bindings(&text[..at.min(text.len())]);
        let ty = self.expression(expression, &bindings)?;
        let mut strings = Vec::new();
        self.literals(&ty, &mut strings, 0)?;
        let mut seen = HashSet::new();
        strings.retain(|s| seen.insert(s.clone()));
        Some(strings)
    }

    /// The names bound by the markup before a point: `{#each xs as x}`
    /// (an element of `xs`) and `{#snippet name(x: T)}`; the last one wins.
    fn bindings(&self, before: &str) -> HashMap<String, Ty> {
        let mut bound: HashMap<String, Ty> = HashMap::new();
        let mut rest = before;
        while let Some(open) = rest.find("{#") {
            rest = &rest[open + 2..];
            let close = rest.find('}').unwrap_or(rest.len());
            let tag = &rest[..close];
            if let Some(each) = tag.strip_prefix("each ") {
                let Some(as_at) = each.rfind(" as ") else {
                    continue;
                };
                let binding: String = each[as_at + 4..]
                    .trim_start()
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                    .collect();
                if binding.is_empty() {
                    continue;
                }
                let element = self
                    .expression(each[..as_at].trim(), &bound)
                    .and_then(|ty| self.element(&ty, 0))
                    .unwrap_or(Ty::Unknown);
                bound.insert(binding, element);
            } else if let Some(snippet) = tag.strip_prefix("snippet ") {
                let Some(paren) = snippet.find('(') else {
                    continue;
                };
                let tokens = tokenize(&snippet[paren + 1..]);
                let mut parser = Parser {
                    tokens: &tokens,
                    at: 0,
                };
                while let Some(Token::Ident(name)) = parser.peek().cloned() {
                    parser.at += 1;
                    parser.eat_punct('?');
                    let ty = if parser.eat_punct(':') {
                        parser.ty()
                    } else {
                        Ty::Unknown
                    };
                    bound.insert(name, ty);
                    // A default value: up to the next parameter.
                    while parser.peek().is_some() && !parser.is_punct(',') && !parser.is_punct(')')
                    {
                        parser.at += 1;
                    }
                    if !parser.eat_punct(',') {
                        break;
                    }
                }
            }
        }
        bound
    }

    /// The type of `a.b[0]?.c!`, a path from a name.
    fn expression(&self, expression: &str, bound: &HashMap<String, Ty>) -> Option<Ty> {
        let tokens = tokenize(expression);
        let mut parser = Parser {
            tokens: &tokens,
            at: 0,
        };
        let Some(Token::Ident(root)) = parser.peek().cloned() else {
            return None;
        };
        parser.at += 1;
        let mut ty = bound.get(&root).or_else(|| self.vars.get(&root))?.clone();
        while let Some(token) = parser.peek().cloned() {
            parser.at += 1;
            match token {
                Token::Punct('!') => {}
                Token::Punct('.') | Token::Other => {
                    let Some(Token::Ident(name)) = parser.peek().cloned() else {
                        return None;
                    };
                    parser.at += 1;
                    ty = self.member(&ty, &name, 0)?;
                }
                Token::Punct('[') => {
                    parser.at -= 1;
                    parser.skip_balanced();
                    ty = self.element(&ty, 0)?;
                }
                _ => return None,
            }
        }
        Some(ty)
    }

    fn named(&self, name: &str) -> Option<&Ty> {
        self.types.get(name)
    }

    fn member(&self, ty: &Ty, name: &str, depth: usize) -> Option<Ty> {
        if depth > MAX_ALIASES {
            return None;
        }
        match ty {
            Ty::Named(n) => self.member(self.named(n)?, name, depth + 1),
            Ty::Object(members) => members.iter().find(|(m, _)| m == name).map(|(_, t)| t.clone()),
            // `T | null`: the member of what is not nothing.
            Ty::Union(options) => {
                let found: Vec<Ty> = options
                    .iter()
                    .filter(|o| !matches!(o, Ty::Literals(l) if l.is_empty()))
                    .map(|o| self.member(o, name, depth + 1))
                    .collect::<Option<_>>()?;
                Some(Ty::Union(found))
            }
            _ => None,
        }
    }

    fn element(&self, ty: &Ty, depth: usize) -> Option<Ty> {
        if depth > MAX_ALIASES {
            return None;
        }
        match ty {
            Ty::Named(n) => self.element(self.named(n)?, depth + 1),
            Ty::Array(element) => Some((**element).clone()),
            Ty::Union(options) => {
                let found: Vec<Ty> = options
                    .iter()
                    .filter(|o| !matches!(o, Ty::Literals(l) if l.is_empty()))
                    .map(|o| self.element(o, depth + 1))
                    .collect::<Option<_>>()?;
                Some(Ty::Union(found))
            }
            _ => None,
        }
    }

    /// Adds the strings of `ty` to `into`; `None` when it holds others.
    fn literals(&self, ty: &Ty, into: &mut Vec<String>, depth: usize) -> Option<()> {
        if depth > MAX_ALIASES {
            return None;
        }
        match ty {
            Ty::Literals(strings) => into.extend(strings.iter().cloned()),
            Ty::Named(n) => self.literals(self.named(n)?, into, depth + 1)?,
            Ty::Union(options) => {
                for option in options {
                    self.literals(option, into, depth + 1)?;
                }
            }
            _ => return None,
        }
        Some(())
    }
}

/// The content of each `<script>` of a component.
fn scripts(text: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find("<script") {
        let after = &rest[open..];
        let Some(start) = after.find('>') else {
            break;
        };
        let body = &after[start + 1..];
        let end = body.find("</script>").unwrap_or(body.len());
        found.push(&body[..end]);
        rest = &body[end..];
    }
    found
}

/// The file a relative import of `from` names: as written, else with `.ts`,
/// else its `index.ts`. Package imports are not read.
fn resolve_import(from: &Path, specifier: &str) -> Option<PathBuf> {
    if !specifier.starts_with('.') {
        return None;
    }
    let base = from.parent()?.join(specifier);
    let candidates = [
        base.clone(),
        base.with_extension("ts"),
        base.join("index.ts"),
    ];
    candidates.into_iter().find(|p| {
        p.is_file() && p.extension().is_some_and(|e| e == "ts" || e == "js")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(scope: &Scope, text: &str, expression: &str) -> Option<Vec<String>> {
        let at = text.find("{t.").unwrap_or(text.len());
        scope.strings_of(text, at, expression)
    }

    #[test]
    fn local_alias_through_each_and_interface() {
        let text = r#"<script lang="ts">
  type Etat = 'fait' | 'encours' | 'avenir'
  interface Etape {
    readonly cle:
      | 'noyau'
      | 'routeur'
    readonly etat: Etat
    aide?(x: number): string
  }
  const ETAPES: readonly Etape[] = [{ cle: 'noyau', etat: 'fait' }]
</script>
{#each ETAPES as etape (etape.cle)}
  <span>{t.avancement[etape.etat]}</span>
{/each}"#;
        let scope = Scope::of_component(Path::new("/x/A.spd"), text);
        assert_eq!(
            strings(&scope, text, "etape.etat"),
            Some(vec!["fait".into(), "encours".into(), "avenir".into()])
        );
        assert_eq!(
            strings(&scope, text, "etape.cle"),
            Some(vec!["noyau".into(), "routeur".into()])
        );
        assert_eq!(strings(&scope, text, "etape.inconnu"), None);
        assert_eq!(strings(&scope, text, "autre"), None);
    }

    #[test]
    fn imported_types_nested_each_and_casts() {
        let root = std::env::temp_dir().join(format!("spd-types-{}", std::process::id()));
        let sections = root.join("src/sections");
        std::fs::create_dir_all(&sections).unwrap();
        std::fs::write(
            root.join("src/data.ts"),
            r#"import progress from './progress.json'
export type State = 'done' | 'partial' // reste
  | 'missing'
export interface Item { name: string; state: State }
export interface Domain {
  name: string
  items: Item[]
}
export const PROGRESS = progress as { updated: string; domains: Domain[] }
export type Platform = 'windows' | 'linux'
export const PACKAGES: ReadonlyArray<{ platform: Platform; size: number | null }> = []
"#,
        )
        .unwrap();
        let text = r#"<script lang="ts">
  import { PROGRESS, PACKAGES } from '../data.ts'
  import type { Item } from '../data'
</script>
{#each PROGRESS.domains as domain (domain.name)}
  {#each domain.items as item (item.name)}
    {#snippet ligne(x: Item, n = 2)}{/snippet}
    <span>{t.progress[item.state]}</span>
  {/each}
{/each}"#;
        let component = sections.join("Progress.spd");
        let scope = Scope::of_component(&component, text);
        assert_eq!(
            strings(&scope, text, "item.state"),
            Some(vec!["done".into(), "partial".into(), "missing".into()])
        );
        assert_eq!(
            strings(&scope, text, "x.state"),
            Some(vec!["done".into(), "partial".into(), "missing".into()])
        );
        assert_eq!(
            scope.strings_of(text, text.len(), "PACKAGES[0].platform"),
            Some(vec!["windows".into(), "linux".into()])
        );
        // A plain string restricts nothing.
        assert_eq!(strings(&scope, text, "item.name"), None);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn bindings_only_count_before_the_read() {
        let text = "<script lang=\"ts\">\n  type A = 'a'\n  type B = 'b'\n  const xs: A[] = []\n  const ys: B[] = []\n</script>\n{#each xs as x}{t.g[x]}{/each}\n{#each ys as x}{/each}";
        let scope = Scope::of_component(Path::new("/x/A.spd"), text);
        assert_eq!(strings(&scope, text, "x"), Some(vec!["a".into()]));
    }

    #[test]
    fn nullable_unions_keep_their_strings() {
        let text = "<script lang=\"ts\">\n  let choix: 'oui' | 'non' | null = null\n</script>";
        let scope = Scope::of_component(Path::new("/x/A.spd"), text);
        assert_eq!(
            scope.strings_of(text, text.len(), "choix!"),
            Some(vec!["oui".into(), "non".into()])
        );
    }
}
