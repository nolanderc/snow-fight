use anyhow::Result;
use cgmath::SquareMatrix;
use std::io::Cursor;
use winit::window::Window;
use zerocopy::AsBytes;

/// `cgmath` uses OpenGL's coordinate system while WebGPU uses 
#[rustfmt::skip]
pub const OPENGL_TO_WGPU_MATRIX: cgmath::Matrix4<f32> = cgmath::Matrix4::new(
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
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface,
    swap_chain: wgpu::SwapChain,
    framebuffer: wgpu::TextureView,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,

    size: Size,
    samples: u32,

    vertices: Vec<Vertex>,
    indices: Vec<u32>,

    uniforms: Uniforms,

    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,

    uniform_buffer: wgpu::Buffer,
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

#[derive(Debug, Copy, Clone, AsBytes)]
#[repr(C)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 3],
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
struct Uniforms {
    transform: cgmath::Matrix4<f32>,
}

#[derive(Debug, Copy, Clone)]
pub struct Camera {
    pub position: [f32; 3],
    pub focus: [f32; 3],
    pub fov: f32,
}

impl Renderer {
    const COLOR_OUTPUT_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

    pub fn new(window: &Window, config: RendererConfig) -> Result<Renderer> {
        let surface = wgpu::Surface::create(window);

        let adapter = wgpu::Adapter::request(&Default::default())
            .ok_or_else(|| anyhow!("failed to get wgpu Adapter"))?;

        let (device, queue) = adapter.request_device(&Default::default());

        let shaders = Shaders::load(&device)?;

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

        // Create a vertex and index buffer
        let vertices = vec![
            Vertex {
                position: [-0.75, 0.5],
                color: [1.0, 0.0, 0.0],
            },
            Vertex {
                position: [-0.5, -0.5],
                color: [1.0, 1.0, 0.0],
            },
            Vertex {
                position: [0.5, -0.5],
                color: [1.0, 1.0, 1.0],
            },
            Vertex {
                position: [0.75, 0.5],
                color: [1.0, 0.0, 1.0],
            },
        ];

        let indices = vec![0, 1, 2, 2, 3, 0];

        let vertex_buffer = device
            .create_buffer_mapped(vertices.len(), wgpu::BufferUsage::VERTEX)
            .fill_from_slice(&vertices);

        let index_buffer = device
            .create_buffer_mapped(indices.len(), wgpu::BufferUsage::INDEX)
            .fill_from_slice(&indices);

        let uniforms = Uniforms {
            transform: cgmath::Matrix4::identity(),
        };
        let uniform_buffer = device
            .create_buffer_mapped(1, wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST)
            .fill_from_slice(&[uniforms]);

        let bind_group = Self::create_bind_group(&device, &bind_group_layout, &uniform_buffer);

        // Finilize
        let renderer = Renderer {
            device,
            queue,
            surface,
            swap_chain,
            framebuffer,
            pipeline,
            bind_group,

            size: Size {
                width: config.width,
                height: config.height,
            },
            samples: config.samples,

            vertices: Vec::new(),
            indices: Vec::new(),
            uniforms,

            vertex_buffer,
            index_buffer,

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
            vertex_stage: wgpu::ProgrammableStageDescriptor {
                module: &shaders.vertex,
                entry_point: "main",
            },
            fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
                module: &shaders.fragment,
                entry_point: "main",
            }),
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
                write_mask: wgpu::ColorWrite::ALL,
            }],
            depth_stencil_state: None,
            index_format: wgpu::IndexFormat::Uint32,
            vertex_buffers: &[wgpu::VertexBufferDescriptor {
                stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::InputStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttributeDescriptor {
                        offset: 0,
                        format: wgpu::VertexFormat::Float2,
                        shader_location: 0,
                    },
                    wgpu::VertexAttributeDescriptor {
                        offset: 8,
                        format: wgpu::VertexFormat::Float3,
                        shader_location: 1,
                    },
                ],
            }],
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
            present_mode: wgpu::PresentMode::Vsync,
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

    fn bind_group_layout_desc() -> wgpu::BindGroupLayoutDescriptor<'static> {
        wgpu::BindGroupLayoutDescriptor {
            bindings: &[wgpu::BindGroupLayoutBinding {
                binding: 0,
                visibility: wgpu::ShaderStage::VERTEX,
                ty: wgpu::BindingType::UniformBuffer { dynamic: true },
            }],
        }
    }

    fn create_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        uniform_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        let bind_group_desc = wgpu::BindGroupDescriptor {
            layout,
            bindings: &[wgpu::Binding {
                binding: 0,
                resource: wgpu::BindingResource::Buffer {
                    buffer: uniform_buffer,
                    range: 0..std::mem::size_of::<Uniforms>() as u64,
                },
            }],
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
    }

    pub fn cleanup(&mut self) {
        self.device.poll(false);
    }

    pub fn next_frame(&mut self) -> Frame {
        self.vertices.clear();
        self.indices.clear();
        Frame { renderer: self }
    }

    fn render(&mut self) {
        let mut encoder = self.device.create_command_encoder(&Default::default());

        self.update_buffers(&mut encoder);

        let frame = self.swap_chain.get_next_texture();

        let (color_attachment, resolve_target) = if self.samples == 1 {
            (&frame.view, None)
        } else {
            (&self.framebuffer, Some(&frame.view))
        };

        let render_pass_desc = wgpu::RenderPassDescriptor {
            color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
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
            }],
            depth_stencil_attachment: None,
        };

        {
            let mut render_pass = encoder.begin_render_pass(&render_pass_desc);
            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &self.bind_group, &[0]);
            render_pass.set_vertex_buffers(0, &[(&self.vertex_buffer, 0)]);
            render_pass.set_index_buffer(&self.index_buffer, 0);

            let index_count = self.indices.len() as u32;
            render_pass.draw_indexed(0..index_count, 0, 0..1);
        }

        let render_commands = encoder.finish();

        self.queue.submit(&[render_commands]);
    }

    fn update_buffers(&mut self, encoder: &mut wgpu::CommandEncoder) {
        self.vertex_buffer = self
            .device
            .create_buffer_mapped(self.vertices.len(), wgpu::BufferUsage::VERTEX)
            .fill_from_slice(&self.vertices);

        self.index_buffer = self
            .device
            .create_buffer_mapped(self.indices.len(), wgpu::BufferUsage::INDEX)
            .fill_from_slice(&self.indices);

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
    pub fn load(device: &wgpu::Device) -> Result<Shaders> {
        let vertex_source = include_bytes!("shaders/shader.vert.spv");
        let vertex_spirv = wgpu::read_spirv(Cursor::new(&vertex_source[..]))?;

        let fragment_source = include_bytes!("shaders/shader.frag.spv");
        let fragment_spirv = wgpu::read_spirv(Cursor::new(&fragment_source[..]))?;

        let shaders = Shaders {
            vertex: device.create_shader_module(&vertex_spirv),
            fragment: device.create_shader_module(&fragment_spirv),
        };

        Ok(shaders)
    }
}

pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl<'a> Frame<'a> {
    fn add_vertices(&mut self, vertices: &[Vertex], indices: &[u32]) {
        let start_index = self.renderer.vertices.len() as u32;
        self.renderer.vertices.extend_from_slice(vertices);
        self.renderer
            .indices
            .extend(indices.iter().map(|index| start_index + index));
    }

    pub fn set_camera(&mut self, camera: Camera) {
        let size = self.renderer.size;
        let aspect = size.width as f32 / size.height as f32;
        let perspective = cgmath::Matrix4::from(cgmath::PerspectiveFov {
            fovy: cgmath::Deg(camera.fov).into(),
            aspect,
            near: 0.01,
            far: 100.0,
        });

        let up = [0.0, 0.0, 1.0];
        let view = cgmath::Matrix4::look_at(camera.position.into(), camera.focus.into(), up.into());

        self.renderer.uniforms.transform = OPENGL_TO_WGPU_MATRIX * perspective * view;
    }

    pub fn draw_rect(&mut self, rect: Rect, color: [f32; 3]) {
        let vertex = |position| Vertex { position, color };

        let Rect { x, y, w, h } = rect;

        let (l, r) = (x, x + w);
        let (b, t) = (y, y + h);

        let corners = [
            vertex([l, b]),
            vertex([l, t]),
            vertex([r, t]),
            vertex([r, b]),
        ];

        let indices = [0, 1, 2, 2, 3, 0];

        self.add_vertices(&corners, &indices)
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
