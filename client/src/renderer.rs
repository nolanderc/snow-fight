use anyhow::Result;

use cgmath::prelude::*;
use cgmath::{Matrix4, Point2, Point3, Vector3, Vector4};

use logic::components::Model;

use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;

use zerocopy::AsBytes;

use wgpu::VertexFormat::{Float2, Float3};
use wgpu_shader::VertexLayout;

use winit::window::Window;

mod gbuffer;
mod models;
mod texture;

use gbuffer::GBuffer;
use models::ModelRegistry;

/// `cgmath` uses OpenGL's coordinate system while WebGPU uses 
#[rustfmt::skip]
pub const OPENGL_TO_WGPU_MATRIX: Matrix4<f32> = Matrix4::new(
    1.0,  0.0,  0.0,  0.0,
    0.0, -1.0,  0.0,  0.0,
    0.0,  0.0,  0.5,  0.0,
    0.0,  0.0,  0.5,  1.0,
);

#[derive(Debug, Copy, Clone)]
pub struct RendererConfig {
    pub width: u32,
    pub height: u32,
    pub samples: u32,
}

pub struct Renderer {
    device: Arc<wgpu::Device>,
    queue: wgpu::Queue,
    surface: wgpu::Surface,
    swap_chain: wgpu::SwapChain,
    pipeline: wgpu::RenderPipeline,

    bind_group: wgpu::BindGroup,
    bind_group_layout: wgpu::BindGroupLayout,

    framebuffer: wgpu::TextureView,
    gbuffer: GBuffer,

    size: Size,
    samples: u32,

    uniforms: Uniforms,

    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,

    uniform_buffer: wgpu::Buffer,

    models: ModelRegistry,
    instances: HashMap<Model, Vec<Instance>>,

    black_texture: wgpu::TextureView,
}

struct Shaders {
    vertex: wgpu::ShaderModule,
    fragment: wgpu::ShaderModule,
}

pub struct Frame {
    camera: Camera,
    instances: HashMap<Model, Vec<Instance>>,
}

#[derive(Copy, Clone)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Copy, Clone, AsBytes)]
#[repr(C)]
struct Uniforms {
    transform: [[f32; 4]; 4],
    camera_pos: [f32; 3],
    _pad0: f32,
    light_pos: [f32; 3],
    camera_far: f32,
}

impl Default for Uniforms {
    fn default() -> Self {
        Uniforms {
            transform: Matrix4::identity().into(),
            camera_pos: [0.0; 3],
            _pad0: 0.0,
            light_pos: [0.0; 3],
            camera_far: Camera::CLIP_FAR,
        }
    }
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
struct Bindings<'a> {
    uniforms: &'a wgpu::Buffer,
    sampler: &'a wgpu::Sampler,
    color: &'a wgpu::TextureView,
    normal: &'a wgpu::TextureView,
    position: &'a wgpu::TextureView,
}

#[derive(Debug, Copy, Clone)]
pub struct Camera {
    pub position: Point3<f32>,
    pub focus: Point3<f32>,
    pub fov: f32,
}

#[derive(Debug, Copy, Clone, AsBytes, VertexLayout)]
#[repr(C)]
struct Vertex {
    #[vertex(format = Float3, location = 0)]
    position: [f32; 3],
    #[vertex(format = Float2, location = 1)]
    tex_coord: [f32; 2],
    #[vertex(format = Float3, location = 2)]
    normal: [f32; 3],
}

#[derive(Debug, Copy, Clone, AsBytes, VertexLayout)]
#[repr(C)]
pub struct Instance {
    #[vertex(format = Float3, location = 4)]
    position: [f32; 3],
    #[vertex(format = Float3, location = 5)]
    scale: [f32; 3],
    #[vertex(format = Float3, location = 6)]
    color: [f32; 3],
}

impl Renderer {
    const COLOR_OUTPUT_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

