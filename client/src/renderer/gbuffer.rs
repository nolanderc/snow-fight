use super::{Instance, Shaders, Size, Vertex};

use cgmath::{prelude::*, Matrix4};
use std::sync::Arc;
use wgpu_shader::VertexLayout;

pub struct GBuffer {
    device: Arc<wgpu::Device>,

    // Buffer attachments
    color: wgpu::TextureView,
    normal: wgpu::TextureView,
    position: wgpu::TextureView,

    depth: wgpu::TextureView,

    pipeline: wgpu::RenderPipeline,

    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct Uniforms {
    pub transform: cgmath::Matrix4<f32>,
}

impl Default for Uniforms {
    fn default() -> Self {
        Uniforms {
            transform: Matrix4::identity(),
        }
    }
}

struct Bindings<'a> {
    uniforms: &'a wgpu::Buffer,
}

impl GBuffer {
    const COLOR_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;
    const NORMAL_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;
    const POSITION_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;
    const DEPTH_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    const COLOR_CLEAR_COLOR: wgpu::Color = wgpu::Color::BLACK;
    const NORMAL_CLEAR_COLOR: wgpu::Color = wgpu::Color {
        a: 1e6,
        ..wgpu::Color::BLACK
    };
    const POSITION_CLEAR_COLOR: wgpu::Color = wgpu::Color::BLACK;

    const BIND_GROUP_BINDINGS: &'static [wgpu::BindGroupLayoutBinding] =
        &[wgpu::BindGroupLayoutBinding {
            binding: 0,
            visibility: wgpu::ShaderStage::VERTEX,
            ty: wgpu::BindingType::UniformBuffer { dynamic: true },
        }];

