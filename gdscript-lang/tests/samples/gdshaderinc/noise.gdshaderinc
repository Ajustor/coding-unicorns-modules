// Noise helpers.
#ifndef NOISE_INC
#define NOISE_INC
#include "res://shaders/common.gdshaderinc"

struct Light { vec3 color; float energy; };

float hash(vec2 p) {
	return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
}

vec3 tint(vec3 c, Light l) {
	return c * l.color * l.energy;
}
#endif
