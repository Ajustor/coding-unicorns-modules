; Godot scene and resource files (.tscn, .tres, project.godot, .import,
; .gdextension), written for tree-sitter-godot-resource (whose crate ships no
; query). Capture names: see unicorns-highlight. When several patterns capture
; the same node, the last one wins.

(comment) @comment
(string) @string
[(integer) (float)] @number
[(true) (false) (null)] @constant.builtin

; [gd_scene load_steps=3 format=3], [node name="Player" type="Node2D"]
(section (identifier) @keyword)
(attribute (identifier) @property)

; key = value, config/name = "…"
(property (path) @property)

; Vector2(0, 0), ExtResource("1_abc"), Array[int]([…]), Object(InputEventKey, …)
(constructor (identifier) @type)
(arguments . (identifier) @type)

; Bare values: inf, nan
((identifier) @number (#any-of? @number "inf" "inf_neg" "nan"))

; &"StringName" (Godot 4): the grammar does not know `&` and leaves it in an
; ERROR node.
((ERROR) @string (#eq? @string "&"))
