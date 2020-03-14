#version 450

layout(location = 0) out vec4 out_color;
layout(location = 1) out vec4 out_normal;
layout(location = 2) out vec4 out_position;

layout(location = 0) in vec4 color;
layout(location = 1) in vec3 position;
layout(location = 2) in vec3 normal;
layout(location = 3) in float depth;

void main() {
    out_color = color;
    out_normal = vec4(normal, depth);
    out_position = vec4(position, depth);
}

