; `.properties` files (tree-sitter-properties). Unlike the grammar's own
; query, a dotted key (`org.gradle.jvmargs`) stays one property.

(comment) @comment
(key) @property
(value) @string
(value (escape) @escape)
(substitution (key) @constant)
