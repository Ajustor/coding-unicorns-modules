; Java highlights, from tree-sitter-java's queries/highlights.scm with fields,
; records, constructors, `var` and Javadoc added (see the tests in src/lib.rs
; for which capture wins where patterns overlap).

; Comments

((block_comment) @comment.documentation
 (#match? @comment.documentation "^/\*\*"))
[
  (line_comment)
  (block_comment)
] @comment

; Annotations

(annotation
  name: (identifier) @attribute)
(marker_annotation
  name: (identifier) @attribute)
(annotation
  name: (scoped_identifier) @attribute)
(marker_annotation
  name: (scoped_identifier) @attribute)
"@" @attribute

; Declarations

(method_declaration
  name: (identifier) @function.method)
(constructor_declaration
  name: (identifier) @constructor)
(compact_constructor_declaration
  name: (identifier) @constructor)
(interface_declaration
  name: (identifier) @type)
(class_declaration
  name: (identifier) @type)
(enum_declaration
  name: (identifier) @type)
(record_declaration
  name: (identifier) @type)
(annotation_type_declaration
  name: (identifier) @type)
(enum_constant
  name: (identifier) @constant)
(field_declaration
  declarator: (variable_declarator
    name: (identifier) @property))

; Expressions

(method_invocation
  name: (identifier) @function.method)
(object_creation_expression
  type: (type_identifier) @constructor)
(field_access
  field: (identifier) @property)

((field_access
  object: (identifier) @type)
 (#match? @type "^[A-Z]"))
((scoped_identifier
  scope: (identifier) @type)
 (#match? @type "^[A-Z]"))
((method_invocation
  object: (identifier) @type)
 (#match? @type "^[A-Z]"))
((method_reference
  . (identifier) @type)
 (#match? @type "^[A-Z]"))

; Types

((type_identifier) @keyword
 (#eq? @keyword "var"))
((type_identifier) @type
 (#not-eq? @type "var"))
[
  (boolean_type)
  (integral_type)
  (floating_point_type)
  (void_type)
] @type.builtin

; Constants

((identifier) @constant
 (#match? @constant "^_*[A-Z][A-Z\d_]+$"))

; Builtins

(this) @variable.builtin
(super) @variable.builtin

; Literals

[
  (hex_integer_literal)
  (decimal_integer_literal)
  (octal_integer_literal)
  (binary_integer_literal)
  (decimal_floating_point_literal)
  (hex_floating_point_literal)
] @number

[
  (character_literal)
  (string_literal)
] @string
(escape_sequence) @string.escape

[
  (true)
  (false)
  (null_literal)
] @constant.builtin

; Keywords

[
  "abstract"
  "assert"
  "break"
  "case"
  "catch"
  "class"
  "continue"
  "default"
  "do"
  "else"
  "enum"
  "exports"
  "extends"
  "final"
  "finally"
  "for"
  "if"
  "implements"
  "import"
  "instanceof"
  "interface"
  "module"
  "native"
  "new"
  "non-sealed"
  "open"
  "opens"
  "package"
  "permits"
  "private"
  "protected"
  "provides"
  "public"
  "requires"
  "record"
  "return"
  "sealed"
  "static"
  "strictfp"
  "switch"
  "synchronized"
  "throw"
  "throws"
  "to"
  "transient"
  "transitive"
  "try"
  "uses"
  "volatile"
  "when"
  "while"
  "with"
  "yield"
] @keyword
