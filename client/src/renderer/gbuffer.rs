use super::{Instance, Shaders, Size, Vertex};

use zerocopy::AsBytes;

use cgmath::{prelude::*, Matrix4};

use std::sync::Arc;

use wgpu_shader::VertexLayout;

pub struct GBuffer {
    device: Arc<wgpu::Device>,

    // Buffer attachments
    color: BufferTexture,
    normal: BufferTexture,
    position: BufferTexture,

    depth: wgpu::TextureView,

    pipeline: wgpu::RenderPipeline,

    uniform_buffer: wgpu::Buffer,

    bind_group: wgpu::BindGroup,
    model_layout: wgpu::BindGroupLayout,
}

struct BufferTexture {
    view: wgpu::TextureView,
}

#[derive(Debug, Copy, Clone, AsBytes)]
#[repr(C)]
pub struct Uniforms {
    pub transform: [[f32; 4]; 4],
}

impl Default for Uniforms {
    fn default() -> Self {
        Uniforms {
            transform: Matrix4::identity().into(),
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

    const COLOR_CLEAR_COLOR: wgpu::Color = wgpu::Color {
        r: 0.9,
        g: 0.9,
        b: 0.9,
        a: 0.0,
    };
    const NORMAL_CLEAR_COLOR: wgpu::Color = wgpu::Color {
        a: 1e6,
        ..wgpu::Color::BLACK
    };
    const POSITION_CLEAR_COLOR: wgpu::Color = wgpu::Color {
        r: 1e6,
        g: 1e6,
        b: 1e6,
        a: 1e6,
    };

    const BIND_GROUP_BINDINGS: &'static [wgpu::BindGroupLayoutEntry] =
        &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStage::VERTEX,
            ty: wgpu::BindingType::UniformBuffer { dynamic: false },
        }];

    const MODEL_GROUP_BINDINGS: &'static [wgpu::BindGroupLayoutEntry] = &[
        wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStage::FRAGMENT,
            ty: wgpu::BindingType::Sampler { comparison: false },
        },
        wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: wgpu::ShaderStage::FRAGMENT,
            ty: wgpu::BindingType::SampledTexture {
                component_type: wgpu::TextureComponentType::Float,
                multisampled: false,
                dimension: wgpu::TextureViewDimension::D2,
            },
        },
    ];

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

        let depth = Self::create_buffer_texture(&device, size, Self::DEPTH_TEXTURE_FORMAT).view;

        let [main_layout, model_layout] = Self::create_bind_group_layouts(&device);
        let pipeline = Self::create_render_pipeline(&device, &[&main_layout, &model_layout]);

        let uniform_buffer = Self::create_uniform_buffer(&device, Uniforms::default());

        let bindings = Bindings {
            uniforms: &uniform_buffer,
        };

        let bind_group = Self::create_bind_group(&device, &main_layout, bindings);

        // 0x7fc753e840a0
        //
        // 0x7fc753e67480

        GBuffer {
            device,

            color,
            normal,
            position,

            depth,

            pipeline,

            uniform_buffer,

            bind_group,
            model_layout,
        }
    }

    fn create_buffer_texture(
        device: &wgpu::Device,
        size: Size,
        format: wgpu::TextureFormat,
    ) -> BufferTexture {
        let descriptor = wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: size.width,
                height: size.height,
                depth: 1,
            },
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsage::WRITE_ALL | wgpu::TextureUsage::READ_ALL,
        };

        let texture = device.create_texture(&descriptor);
        let view = texture.create_default_view();

        BufferTexture { view }
    }

    fn create_render_pipeline(
        device: &wgpu::Device,
        bind_group_layouts: &[&wgpu::BindGroupLayout],
    ) -> wgpu::RenderPipeline {
        let descriptor = wgpu::PipelineLayoutDescriptor { bind_group_layouts };
        let layout = device.create_pipeline_layout(&descriptor);

        let vertex_path = "src/shaders/gbuffer.vert.spv";
        let fragment_path = "src/shaders/gbuffer.frag.spv";
        let shaders = Shaders::open(&device, vertex_path, fragment_path).unwrap();

        let descriptor = wgpu::RenderPipelineDescriptor {
            layout: &layout,
            vertex_stage: shaders.vertex_stage(),
            fragment_stage: Some(shaders.fragment_stage()),
            rasterization_state: Some(wgpu::RasterizationStateDescriptor {
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: wgpu::CullMode::None,
                ..Default::default()
            }),
            primitive_topology: wgpu::PrimitiveTopology::TriangleList,
            color_states: Self::COLOR_STATES,
            depth_stencil_state: Some(Self::DEPTH_STENCIL_STATE),
            vertex_state: wgpu::VertexStateDescriptor {
                index_format: wgpu::IndexFormat::Uint32,
                vertex_buffers: Self::VERTEX_BUFFERS,
            },
            sample_count: 1,
            sample_mask: !0,
            alpha_to_coverage_enabled: false,
        };

        device.create_render_pipeline(&descriptor)
    }

    fn create_bind_group_layouts(device: &wgpu::Device) -> [wgpu::BindGroupLayout; 2] {
        let main_desc = wgpu::BindGroupLayoutDescriptor {
            label: None,
            bindings: Self::BIND_GROUP_BINDINGS,
        };

        let model_desc = wgpu::BindGroupLayoutDescriptor {
            label: None,
            bindings: Self::MODEL_GROUP_BINDINGS,
        };

        let main = device.create_bind_group_layout(&main_desc);
        let model = device.create_bind_group_layout(&model_desc);

        [main, model]
    }

    fn create_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        bindings: Bindings,
    ) -> wgpu::BindGroup {
        let descriptor = wgpu::BindGroupDescriptor {
            label: None,
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
        device.create_buffer_with_data(
            uniforms.as_bytes(),
            wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
        )
    }

    pub fn model_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.model_layout
    }

    pub fn begin_render_pass<'a>(
        &'a self,
        encoder: &'a mut wgpu::CommandEncoder,
        uniforms: Uniforms,
    ) -> wgpu::RenderPass<'a> {
        self.update_uniforms(encoder, uniforms);

        let color = Self::color_attachment(&self.color.view, Self::COLOR_CLEAR_COLOR);
        let normal = Self::color_attachment(&self.normal.view, Self::NORMAL_CLEAR_COLOR);
        let position = Self::color_attachment(&self.position.view, Self::POSITION_CLEAR_COLOR);

        let depth = Self::depth_attachment(&self.depth);

        let descriptor = wgpu::RenderPassDescriptor {
            color_attachments: &[color, normal, position],
            depth_stencil_attachment: Some(depth),
        };

        let mut render_pass = encoder.begin_render_pass(&descriptor);

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);

        render_pass
    }

    fn update_uniforms(&self, encoder: &mut wgpu::CommandEncoder, uniforms: Uniforms) {
        let staging = self
            .device
            .create_buffer_with_data(uniforms.as_bytes(), wgpu::BufferUsage::COPY_SRC);

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

    fn depth_attachment(
        attachment: &wgpu::TextureView,
    ) -> wgpu::RenderPassDepthStencilAttachmentDescriptor {
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

    pub fn color_buffer_view(&self) -> &wgpu::TextureView {
        &self.color.view
    }

    pub fn normal_buffer_view(&self) -> &wgpu::TextureView {
        &self.normal.view
    }

    pub fn position_buffer_view(&self) -> &wgpu::TextureView {
        &self.position.view
    }
}