    pub async fn new(window: &Window, config: RendererConfig) -> Result<Renderer> {
        let surface = wgpu::Surface::create(window);

        let size = Size {
            width: config.width,
            height: config.height,
        };

        let adapter_options = wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::Default,
            compatible_surface: Some(&surface),
        };
        let adapter = wgpu::Adapter::request(&adapter_options, wgpu::BackendBit::all())
            .await
            .ok_or_else(|| anyhow!("failed to get wgpu Adapter"))?;

        let (device, queue) = adapter.request_device(&Default::default()).await;
        let device = Arc::new(device);

        let vertex_path = "src/shaders/fullscreen.vert.spv";
        let fragment_path = "src/shaders/composition.frag.spv";
        let shaders = Shaders::open(&device, vertex_path, fragment_path)?;

        // Create bind groups
        let bind_group_layout_desc = Self::bind_group_layout_desc();
        let bind_group_layout = device.create_bind_group_layout(&bind_group_layout_desc);

        // Create pipeline layout
        let layout_desc = wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &[&bind_group_layout],
        };
        let pipeline_layout = device.create_pipeline_layout(&layout_desc);

        // Create render pipeline
        let render_pipeline_desc = Self::render_pipeline_desc(&pipeline_layout, &shaders, config);
        let pipeline = device.create_render_pipeline(&render_pipeline_desc);

        // Setup swap chain
        let swap_chain_desc = Self::swap_chain_desc(config.width, config.height);
        let swap_chain = device.create_swap_chain(&surface, &swap_chain_desc);

        // Create multipsampled framebuffer
        let framebuffer_desc = Self::framebuffer_desc(config.width, config.height, config.samples);
        let framebuffer = device
            .create_texture(&framebuffer_desc)
            .create_default_view();

        let gbuffer = GBuffer::new(device.clone(), size);

        // Load models
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let models = ModelRegistry::load_all(&device, &mut encoder)?;

        // Create a vertex and index buffer
        let vertices = models.vertices();
        let indices = models.indices();

        let vertex_buffer =
            device.create_buffer_with_data(vertices.as_bytes(), wgpu::BufferUsage::VERTEX);
        let index_buffer =
            device.create_buffer_with_data(indices.as_bytes(), wgpu::BufferUsage::INDEX);

        // Setup shader uniforms
        let uniforms = Uniforms::default();
        let uniform_buffer = device.create_buffer_with_data(
            uniforms.as_bytes(),
            wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
        );

        let sampler = Self::create_sampler(&device);

        let mut black_image = image::RgbaImage::new(1, 1);
        black_image.put_pixel(0, 0, image::Rgba([0, 0, 0, 255]));
        let black_texture = texture::from_image(&black_image, &device, &mut encoder);

        let bindings = Bindings {
            uniforms: &uniform_buffer,
            sampler: &sampler,
            color: gbuffer.color_buffer_view(),
            normal: gbuffer.normal_buffer_view(),
            position: gbuffer.position_buffer_view(),
        };

        let bind_group = Self::create_bind_group(&device, &bind_group_layout, bindings);

        queue.submit(&[encoder.finish()]);

        // Finilize
        let renderer = Renderer {
            device,
            queue,
            surface,
            swap_chain,
            pipeline,

            bind_group,
            bind_group_layout,

            framebuffer,
            gbuffer,

            size: Size {
                width: config.width,
                height: config.height,
            },
            samples: config.samples,

            uniforms,

            vertex_buffer,
            index_buffer,

            models,
            instances: HashMap::new(),

            uniform_buffer,
            black_texture,
        };

        Ok(renderer)
    }

    fn render_pipeline_desc<'a>(
        layout: &'a wgpu::PipelineLayout,
        shaders: &'a Shaders,
        config: RendererConfig,
    ) -> wgpu::RenderPipelineDescriptor<'a> {
        wgpu::RenderPipelineDescriptor {
            layout,
            vertex_stage: shaders.vertex_stage(),
            fragment_stage: Some(shaders.fragment_stage()),
            rasterization_state: Some(wgpu::RasterizationStateDescriptor {
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: wgpu::CullMode::None,
                ..Default::default()
            }),
            primitive_topology: wgpu::PrimitiveTopology::TriangleList,
            color_states: &[wgpu::ColorStateDescriptor {
                format: Self::COLOR_OUTPUT_TEXTURE_FORMAT,
                color_blend: wgpu::BlendDescriptor::REPLACE,
                alpha_blend: wgpu::BlendDescriptor::REPLACE,
                write_mask: wgpu::ColorWrite::COLOR,
            }],
            depth_stencil_state: None,
            vertex_state: wgpu::VertexStateDescriptor {
                index_format: wgpu::IndexFormat::Uint32,
                vertex_buffers: &[],
            },
            sample_count: config.samples,
            sample_mask: !0,
            alpha_to_coverage_enabled: false,
        }
    }

    fn swap_chain_desc(width: u32, height: u32) -> wgpu::SwapChainDescriptor {
        wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            format: Self::COLOR_OUTPUT_TEXTURE_FORMAT,
            width,
            height,
            present_mode: wgpu::PresentMode::Mailbox,
        }
    }

    fn framebuffer_desc(
        width: u32,
        height: u32,
        sample_count: u32,
    ) -> wgpu::TextureDescriptor<'static> {
        wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width,
                height,
                depth: 1,
            },
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: Self::COLOR_OUTPUT_TEXTURE_FORMAT,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
        }
    }

    fn create_sampler(device: &wgpu::Device) -> wgpu::Sampler {
        let descriptor = wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_min_clamp: -100.0,
            lod_max_clamp: 100.0,
            compare: wgpu::CompareFunction::Always,
        };

        device.create_sampler(&descriptor)
    }

    fn bind_group_layout_desc() -> wgpu::BindGroupLayoutDescriptor<'static> {
        wgpu::BindGroupLayoutDescriptor {
            label: None,
            bindings: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::UniformBuffer { dynamic: true },
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::Sampler { comparison: false },
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::SampledTexture {
                        component_type: wgpu::TextureComponentType::Float,
                        multisampled: false,
                        dimension: wgpu::TextureViewDimension::D2,
                    },
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::SampledTexture {
                        component_type: wgpu::TextureComponentType::Float,
                        multisampled: false,
                        dimension: wgpu::TextureViewDimension::D2,
                    },
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::SampledTexture {
                        component_type: wgpu::TextureComponentType::Float,
                        multisampled: false,
                        dimension: wgpu::TextureViewDimension::D2,
                    },
                },
            ],
        }
    }

    fn create_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        bindings: Bindings,
    ) -> wgpu::BindGroup {
        let bind_group_desc = wgpu::BindGroupDescriptor {
            label: None,
            layout,
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
                    resource: wgpu::BindingResource::Sampler(bindings.sampler),
                },
                wgpu::Binding {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(bindings.color),
                },
                wgpu::Binding {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(bindings.normal),
                },
                wgpu::Binding {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(bindings.position),
                },
            ],
        };

        device.create_bind_group(&bind_group_desc)
    }

    pub fn set_size(&mut self, width: u32, height: u32) {
        self.size = Size { width, height };

        let swap_chain_desc = Self::swap_chain_desc(width, height);
        self.swap_chain = self
            .device
            .create_swap_chain(&self.surface, &swap_chain_desc);

        let framebuffer_desc = Self::framebuffer_desc(width, height, self.samples);
        self.framebuffer = self
            .device
            .create_texture(&framebuffer_desc)
            .create_default_view();

        self.gbuffer = GBuffer::new(self.device.clone(), self.size);

        let sampler = Self::create_sampler(&self.device);

        let bindings = Bindings {
            uniforms: &self.uniform_buffer,
            sampler: &sampler,
            color: self.gbuffer.color_buffer_view(),
            normal: self.gbuffer.normal_buffer_view(),
            position: self.gbuffer.position_buffer_view(),
        };

        self.bind_group = Self::create_bind_group(&self.device, &self.bind_group_layout, bindings);

        self.cleanup();
    }

    pub fn cleanup(&mut self) {
        self.device.poll(wgpu::Maintain::Wait);
    }

    pub fn next_frame(&mut self, camera: Camera) -> Frame {
        let mut instances = std::mem::take(&mut self.instances);
        for batch in instances.values_mut() {
            batch.clear();
        }
        Frame { instances, camera }
    }

    pub fn submit(&mut self, frame: Frame) {
        let Frame { instances, camera } = frame;

        self.instances = instances;
        self.uniforms.transform = camera.transform(self.size).into();
        self.uniforms.camera_pos = camera.position.into();
        self.uniforms.light_pos = camera.focus.into();

        self.render();
    }

    fn render(&mut self) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        self.update_buffers(&mut encoder);

        let frame = self.swap_chain.get_next_texture().unwrap();

        let color_attachment =
            Self::color_attachment_desc(&frame.view, &self.framebuffer, self.samples);

        let render_pass_desc = wgpu::RenderPassDescriptor {
            color_attachments: &[color_attachment],
            depth_stencil_attachment: None,
        };

        // G-buffer
        {
            let uniforms = gbuffer::Uniforms {
                transform: self.uniforms.transform,
            };

            let instances = self.prepare_instances();

            let mut render_pass = self.gbuffer.begin_render_pass(&mut encoder, uniforms);
            render_pass.set_vertex_buffer(0, &self.vertex_buffer, 0, 0);
            render_pass.set_index_buffer(&self.index_buffer, 0, 0);

            for (bind_group, instance_buffer, indices, count) in &instances {
                render_pass.set_bind_group(1, &bind_group, &[]);
                render_pass.set_vertex_buffer(1, &instance_buffer, 0, 0);
                render_pass.draw_indexed(indices.ccw.clone(), 0, 0..*count);
            }
        }

        // Final composit
        {
            let mut render_pass = encoder.begin_render_pass(&render_pass_desc);
            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &self.bind_group, &[0]);
            render_pass.draw(0..3, 0..1);
            render_pass.draw(1..4, 0..1);
        }

        let render_commands = encoder.finish();

        self.queue.submit(&[render_commands]);
    }

    fn prepare_instances(&self) -> Vec<(wgpu::BindGroup, wgpu::Buffer, models::IndexRange, u32)> {
        self.instances
            .iter()
            .filter(|(_, instances)| !instances.is_empty())
            .map(|(&model, instances)| {
                let data = self.models.get_model(model).unwrap();

                let sampler = Self::create_sampler(&self.device);
                let texture = data
                    .texture
                    .as_ref()
                    .map(|t| t.as_ref())
                    .unwrap_or(&self.black_texture);

                let bind_group_desc = wgpu::BindGroupDescriptor {
                    label: None,
                    layout: self.gbuffer.model_bind_group_layout(),
                    bindings: &[
                        wgpu::Binding {
                            binding: 0,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                        wgpu::Binding {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(texture),
                        },
                    ],
                };

                let bind_group = self.device.create_bind_group(&bind_group_desc);

                let instance_buffer = self
                    .device
                    .create_buffer_with_data(instances.as_bytes(), wgpu::BufferUsage::VERTEX);

                (
                    bind_group,
                    instance_buffer,
                    data.indices.clone(),
                    instances.len() as u32,
                )
            })
            .collect::<Vec<_>>()
    }

    fn color_attachment_desc<'a>(
        frame: &'a wgpu::TextureView,
        framebuffer: &'a wgpu::TextureView,
        samples: u32,
    ) -> wgpu::RenderPassColorAttachmentDescriptor<'a> {
        let (color_attachment, resolve_target) = if samples <= 1 {
            (frame, None)
        } else {
            (framebuffer, Some(frame))
        };

        wgpu::RenderPassColorAttachmentDescriptor {
            attachment: color_attachment,
            resolve_target,
            load_op: wgpu::LoadOp::Clear,
            store_op: wgpu::StoreOp::Store,
            clear_color: wgpu::Color {
                r: 0.2,
                g: 0.2,
                b: 0.2,
                a: 0.2,
            },
        }
    }

    fn update_buffers(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let scratch_uniform_buffer = self.device.create_buffer_with_data(
            self.uniforms.as_bytes(),
            wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_SRC,
        );

        encoder.copy_buffer_to_buffer(
            &scratch_uniform_buffer,
            0,
            &self.uniform_buffer,
            0,
            std::mem::size_of_val(&self.uniforms) as u64,
        );
    }
}

