use super::{GBuffer, Shaders, Size};

use cgmath::{prelude::*, Matrix4, Vector3, Point3};
use rand::Rng;
use std::sync::Arc;

const SAMPLE_COUNT: usize = 1;

pub struct Ssao {
    device: Arc<wgpu::Device>,

    output: wgpu::TextureView,

    pipeline: wgpu::RenderPipeline,

    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    kernel_buffer: wgpu::Buffer,
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct Uniforms {
    pub transform: Matrix4<f32>,
    pub camera_pos: Point3<f32>,
}

impl Default for Uniforms {
    fn default() -> Self {
        Uniforms {
            transform: Matrix4::identity(),
            camera_pos: [0.0; 3].into(),
        }
    }
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct Kernel {
    samples: [[f32; 4]; SAMPLE_COUNT],
}

struct Bindings<'a> {
    uniforms: &'a wgpu::Buffer,
    kernel: &'a wgpu::Buffer,
    sampler: &'a wgpu::Sampler,
    position: &'a wgpu::TextureView,
    normal: &'a wgpu::TextureView,
}

impl Ssao {
    const VERTEX_PATH: &'static str = "src/shaders/fullscreen.vert.spv";
    const FRAGMENT_PATH: &'static str = "src/shaders/ssao.frag.spv";

    const OUTPUT_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;

    const CLEAR_COLOR: wgpu::Color = wgpu::Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };

