shader_type canvas_item;
render_mode blend_mix;

/** Speed of the waves. */
uniform float speed : hint_range(0.0, 10.0) = 1.0;
uniform sampler2D noise : repeat_enable;
varying vec2 world_uv;

// Wobble the vertices.
void vertex() {
	world_uv = VERTEX * 0.01;
}

void fragment() {
	vec4 base = texture(TEXTURE, UV);
	float wave = sin(TIME * speed + world_uv.x * 6.28);
	COLOR = vec4(base.rgb * (0.8 + 0.2 * wave), base.a);
}
