use anyhow::Result;
use cgmath::prelude::*;
use cgmath::{Matrix4, Point3, Vector3};
use logic::components::Model;
use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;
use wgpu::VertexFormat::Float3;
use wgpu_shader::VertexLayout;
use winit::window::Window;

mod gbuffer;
mod models;

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

const CLIP_NEAR: f32 = 0.1;
const CLIP_FAR: f32 = 40.0;

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
}

struct Shaders {
    vertex: wgpu::ShaderModule,
    fragment: wgpu::ShaderModule,
}

pub struct Frame<'a> {
    renderer: &'a mut Renderer,
}

#[derive(Copy, Clone)]
struct Size {
    width: u32,
    height: u32,
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
struct Uniforms {
    transform: Matrix4<f32>,
    camera_pos: Point3<f32>,
    _pad0: f32,
    light_pos: Point3<f32>,
    camera_far: f32,
}

impl Default for Uniforms {
    fn default() -> Self {
        Uniforms {
            transform: Matrix4::identity(),
            camera_pos: [0.0; 3].into(),
            _pad0: 0.0,
            light_pos: [0.0; 3].into(),
            camera_far: CLIP_FAR,
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

#[derive(Debug, Copy, Clone, VertexLayout)]
#[repr(C)]
struct Vertex {
    #[vertex(format = Float3, location = 0)]
    position: Vector3<f32>,
    #[vertex(format = Float3, location = 1)]
    color: [f32; 3],
    #[vertex(format = Float3, location = 2)]
    normal: Vector3<f32>,
}

#[derive(Debug, Copy, Clone, VertexLayout)]
#[repr(C)]
pub struct Instance {
    #[vertex(format = Float3, location = 3)]
    pub position: Point3<f32>,
    #[vertex(format = Float3, location = 4)]
    pub scale: Vector3<f32>,
    #[vertex(format = Float3, location = 5)]
    pub color: [f32; 3],
}

impl Renderer {
    const COLOR_OUTPUT_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

    pub fn new(window: &Window, config: RendererConfig) -> Result<Renderer> {
        let surface = wgpu::Surface::create(window);

        dbg!(std::mem::size_of::<Uniforms>());

        let size = Size {
            width: config.width,
            height: config.height,
        };

        let adapter = wgpu::Adapter::request(&Default::default())
            .ok_or_else(|| anyhow!("failed to get wgpu Adapter"))?;

        let (device, queue) = adapter.request_device(&Default::default());
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
        let models = ModelRegistry::load()?;

        // Create a vertex and index buffer
        let vertices = models.vertices();
        let indices = models.indices();

        let vertex_buffer = device
            .create_buffer_mapped::<Vertex>(vertices.len(), wgpu::BufferUsage::VERTEX)
            .fill_from_slice(vertices);
        let index_buffer = device
            .create_buffer_mapped::<u32>(indices.len(), wgpu::BufferUsage::INDEX)
            .fill_from_slice(indices);

        // Setup shader uniforms
        let uniforms = Uniforms::default();
        let uniform_buffer = device
            .create_buffer_mapped(1, wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST)
            .fill_from_slice(&[uniforms]);

        let sampler = Self::create_sampler(&device);

        let bindings = Bindings {
            uniforms: &uniform_buffer,
            sampler: &sampler,
            color: gbuffer.color_buffer(),
            normal: gbuffer.normal_buffer(),
            position: gbuffer.position_buffer(),
        };

        let bind_group = Self::create_bind_group(&device, &bind_group_layout, bindings);

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
            index_format: wgpu::IndexFormat::Uint32,
            vertex_buffers: &[],
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
            present_mode: wgpu::PresentMode::NoVsync,
        }
    }

    fn framebuffer_desc(width: u32, height: u32, sample_count: u32) -> wgpu::TextureDescriptor {
        wgpu::TextureDescriptor {
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
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            lod_min_clamp: -100.0,
            lod_max_clamp: 100.0,
            compare_function: wgpu::CompareFunction::Always,
        };

        device.create_sampler(&descriptor)
    }

    fn bind_group_layout_desc() -> wgpu::BindGroupLayoutDescriptor<'static> {
        wgpu::BindGroupLayoutDescriptor {
            bindings: &[
                wgpu::BindGroupLayoutBinding {
                    binding: 0,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::UniformBuffer { dynamic: true },
                },
                wgpu::BindGroupLayoutBinding {
                    binding: 1,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::Sampler,
                },
                wgpu::BindGroupLayoutBinding {
                    binding: 2,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::SampledTexture {
                        multisampled: false,
                        dimension: wgpu::TextureViewDimension::D2,
                    },
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
            ],
        }
    }

    fn create_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        bindings: Bindings,
    ) -> wgpu::BindGroup {
        let bind_group_desc = wgpu::BindGroupDescriptor {
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
        self.size = Size { width, height};

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
            color: self.gbuffer.color_buffer(),
            normal: self.gbuffer.normal_buffer(),
            position: self.gbuffer.position_buffer(),
        };

        self.bind_group = Self::create_bind_group(&self.device, &self.bind_group_layout, bindings);

        self.cleanup();
    }

    pub fn cleanup(&mut self) {
        self.device.poll(false);
    }

    pub fn next_frame(&mut self) -> Frame {
        self.instances.clear();
        Frame { renderer: self }
    }

    fn render(&mut self) {
        let mut encoder = self.device.create_command_encoder(&Default::default());

        self.update_buffers(&mut encoder);

        let frame = self.swap_chain.get_next_texture();

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

            let mut render_pass = self.gbuffer.begin_render_pass(&mut encoder, uniforms);
            render_pass.set_vertex_buffers(0, &[(&self.vertex_buffer, 0)]);
            render_pass.set_index_buffer(&self.index_buffer, 0);

            for (&model, instances) in self.instances.iter() {
                let instance_buffer = self
                    .device
                    .create_buffer_mapped(instances.len(), wgpu::BufferUsage::VERTEX)
                    .fill_from_slice(&instances);
                let data = &self.models.get_model(model).unwrap();
                render_pass.set_vertex_buffers(1, &[(&instance_buffer, 0)]);
                render_pass.draw_indexed(data.indices.clone(), 0, 0..instances.len() as u32);
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
        let scratch_uniform_buffer = self
            .device
            .create_buffer_mapped(1, wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_SRC)
            .fill_from_slice(&[self.uniforms]);

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

impl<'a> Frame<'a> {
    pub fn set_camera(&mut self, camera: Camera) {
        let size = self.renderer.size;
        let aspect = size.width as f32 / size.height as f32;
        let perspective = cgmath::Matrix4::from(cgmath::PerspectiveFov {
            fovy: cgmath::Deg(camera.fov).into(),
            aspect,
            near: CLIP_NEAR,
            far: CLIP_FAR,
        });

        let up = [0.0, 0.0, 1.0].into();
        let view = cgmath::Matrix4::look_at(camera.position, camera.focus, up);

        let transform = OPENGL_TO_WGPU_MATRIX * perspective * view;

        self.renderer.uniforms.transform = transform;
        self.renderer.uniforms.camera_pos = camera.position;
        self.renderer.uniforms.light_pos = camera.focus;
    }

    pub fn draw(&mut self, model: Model, instance: Instance) {
        self.renderer
            .instances
            .entry(model)
            .or_insert_with(Default::default)
            .push(instance);
    }

    pub fn submit(self) {
        drop(self);
    }
}

impl Drop for Frame<'_> {
    fn drop(&mut self) {
        self.renderer.render();
    }
}