impl Shaders {
    pub fn open(
        device: &wgpu::Device,
        vertex: impl AsRef<Path>,
        fragment: impl AsRef<Path>,
    ) -> Result<Shaders> {
        let vertex_source = fs::read(vertex)?;
        let fragment_source = fs::read(fragment)?;
        Shaders::load(device, &vertex_source, &fragment_source)
    }

    pub fn load(device: &wgpu::Device, vertex: &[u8], fragment: &[u8]) -> Result<Shaders> {
        let vertex_spirv = wgpu::read_spirv(Cursor::new(vertex))?;
        let fragment_spirv = wgpu::read_spirv(Cursor::new(fragment))?;

        let shaders = Shaders {
            vertex: device.create_shader_module(&vertex_spirv),
            fragment: device.create_shader_module(&fragment_spirv),
        };

        Ok(shaders)
    }

    pub fn vertex_stage(&self) -> wgpu::ProgrammableStageDescriptor {
        wgpu::ProgrammableStageDescriptor {
            module: &self.vertex,
            entry_point: "main",
        }
    }

    pub fn fragment_stage(&self) -> wgpu::ProgrammableStageDescriptor {
        wgpu::ProgrammableStageDescriptor {
            module: &self.fragment,
            entry_point: "main",
        }
    }
}

