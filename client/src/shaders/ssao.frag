#version 450

layout(location = 0) out float out_color;

layout(location = 0) in vec2 tex_coord;

layout(binding = 0, std140) uniform Locals {
    mat4 u_transform;
    vec3 u_camera_pos;
};

const int SAMPLE_COUNT = 64;
const float RADIUS = 0.05;
const float BIAS = 0.05;

layout(binding = 1, std140) uniform Kernel {
    vec3 u_kernel[SAMPLE_COUNT];
};

layout(set = 0, binding = 2) uniform sampler g_sampler;
layout(set = 0, binding = 3) uniform texture2D g_position;
layout(set = 0, binding = 4) uniform texture2D g_normal;

/// Generate a random number.
/// Lifted from: https://stackoverflow.com/questions/4200224/random-noise-functions-for-glsl
float random(vec2 co){
    return fract(sin(dot(co.xy, vec2(12.9898,78.233))) * 43758.5453);
}

// Based on https://learnopengl.com/Advanced-Lighting/SSAO
void main() {
    vec3 f_position = texture(sampler2D(g_position, g_sampler), tex_coord).xyz;
    vec3 f_normal = texture(sampler2D(g_normal, g_sampler), tex_coord).xyz;
    float f_depth = distance(u_camera_pos, f_position);

    vec3 random_vec = vec3(
            random(tex_coord + vec2(1, 2)), 
            random(tex_coord + vec2(2, 3)), 
            0.0
        );

    vec3 tangent = normalize(random_vec - f_normal * dot(random_vec, f_normal));
    vec3 bitangent = cross(f_normal, tangent);
    mat3 btn = mat3(tangent, bitangent, f_normal);
    
    float occlusion = 0;

    for (int i = 0; i < SAMPLE_COUNT; i++) {
        vec3 sample_dir = btn * u_kernel[i];
        vec3 sample_pos = f_position + RADIUS * sample_dir;
        vec4 screen = u_transform * vec4(sample_pos, 1.0);
        screen.xyz /= screen.w;
        screen.xy = screen.xy * 0.5 + 0.5;

        vec3 actual_pos = texture(sampler2D(g_position, g_sampler), screen.xy).xyz;

        float sample_depth = distance(u_camera_pos, sample_pos);
        float actual_depth = distance(u_camera_pos, actual_pos);

        float range = smoothstep(0.0, 1.0, RADIUS / abs(f_depth - sample_depth));
        occlusion += (sample_depth + BIAS <= actual_depth ? 1.0 : 0.0) * range;
    }

    out_color = 1.0 - occlusion / SAMPLE_COUNT;
}
