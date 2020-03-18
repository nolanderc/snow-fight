#version 450

layout(location = 0) out vec4 out_color;

layout(location = 0) in vec2 tex_coord;

layout(binding = 0, std140) uniform Locals {
    mat4 u_transform;
    vec3 u_camera_pos; 
    vec3 u_light_pos; 
    float u_camera_far;
};

layout(set = 0, binding = 1) uniform sampler g_sampler;
layout(set = 0, binding = 2) uniform texture2D g_color;
layout(set = 0, binding = 3) uniform texture2D g_normal;
layout(set = 0, binding = 4) uniform texture2D g_position;

const float EDGE_DEPTH = 0.01;
const float EDGE_NORMAL = 0.01;

const float LIGHT_AMBIENT = 0.4;

vec4 f_color;
vec3 f_normal;
vec3 f_position;
float f_depth;
float f_distance;

/// Initialize global variables
void init() {
    f_color = texture(sampler2D(g_color, g_sampler), tex_coord);
    f_normal = texture(sampler2D(g_normal, g_sampler), tex_coord).xyz;
    f_depth = texture(sampler2D(g_normal, g_sampler), tex_coord).w;
    f_position = texture(sampler2D(g_position, g_sampler), tex_coord).xyz;
    f_distance = distance(f_position, u_camera_pos);
}

/// Find edges using a sobel kernel.
vec4 edge() {
    ivec2 size = textureSize(sampler2D(g_normal, g_sampler), 0);
    vec2 delta = 1.0 / size;

    vec4 sum = vec4(0.0);

    for (int x = -1; x < 1; x++) {
        for (int y = -1; y < 1; y++) {
            float scalar = (x == 0 && y == 0) ? 3.0 : -1.0;

            vec2 tex = tex_coord + delta * vec2(x, y);

            vec3 normal = texture(sampler2D(g_normal, g_sampler), tex).xyz;

            vec3 position = texture(sampler2D(g_position, g_sampler), tex).xyz;
            float depth = f_depth > 2 ? f_depth : distance(position, u_camera_pos);

            sum += scalar * vec4(normal, depth);

        }
    }

    return sum;
}

/// Find the outline of geometry using the normal and depth buffer.
float outline() {
    vec3 camera_dir = normalize(f_position - u_camera_pos);
    float incidence = dot(f_normal, -camera_dir);

    vec4 edges = edge();

    bool edge_normal = length(edges.xyz) > EDGE_NORMAL;
    bool edge_depth = abs(edges.w) > f_distance * mix(EDGE_DEPTH, 16 * EDGE_DEPTH, 1.0 - incidence);

    return edge_normal || edge_depth ? 1.0 : 0.0;
}

/// Calculate lighting using the Phong model
float phong() {
    vec3 light_delta = vec3(1.5, 2.0, -2.5);
    vec3 light_dir = normalize(light_delta);

    float incoming = max(0.0, dot(f_normal, -light_dir));
    float brightness = LIGHT_AMBIENT + (1 - LIGHT_AMBIENT) * incoming;

    return brightness;
}

void main() {
    init();

    float outline = outline();
    float brightness = phong();

    out_color = brightness * f_color * vec4(1.0 - outline);
}