    const BIND_GROUP_BINDINGS: &'static [wgpu::BindGroupLayoutBinding] = &[
        wgpu::BindGroupLayoutBinding {
            binding: 0,
            visibility: wgpu::ShaderStage::FRAGMENT,
            ty: wgpu::BindingType::UniformBuffer { dynamic: false },
        },
        wgpu::BindGroupLayoutBinding {
            binding: 1,
            visibility: wgpu::ShaderStage::FRAGMENT,
            ty: wgpu::BindingType::UniformBuffer { dynamic: false },
        },
        wgpu::BindGroupLayoutBinding {
            binding: 2,
            visibility: wgpu::ShaderStage::FRAGMENT,
            ty: wgpu::BindingType::Sampler,
        },
        wgpu::BindGroupLayoutBinding {
            binding: 3,
            visibility: wgpu::ShaderStage::FRAGMENT,
            ty: wgpu::BindingType::SampledTexture {
                multisampled: false,
                dimension: wgpu::TextureViewDimension::D2,
            },
        },
        wgpu::BindGroupLayoutBinding {
            binding: 4,
            visibility: wgpu::ShaderStage::FRAGMENT,
            ty: wgpu::BindingType::SampledTexture {
                multisampled: false,
                dimension: wgpu::TextureViewDimension::D2,
            },
        },
    ];

    const COLOR_STATES: &'static [wgpu::ColorStateDescriptor] = &[wgpu::ColorStateDescriptor {
        format: Self::OUTPUT_TEXTURE_FORMAT,
        color_blend: wgpu::BlendDescriptor::REPLACE,
        alpha_blend: wgpu::BlendDescriptor::REPLACE,
        write_mask: wgpu::ColorWrite::COLOR,
    }];

    pub(super) fn new(device: Arc<wgpu::Device>, size: Size, gbuffer: &GBuffer) -> Ssao {
        let output = Self::create_buffer_texture(&device, size, Self::OUTPUT_TEXTURE_FORMAT);

        let bind_group_layout = Self::create_bind_group_layout(&device);
        let pipeline = Self::create_render_pipeline(&device, &bind_group_layout);

        let uniform_buffer = Self::create_uniform_buffer(&device, Uniforms::default());
        let kernel_buffer = Self::create_uniform_buffer(&device, Kernel::new());

        let sampler = Self::create_sampler(&device);
        let bindings = Bindings {
            uniforms: &uniform_buffer,
            kernel: &kernel_buffer,
            sampler: &sampler,
            position: gbuffer.position_buffer(),
            normal: gbuffer.normal_buffer(),
        };
        let bind_group = Self::create_bind_group(&device, &bind_group_layout, bindings);

        Ssao {
            device,

            output,

            pipeline,

            bind_group,
            uniform_buffer,
            kernel_buffer,
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
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsage::WRITE_ALL | wgpu::TextureUsage::READ_ALL,
        };

        device.create_texture(&descriptor).create_default_view()
    }

    fn create_render_pipeline(
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::RenderPipeline {
        let layout = Self::create_pipeline_layout(device, bind_group_layout);

        let shaders = Shaders::open(&device, Self::VERTEX_PATH, Self::FRAGMENT_PATH).unwrap();

        let descriptor = wgpu::RenderPipelineDescriptor {
            layout: &layout,
            vertex_stage: shaders.vertex_stage(),
            fragment_stage: Some(shaders.fragment_stage()),
            rasterization_state: Some(Default::default()),
            primitive_topology: wgpu::PrimitiveTopology::TriangleList,
            color_states: Self::COLOR_STATES,
            depth_stencil_state: None,
            index_format: wgpu::IndexFormat::Uint32,
            vertex_buffers: &[],
            sample_count: 1,
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

    fn create_sampler(device: &wgpu::Device) -> wgpu::Sampler {
        let descriptor = wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            lod_min_clamp: -100.0,
            lod_max_clamp: 100.0,
            compare_function: wgpu::CompareFunction::Always,
        };

        device.create_sampler(&descriptor)
    }

    fn create_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        bindings: Bindings,
    ) -> wgpu::BindGroup {
        let descriptor = wgpu::BindGroupDescriptor {
            bindings: &[
                wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: bindings.uniforms,
                        range: 0..std::mem::size_of::<Uniforms>() as u64,
                    },
                },
                wgpu::Binding {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: bindings.kernel,
                        range: 0..std::mem::size_of::<Kernel>() as u64,
                    },
                },
                wgpu::Binding {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(bindings.sampler),
                },
                wgpu::Binding {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(bindings.position),
                },
                wgpu::Binding {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(bindings.normal),
                },
            ],
            layout,
        };

        device.create_bind_group(&descriptor)
    }

    fn create_uniform_buffer<T: 'static + Copy + Sized>(
        device: &wgpu::Device,
        uniforms: T,
    ) -> wgpu::Buffer {
        device
            .create_buffer_mapped(1, wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST)
            .fill_from_slice(&[uniforms])
    }

    pub fn begin_render_pass<'a>(
        &self,
        encoder: &'a mut wgpu::CommandEncoder,
        uniforms: Uniforms,
    ) -> wgpu::RenderPass<'a> {
        Self::update_uniform_buffer(&self.device, encoder, &self.uniform_buffer, uniforms);
        Self::update_uniform_buffer(&self.device, encoder, &self.kernel_buffer, dbg!(Kernel::new()));

        let output = Self::color_attachment(&self.output, Self::CLEAR_COLOR);

        let descriptor = wgpu::RenderPassDescriptor {
            color_attachments: &[output],
            depth_stencil_attachment: None,
        };

        let mut render_pass = encoder.begin_render_pass(&descriptor);

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);

        render_pass
    }

    fn update_uniform_buffer<T: 'static + Copy + Sized>(
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        buffer: &wgpu::Buffer,
        uniforms: T,
    ) {
        let staging = device
            .create_buffer_mapped(1, wgpu::BufferUsage::COPY_SRC)
            .fill_from_slice(&[uniforms]);

        encoder.copy_buffer_to_buffer(
            &staging,
            0,
            buffer,
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

    pub fn output(&self) -> &wgpu::TextureView {
        &self.output
    }
}

impl Kernel {
    pub fn new() -> Kernel {
        let mut samples = [[0.0; 4]; SAMPLE_COUNT];

        let mut rng = rand::thread_rng();

        for (i, sample) in samples.iter_mut().enumerate() {
            let direction = Vector3::new(
                rng.gen_range(-1.0, 1.0),
                rng.gen_range(-1.0, 1.0),
                rng.gen_range(0.0, 1.0),
            );

            let distance = rng.gen_range(0.0, 1.0);
            let scale = i as f32 / SAMPLE_COUNT as f32;
            let distribution = 0.1 + 0.9 * scale * scale;

            let direction = distribution * distance * direction.normalize();

            sample[0] = direction.x;
            sample[1] = direction.y;
            sample[2] = direction.z;
        }

        Kernel { samples }
    }
}
