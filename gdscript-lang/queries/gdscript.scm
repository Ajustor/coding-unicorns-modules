; GDScript highlighting for Coding Unicorns, written for tree-sitter-gdscript
; (whose crate ships no query). Capture names: see unicorns-highlight. When
; several patterns capture the same node, the LAST one wins
; (tree-sitter-highlight 0.25): general patterns first, exceptions after.

(comment) @comment
[(region_start) (region_end)] @comment

; "text", &"StringName", ^"NodePath", $Node, %UniqueNode
[(string) (string_name) (node_path) (get_node)] @string
(escape_sequence) @escape
; A string standing alone is GDScript's block comment ("""…""").
(expression_statement (string) @comment)

[(integer) (float)] @number
[(true) (false) (null)] @constant.builtin

; @export, @onready, @tool…
(annotation "@" @attribute)
(annotation (identifier) @attribute)

; foo(), obj.foo(), super.foo()
(call (identifier) @function)
(attribute_call (identifier) @function)
(base_call (identifier) @function)

; Naming conventions: PascalCase is a class or an enum (Vector2(…), Node.new(),
; State.IDLE), CONSTANT_CASE a constant (PI, State.IDLE).
((identifier) @type (#match? @type "^[A-Z]"))
((identifier) @constant (#match? @constant "^[A-Z][A-Z0-9_]*$"))
((identifier) @variable.builtin
  (#any-of? @variable.builtin "self" "super"))

(function_definition name: (name) @function)
(lambda name: (name) @function)
(constructor_definition "_init" @function)
(signal_statement name: (name) @function)
[(setter) (getter)] @function
(class_name_statement name: (name) @type)
(class_definition name: (name) @type)
(enum_definition name: (name) @type)
(const_statement name: (name) @constant)
(enumerator left: (identifier) @constant)
(type) @type
(type (identifier) @type)
(type (subscript (subscript_arguments (identifier) @type)))
; x is int, y as AABB
(binary_operator op: ["is" "as"] right: (identifier) @type)

[
  "and" "as" "await" "break" "class" "class_name" "const" "continue"
  "elif" "else" "enum" "extends" "for" "func" "if" "in" "is" "match"
  "not" "or" "pass" "return" "signal" "var" "when" "while" "set" "get"
  "export" "onready" "setget"
] @keyword
[(static_keyword) (remote_keyword) (breakpoint_statement)] @keyword
