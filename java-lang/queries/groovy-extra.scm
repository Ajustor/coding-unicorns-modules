; Coding Unicorns additions to the Groovy grammar's own query. They come
; after it: when two patterns capture the same node, the last one wins.

; The closing slash of a /slashy/ string (the grammar's query paints every
; "/" as an operator).
(string_literal "/" @string)

; The code inside "${…}" is not text: brackets and operators there keep the
; default color instead of the string's.
(gstring_brace_interpolation) @variable

; Gradle's `implementation group: 'g', name: 'n', version: '1'`: the grammar
; has no call without parentheses taking named arguments, and reads it as a
; lone name followed by labels. Color it as the call it is.
((expression_statement . (identifier) @function .) . (labeled_statement))
(labeled_statement . (identifier) @property)
