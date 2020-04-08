#version 450

out gl_PerVertex {
    vec4 gl_Position;
};

layout(location = 0) out vec2 f_tex_coord;
layout(location = 1) out vec3 f_position;
layout(location = 2) out vec3 f_normal;
layout(location = 3) out float f_depth;
layout(location = 4) out vec3 f_color;

layout(location = 0) in vec3 v_position;
layout(location = 1) in vec2 v_tex_coord;
layout(location = 2) in vec3 v_normal;

layout(location = 4) in vec3 i_position;
layout(location = 5) in vec3 i_scale;
layout(location = 6) in vec3 i_color;

layout(binding = 0) uniform Locals {
    mat4 u_transform;
};

void main() {
    f_position = vec3(i_scale * v_position + i_position);
    f_tex_coord = v_tex_coord;
    f_normal = v_normal;

    f_color = i_color;

    vec4 screen = u_transform * vec4(f_position, 1.0);
    f_depth = screen.z / screen.w;
    gl_Position = screen;
}


