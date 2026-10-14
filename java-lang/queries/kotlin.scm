; Kotlin (tree-sitter-kotlin-ng): Gradle Kotlin scripts (`*.gradle.kts`,
; `*.kts`) and `.kt` files. The grammar ships no highlight query; this one is
; written for its node types (src/node-types.json of tree-sitter-kotlin-ng 1.1.0).

(line_comment) @comment
(block_comment) @comment
(shebang) @comment

(string_literal) @string
(multiline_string_literal) @string
(character_literal) @character
(escape_sequence) @escape
; `$name` and `${…}` inside a string are code, not text.
(interpolation) @variable

(number_literal) @number
(float_literal) @float

[(this_expression) (super_expression)] @variable.builtin

(user_type (identifier) @type)
(class_declaration name: (identifier) @type)
(object_declaration name: (identifier) @type)

(function_declaration name: (identifier) @function)
; `println(…)`, `implementation("…")`, `plugins { … }`
(call_expression . (identifier) @function)
; `project.file(…)`, `tasks.register(…)`
(call_expression . (navigation_expression (identifier) @function .))

(package_header (qualified_identifier) @namespace)
(import (qualified_identifier) @namespace)

(annotation) @attribute
(label) @label

[
  "abstract" "actual" "annotation" "as" "by" "catch" "class" "companion"
  "const" "constructor" "crossinline" "data" "do" "else" "enum" "expect"
  "external" "final" "finally" "for" "fun" "get" "if" "import" "in" "infix"
  "init" "inline" "inner" "interface" "internal" "is" "lateinit" "noinline"
  "object" "open" "operator" "out" "override" "package" "private"
  "protected" "public" "return" "sealed" "set" "super" "suspend" "tailrec"
  "this" "throw" "try" "typealias" "val" "value" "var" "vararg" "when"
  "where" "while"
] @keyword
