#version 450

out gl_PerVertex {
    vec4 gl_Position;
};

layout(location = 0) out vec4 f_color;

layout(location = 0) in vec2 v_position;
layout(location = 1) in vec3 v_color;

layout(set = 0, binding = 0) uniform Locals {
    mat4 u_transform;
};

void main() {
    gl_Position = u_transform * vec4(v_position, dot(v_position, v_position), 1.0);
    f_color = vec4(v_color, 1.0);
}


