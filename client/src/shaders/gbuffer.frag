#version 450

layout(location = 0) out vec4 out_color;
layout(location = 1) out vec4 out_normal;
layout(location = 2) out vec4 out_position;

layout(location = 0) in vec2 f_tex_coord;
layout(location = 1) in vec3 f_position;
layout(location = 2) in vec3 f_normal;
layout(location = 3) in float f_depth;
layout(location = 4) in vec3 f_color;

layout(set = 1, binding = 0) uniform sampler u_sampler;
layout(set = 1, binding = 1) uniform texture2D u_texture;

void main() {
    vec4 base_color = texture(sampler2D(u_texture, u_sampler), f_tex_coord);

    if (base_color.a < 1.0) {
        discard;
    } else {
        out_color = vec4(base_color.rgb * f_color, 1.0);
        out_normal = vec4(f_normal, f_depth);
        out_position = vec4(f_position, f_depth);
    }
}