    const COLOR_STATES: &'static [wgpu::ColorStateDescriptor] = &[
        // Color
        wgpu::ColorStateDescriptor {
            format: Self::COLOR_TEXTURE_FORMAT,
            color_blend: wgpu::BlendDescriptor::REPLACE,
            alpha_blend: wgpu::BlendDescriptor::REPLACE,
            write_mask: wgpu::ColorWrite::COLOR,
        },
        // Normal
        wgpu::ColorStateDescriptor {
            format: Self::NORMAL_TEXTURE_FORMAT,
            color_blend: wgpu::BlendDescriptor::REPLACE,
            alpha_blend: wgpu::BlendDescriptor::REPLACE,
            write_mask: wgpu::ColorWrite::ALL,
        },
        // Position
        wgpu::ColorStateDescriptor {
            format: Self::POSITION_TEXTURE_FORMAT,
            color_blend: wgpu::BlendDescriptor::REPLACE,
            alpha_blend: wgpu::BlendDescriptor::REPLACE,
            write_mask: wgpu::ColorWrite::ALL,
        },
    ];

    const DEPTH_STENCIL_STATE: wgpu::DepthStencilStateDescriptor =
        wgpu::DepthStencilStateDescriptor {
            format: Self::DEPTH_TEXTURE_FORMAT,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil_front: wgpu::StencilStateFaceDescriptor::IGNORE,
            stencil_back: wgpu::StencilStateFaceDescriptor::IGNORE,
            stencil_read_mask: 0,
            stencil_write_mask: 0,
        };

    const VERTEX_BUFFERS: &'static [wgpu::VertexBufferDescriptor<'static>] = &[
        wgpu::VertexBufferDescriptor {
            stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::InputStepMode::Vertex,
            attributes: Vertex::ATTRIBUTES,
        },
        wgpu::VertexBufferDescriptor {
            stride: std::mem::size_of::<Instance>() as u64,
            step_mode: wgpu::InputStepMode::Instance,
            attributes: Instance::ATTRIBUTES,
        },
    ];

    pub(super) fn new(device: Arc<wgpu::Device>, size: Size) -> GBuffer {
        let color = Self::create_buffer_texture(&device, size, Self::COLOR_TEXTURE_FORMAT);
        let normal = Self::create_buffer_texture(&device, size, Self::NORMAL_TEXTURE_FORMAT);
        let position = Self::create_buffer_texture(&device, size, Self::POSITION_TEXTURE_FORMAT);

        let depth = Self::create_buffer_texture(&device, size, Self::DEPTH_TEXTURE_FORMAT);

        let bind_group_layout = Self::create_bind_group_layout(&device);
        let pipeline = Self::create_render_pipeline(&device, size, &bind_group_layout);

        let uniform_buffer = Self::create_uniform_buffer(&device, Uniforms::default());

        let bindings = Bindings {
            uniforms: &uniform_buffer,
        };
        let bind_group = Self::create_bind_group(&device, &bind_group_layout, bindings);

        GBuffer {
            device,

            color,
            normal,
            position,

            depth,

            pipeline,

            bind_group,
            uniform_buffer,
        }
    }

    fn create_buffer_texture(
        device: &wgpu::Device,
        size: Size,
        format: wgpu::TextureFormat,
    ) -> wgpu::TextureView {
        let descriptor = wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: size.width,
                height: size.height,
                depth: 1,
            },
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: size.samples,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsage::WRITE_ALL | wgpu::TextureUsage::READ_ALL,
        };

        device.create_texture(&descriptor).create_default_view()
    }

    fn create_render_pipeline(
        device: &wgpu::Device,
        size: Size,
        bind_group_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::RenderPipeline {
        let layout = Self::create_pipeline_layout(device, bind_group_layout);

        let vertex_path = "src/shaders/gbuffer.vert.spv";
        let fragment_path = "src/shaders/gbuffer.frag.spv";
        let shaders = Shaders::open(&device, vertex_path, fragment_path).unwrap();

        let descriptor = wgpu::RenderPipelineDescriptor {
            layout: &layout,
            vertex_stage: shaders.vertex_stage(),
            fragment_stage: Some(shaders.fragment_stage()),
            rasterization_state: Some(Default::default()),
            primitive_topology: wgpu::PrimitiveTopology::TriangleList,
            color_states: Self::COLOR_STATES,
            depth_stencil_state: Some(Self::DEPTH_STENCIL_STATE),
            index_format: wgpu::IndexFormat::Uint32,
            vertex_buffers: Self::VERTEX_BUFFERS,
            sample_count: size.samples,
            sample_mask: !0,
            alpha_to_coverage_enabled: false,
        };

        device.create_render_pipeline(&descriptor)
    }

    fn create_pipeline_layout(
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::PipelineLayout {
        let descriptor = wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &[bind_group_layout],
        };

        device.create_pipeline_layout(&descriptor)
    }

    fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        let descriptor = wgpu::BindGroupLayoutDescriptor {
            bindings: Self::BIND_GROUP_BINDINGS,
        };
        device.create_bind_group_layout(&descriptor)
    }

    fn create_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        bindings: Bindings,
    ) -> wgpu::BindGroup {
        let descriptor = wgpu::BindGroupDescriptor {
            bindings: &[wgpu::Binding {
                binding: 0,
                resource: wgpu::BindingResource::Buffer {
                    buffer: bindings.uniforms,
                    range: 0..std::mem::size_of::<Uniforms>() as u64,
                },
            }],
            layout,
        };

        device.create_bind_group(&descriptor)
    }

    fn create_uniform_buffer(device: &wgpu::Device, uniforms: Uniforms) -> wgpu::Buffer {
        device
            .create_buffer_mapped(1, wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST)
            .fill_from_slice(&[uniforms])
    }

    pub fn begin_render_pass<'a>(
        &self,
        encoder: &'a mut wgpu::CommandEncoder,
        uniforms: Uniforms,
    ) -> wgpu::RenderPass<'a> {
        self.update_uniforms(encoder, uniforms);

        let color = Self::color_attachment(&self.color, Self::COLOR_CLEAR_COLOR);
        let normal = Self::color_attachment(&self.normal, Self::NORMAL_CLEAR_COLOR);
        let position = Self::color_attachment(&self.position, Self::POSITION_CLEAR_COLOR);

        let depth = Self::depth_attachment(&self.depth);

        let descriptor = wgpu::RenderPassDescriptor {
            color_attachments: &[color, normal, position],
            depth_stencil_attachment: Some(depth),
        };

        let mut render_pass = encoder.begin_render_pass(&descriptor);

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[0]);

        render_pass
    }

    fn update_uniforms(&self, encoder: &mut wgpu::CommandEncoder, uniforms: Uniforms) {
        let staging = self
            .device
            .create_buffer_mapped(1, wgpu::BufferUsage::COPY_SRC)
            .fill_from_slice(&[uniforms]);

        encoder.copy_buffer_to_buffer(
            &staging,
            0,
            &self.uniform_buffer,
            0,
            std::mem::size_of_val(&uniforms) as u64,
        );
    }

    fn color_attachment(
        attachment: &wgpu::TextureView,
        clear_color: wgpu::Color,
    ) -> wgpu::RenderPassColorAttachmentDescriptor {
        wgpu::RenderPassColorAttachmentDescriptor {
            attachment,
            resolve_target: None,
            clear_color,
            load_op: wgpu::LoadOp::Clear,
            store_op: wgpu::StoreOp::Store,
        }
    }

    fn depth_attachment<'a>(
        attachment: &'a wgpu::TextureView,
    ) -> wgpu::RenderPassDepthStencilAttachmentDescriptor<&'a wgpu::TextureView> {
        wgpu::RenderPassDepthStencilAttachmentDescriptor {
            attachment,
            clear_depth: 1.0,
            depth_load_op: wgpu::LoadOp::Clear,
            depth_store_op: wgpu::StoreOp::Store,
            clear_stencil: 0,
            stencil_load_op: wgpu::LoadOp::Clear,
            stencil_store_op: wgpu::StoreOp::Store,
        }
    }

    pub fn color_buffer(&self) -> &wgpu::TextureView {
        &self.color
    }

    pub fn normal_buffer(&self) -> &wgpu::TextureView {
        &self.normal
    }

    pub fn position_buffer(&self) -> &wgpu::TextureView {
        &self.position
    }
}
