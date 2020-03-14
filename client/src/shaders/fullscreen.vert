#version 450

out gl_PerVertex {
    vec4 gl_Position;
};

layout(location = 0) out vec2 tex_coord;

void main() {
    tex_coord = vec2(gl_VertexIndex & 1, (gl_VertexIndex >> 1) & 1);
    gl_Position = vec4(2.0 * tex_coord - 1.0, 0.0, 1.0);
}

