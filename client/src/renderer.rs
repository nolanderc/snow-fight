use anyhow::{Context, Result};
use cgmath::prelude::*;
use cgmath::{Matrix4, Point3, Vector3};
use logic::components::Model;
use std::collections::HashMap;
use std::f32::consts::PI;
use std::fs;
use std::io::Cursor;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;
use wgpu::VertexFormat::Float3;
use wgpu_shader::VertexLayout;
use winit::window::Window;

mod gbuffer;

use gbuffer::GBuffer;

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

    uniforms: Uniforms,

    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,

    uniform_buffer: wgpu::Buffer,

    models: HashMap<Model, ModelData>,
    instances: HashMap<Model, Vec<Instance>>,
}

struct ModelRegistry {
    models: HashMap<Model, ModelData>,
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}

struct ModelData {
    indices: Range<u32>,
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
    samples: u32,
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
struct Uniforms {
    transform: Matrix4<f32>,
    camera_pos: Point3<f32>,
    _pad0: f32,
    light_pos: Point3<f32>,
}

impl Default for Uniforms {
    fn default() -> Self {
        Uniforms {
            transform: Matrix4::identity(),
            camera_pos: [0.0; 3].into(),
            _pad0: 0.0,
            light_pos: [0.0; 3].into(),
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
            samples: config.samples,
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
        let ModelRegistry {
            models,
            vertices,
            indices,
        } = ModelRegistry::load()?;

        // Create a vertex and index buffer
        let vertex_buffer = device
            .create_buffer_mapped::<Vertex>(vertices.len(), wgpu::BufferUsage::VERTEX)
            .fill_from_slice(&vertices);
        let index_buffer = device
            .create_buffer_mapped::<u32>(indices.len(), wgpu::BufferUsage::INDEX)
            .fill_from_slice(&indices);

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
                samples: config.samples,
            },

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
        self.size = Size {
            width,
            height,
            ..self.size
        };

        let swap_chain_desc = Self::swap_chain_desc(width, height);
        self.swap_chain = self
            .device
            .create_swap_chain(&self.surface, &swap_chain_desc);

        let framebuffer_desc = Self::framebuffer_desc(width, height, self.size.samples);
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
            Self::color_attachment_desc(&frame.view, &self.framebuffer, self.size.samples);

        let render_pass_desc = wgpu::RenderPassDescriptor {
            color_attachments: &[color_attachment],
            depth_stencil_attachment: None,
        };

        {
            let uniforms = gbuffer::Uniforms {
                transform: self.uniforms.transform,
            };

            let mut render_pass = self.gbuffer.begin_render_pass(&mut encoder, uniforms);
            render_pass.set_vertex_buffers(0, &[(&self.vertex_buffer, 0)]);
            render_pass.set_index_buffer(&self.index_buffer, 0);

            for (model, instances) in self.instances.iter() {
                let instance_buffer = self
                    .device
                    .create_buffer_mapped(instances.len(), wgpu::BufferUsage::VERTEX)
                    .fill_from_slice(&instances);
                let data = &self.models[model];
                render_pass.set_vertex_buffers(1, &[(&instance_buffer, 0)]);
                render_pass.draw_indexed(data.indices.clone(), 0, 0..instances.len() as u32);
            }
        }

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
            near: 0.1,
            far: 20.0,
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

impl ModelRegistry {
    fn new() -> ModelRegistry {
        ModelRegistry {
            models: HashMap::new(),
            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }

    pub fn load() -> Result<ModelRegistry> {
        let mut registry = ModelRegistry::new();

        for &kind in Model::KINDS {
            let data = match kind {
                Model::Rect => registry.push_rect(),
                Model::Circle => registry.push_circle(32),
                Model::Tree => registry
                    .push_image("assets/tree_poplar.png")
                    .context("failed to build model for image")?,
            };

            registry.models.insert(kind, data);
        }

        Ok(registry)
    }

    fn add_vertices(&mut self, vertices: &[Vertex], indices: &[u32]) -> ModelData {
        let start_vertex = self.vertices.len() as u32;
        self.vertices.extend_from_slice(vertices);

        let start_index = self.indices.len() as u32;
        self.indices
            .extend(indices.iter().map(|index| start_vertex + index));
        let end_index = self.indices.len() as u32;

        ModelData {
            indices: start_index..end_index,
        }
    }

    fn push_rect(&mut self) -> ModelData {
        let vertex = |x, y| Vertex {
            position: [x, y, 0.0].into(),
            color: [1.0; 3],
            normal: [0.0, 0.0, 1.0].into(),
        };

        let corners = [
            vertex(0.0, 0.0),
            vertex(1.0, 0.0),
            vertex(1.0, 1.0),
            vertex(0.0, 1.0),
        ];

        let indices = [0, 1, 2, 2, 3, 0];

        self.add_vertices(&corners, &indices)
    }

    fn push_circle(&mut self, resolution: u32) -> ModelData {
        let vertex = |x, y| Vertex {
            position: [x, y, 0.0].into(),
            color: [1.0; 3],
            normal: [0.0, 0.0, 1.0].into(),
        };

        let mut vertices = Vec::with_capacity(resolution as usize);
        let mut indices = Vec::with_capacity(3 * (resolution as usize - 1));

        let theta = 2.0 * PI / resolution as f32;
        let (sin, cos) = theta.sin_cos();

        let mut dx = 0.5;
        let mut dy = 0.0;

        for _ in 0..resolution {
            vertices.push(vertex(0.5 + dx, 0.5 + dy));

            // rotate
            let next_x = dx * cos - dy * sin;
            let next_y = dx * sin + dy * cos;

            dx = next_x;
            dy = next_y;
        }

        for i in 0..resolution.saturating_sub(1) {
            indices.push(0);
            indices.push(i + 1);
            indices.push((i + 2) % resolution);
        }

        self.add_vertices(&vertices, &indices)
    }

    fn push_image(&mut self, path: impl AsRef<Path>) -> Result<ModelData> {
        let image = image::open(&path)
            .with_context(|| format!("failed to open image '{}'", path.as_ref().display()))?
            .into_rgba();

        let (width, height) = image.dimensions();

        let voxel_size = 1.0 / 16.0;

        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        for (col, row, color) in image.enumerate_pixels() {
            let [red, green, blue, alpha] = color.0;

            let normalize = |byte| byte as f32 / 255.0;
            let color = [normalize(red), normalize(green), normalize(blue)];

            let x = col as f32 * voxel_size;
            let z = (height - row - 1) as f32 * voxel_size;

            if alpha != 0 {
                let voxel = Voxel::new([x, 0.5 - voxel_size / 2.0, z], color, voxel_size);

                let mut add_face = |face: &VoxelFace| {
                    let start_vertex = vertices.len() as u32;
                    vertices.extend_from_slice(&face.vertices);

                    let offset_indices = VoxelFace::INDICES.iter().map(|i| *i + start_vertex);
                    indices.extend(offset_indices);
                };

                add_face(&voxel.faces.front);
                add_face(&voxel.faces.back);

                let is_transparent = |col: i32, row: i32| {
                    if col < 0 || col >= width as i32 || row < 0 || row >= height as i32 {
                        true
                    } else {
                        let [_, _, _, alpha] = image.get_pixel(col as u32, row as u32).0;
                        alpha != 255
                    }
                };

                let (col, row) = (col as i32, row as i32);

                if is_transparent(col, row - 1) {
                    add_face(&voxel.faces.top);
                }
                if is_transparent(col, row + 1) {
                    add_face(&voxel.faces.bottom);
                }
                if is_transparent(col - 1, row) {
                    add_face(&voxel.faces.left);
                }
                if is_transparent(col + 1, row) {
                    add_face(&voxel.faces.right);
                }
            }
        }

        let n = indices.len();
        eprintln!("{} indices ({} triangles or {} faces)", n, n / 3, n / 6);

        Ok(self.add_vertices(&vertices, &indices))
    }
}

struct Voxel {
    faces: Faces<VoxelFace>,
}

struct VoxelFace {
    vertices: [Vertex; 4],
}

struct Faces<T> {
    front: T,
    back: T,
    top: T,
    bottom: T,
    left: T,
    right: T,
}

impl Voxel {
    pub fn new([x, y, z]: [f32; 3], color: [f32; 3], size: f32) -> Voxel {
        let vertex = |position: [f32; 3], normal: [f32; 3]| Vertex {
            position: position.into(),
            color,
            normal: normal.into(),
        };

        let (x0, y0, z0) = (x, y, z);
        let (x1, y1, z1) = (x + size, y + size, z + size);

        let faces = Faces {
            front: VoxelFace {
                vertices: [
                    vertex([x0, y0, z0], [0.0, -1.0, 0.0]),
                    vertex([x1, y0, z0], [0.0, -1.0, 0.0]),
                    vertex([x1, y0, z1], [0.0, -1.0, 0.0]),
                    vertex([x0, y0, z1], [0.0, -1.0, 0.0]),
                ],
            },
            back: VoxelFace {
                vertices: [
                    vertex([x1, y1, z0], [0.0, 1.0, 0.0]),
                    vertex([x0, y1, z0], [0.0, 1.0, 0.0]),
                    vertex([x0, y1, z1], [0.0, 1.0, 0.0]),
                    vertex([x1, y1, z1], [0.0, 1.0, 0.0]),
                ],
            },
            top: VoxelFace {
                vertices: [
                    vertex([x0, y0, z1], [0.0, 0.0, 1.0]),
                    vertex([x1, y0, z1], [0.0, 0.0, 1.0]),
                    vertex([x1, y1, z1], [0.0, 0.0, 1.0]),
                    vertex([x0, y1, z1], [0.0, 0.0, 1.0]),
                ],
            },
            bottom: VoxelFace {
                vertices: [
                    vertex([x1, y0, z0], [0.0, 0.0, 1.0]),
                    vertex([x0, y0, z0], [0.0, 0.0, 1.0]),
                    vertex([x0, y1, z0], [0.0, 0.0, 1.0]),
                    vertex([x1, y1, z0], [0.0, 0.0, 1.0]),
                ],
            },
            left: VoxelFace {
                vertices: [
                    vertex([x0, y1, z0], [-1.0, 0.0, 0.0]),
                    vertex([x0, y0, z0], [-1.0, 0.0, 0.0]),
                    vertex([x0, y0, z1], [-1.0, 0.0, 0.0]),
                    vertex([x0, y1, z1], [-1.0, 0.0, 0.0]),
                ],
            },
            right: VoxelFace {
                vertices: [
                    vertex([x1, y0, z0], [1.0, 0.0, 0.0]),
                    vertex([x1, y1, z0], [1.0, 0.0, 0.0]),
                    vertex([x1, y1, z1], [1.0, 0.0, 0.0]),
                    vertex([x1, y0, z1], [1.0, 0.0, 0.0]),
                ],
            },
        };

        Voxel { faces }
    }
}

impl VoxelFace {
    const INDICES: [u32; 6] = [0, 1, 2, 2, 3, 0];
}