impl Frame {
    pub fn draw(&mut self, model: Model, instance: Instance) {
        self.instances
            .entry(model)
            .or_insert_with(Default::default)
            .push(instance);
    }
}

impl Camera {
    const CLIP_NEAR: f32 = 0.1;
    const CLIP_FAR: f32 = 40.0;

    pub fn perspective(self, size: Size) -> Matrix4<f32> {
        let aspect = size.width as f32 / size.height as f32;
        let perspective = Matrix4::from(cgmath::PerspectiveFov {
            fovy: cgmath::Deg(self.fov).into(),
            aspect,
            near: Self::CLIP_NEAR,
            far: Self::CLIP_FAR,
        });

        OPENGL_TO_WGPU_MATRIX * perspective
    }

    pub fn view(self) -> Matrix4<f32> {
        Matrix4::look_at(self.position, self.focus, [0.0, 0.0, 1.0].into())
    }

    pub fn transform(self, size: Size) -> Matrix4<f32> {
        self.perspective(size) * self.view()
    }

    pub fn cast_ray(self, size: Size, screen: Point2<f32>) -> Vector3<f32> {
        let perspective = self
            .perspective(size)
            .invert()
            .unwrap_or_else(Matrix4::identity);
        let view = self.view().invert().unwrap_or_else(Matrix4::identity);

        let clip = Vector4 {
            x: screen.x,
            y: screen.y,
            z: 1.0,
            w: 1.0,
        };

        let mut eye = perspective * clip;
        eye.z = -1.0;
        eye.w = 0.0;

        let world = view * eye;

        let delta = world.xyz();
        delta.normalize()
    }
}

impl Instance {
    pub fn new(position: impl Into<[f32; 3]>) -> Self {
        Instance {
            position: position.into(),
            scale: [1.0; 3],
            color: [0.0; 3],
        }
    }

    pub fn with_scale(self, scale: impl Into<[f32; 3]>) -> Self {
        Instance {
            scale: scale.into(),
            ..self
        }
    }

    pub fn with_color(self, color: [f32; 3]) -> Self {
        Instance { color, ..self }
    }
}
