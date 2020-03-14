
pub use wgpu_shader_macros::*;

pub trait VertexLayout {
    const ATTRIBUTES: &'static [wgpu::VertexAttributeDescriptor];
}

