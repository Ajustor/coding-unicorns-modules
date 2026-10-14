; Groovy (tree-sitter-groovy): Gradle build scripts (`*.gradle`) and
; `.groovy` files. The grammar ships no highlight query; this one is written
; for its node types (src/node-types.json of tree-sitter-groovy 0.1.2).

(line_comment) @comment
(block_comment) @comment
(shebang) @comment

(string_literal) @string
(character_literal) @character
(escape_sequence) @escape
; `${…}` inside a GString is code, not text.
(string_interpolation) @variable

[
  (decimal_integer_literal)
  (hex_integer_literal)
  (octal_integer_literal)
  (binary_integer_literal)
] @number
[
  (decimal_floating_point_literal)
  (hex_floating_point_literal)
] @float

[(true) (false)] @boolean
(null_literal) @constant.builtin
[(this) (super)] @variable.builtin

(type_identifier) @type
[
  (integral_type)
  (floating_point_type)
  (boolean_type)
  (void_type)
] @type
(class_declaration name: (identifier) @type)
(interface_declaration name: (identifier) @type)
(enum_declaration name: (identifier) @type)

(method_declaration name: (identifier) @function)
(function_definition name: (identifier) @function)
(constructor_declaration name: (identifier) @constructor)
(method_invocation name: (identifier) @function)
; Gradle's `implementation 'group:name:1.0'`, `id 'java'`, `apply plugin: …`
(juxt_function_call name: (identifier) @function)

; `group: 'org.example', name: 'lib'`
(map_item key: (identifier) @property)
(field_access field: (identifier) @property)

(annotation name: (identifier) @attribute)
(marker_annotation name: (identifier) @attribute)

[
  "abstract" "as" "assert" "break" "case" "catch" "class" "continue" "def"
  "default" "do" "else" "enum" "extends" "final" "finally" "for" "if"
  "implements" "import" "in" "instanceof" "interface" "native" "new"
  "package" "private" "protected" "public" "return" "static" "strictfp"
  "switch" "synchronized" "throw" "throws" "transient" "try" "volatile"
  "while"
] @keyword
