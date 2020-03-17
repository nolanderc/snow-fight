#version 450

layout(location = 0) out vec4 out_color;

layout(location = 0) in vec2 tex_coord;

layout(binding = 0, std140) uniform Locals {
    mat4 u_transform;
    vec3 u_camera_pos; 
    vec3 u_light_pos; 
};

layout(set = 0, binding = 1) uniform sampler g_sampler;
layout(set = 0, binding = 2) uniform texture2D g_color;
layout(set = 0, binding = 3) uniform texture2D g_normal;
layout(set = 0, binding = 4) uniform texture2D g_position;

const float EDGE_DEPTH = 0.001;
const float EDGE_NORMAL = 0.01;

const float LIGHT_AMBIENT = 0.4;

vec4 color;
vec3 normal;
vec3 position;
float depth;

/// Initialize global variables
void init() {
    color = texture(sampler2D(g_color, g_sampler), tex_coord);
    normal = texture(sampler2D(g_normal, g_sampler), tex_coord).xyz;
    depth = texture(sampler2D(g_normal, g_sampler), tex_coord).w;
    position = texture(sampler2D(g_position, g_sampler), tex_coord).xyz;
}

/// Find edges using a sobel kernel.
vec4 edge(texture2D image) {
    ivec2 size = textureSize(sampler2D(image, g_sampler), 0);
    vec2 delta = 1.0 / size;

    vec4 sum = vec4(0.0);

    for (int x = -1; x < 2; x++) {
        for (int y = -1; y < 2; y++) {
            float scalar = (x == 0 && y == 0) ? 8.0 : -1.0;
            vec4 value = texture(sampler2D(image, g_sampler), tex_coord + delta * vec2(x, y));
            sum += scalar * value;
        }
    }

    return sum;
}

/// Find the outline of geometry using the normal and depth buffer.
float outline() {
    vec3 camera_dir = normalize(position - u_camera_pos);
    float incidence = dot(normal, -camera_dir);

    vec4 edges = edge(g_normal);

    bool edge_normal = length(edges.xyz) > EDGE_NORMAL;
    bool edge_depth = abs(edges.w) > depth * mix(5.0 * EDGE_DEPTH, EDGE_DEPTH, incidence);

    return edge_normal || edge_depth ? 1.0 : 0.0;
}

/// Calculate lighting using the Phong model
float phong() {
    vec3 light_delta = vec3(1.5, 2.0, -2.5);
    vec3 light_dir = normalize(light_delta);

    float incoming = max(0.0, dot(normal, -light_dir));
    float brightness = LIGHT_AMBIENT + (1 - LIGHT_AMBIENT) * incoming;

    return brightness;
}

void main() {
    init();

    float outline = outline();
    float brightness = phong();

    if (depth > 2) {
        out_color = vec4(0.2);
    } else {
        out_color = brightness * color * vec4(1.0 - outline);
    }
}
