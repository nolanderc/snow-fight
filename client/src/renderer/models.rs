use anyhow::{Context, Result};
use cgmath::{prelude::*, Point3, Vector2, Vector3};
use logic::components::Model;
use std::collections::HashMap;
use std::f32::consts::PI;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use super::Vertex;

const VOXEL_SIZE: f32 = 1.0 / 16.0;

pub(super) struct ModelRegistry {
    models: HashMap<Model, ModelData>,
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}

pub struct ModelData {
    pub(super) indices: IndexRange,
    pub(super) texture: Option<Arc<wgpu::TextureView>>,
}

#[derive(Debug, Clone)]
pub struct IndexRange {
    pub ccw: Range<u32>,
    pub cw: Range<u32>,
}

impl ModelRegistry {
    fn new() -> ModelRegistry {
        ModelRegistry {
            models: HashMap::new(),
            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }

    pub fn load_all(
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<ModelRegistry> {
        let mut registry = ModelRegistry::new();

        for &kind in Model::KINDS {
            registry.load(kind, device, encoder)?;
        }

        Ok(registry)
    }

    pub fn load(
        &mut self,
        kind: Model,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<()> {
        let data = match kind {
            Model::Rect => self.push_rect(),
            Model::Circle => self.push_circle(32),
            Model::Tree => self
                .push_image("assets/tree_poplar.png", device, encoder)
                .context("failed to build model for image")?,
            Model::Player => self
                .push_image("assets/player.png", device, encoder)
                .context("failed to build model for image")?,
            Model::Mushroom => self
                .push_image("assets/mushroom.png", device, encoder)
                .context("failed to build model for image")?,
            Model::Cube => self.push_cube(),
        };

        self.models.insert(kind, data);

        Ok(())
    }

    pub fn vertices(&self) -> &[Vertex] {
        &self.vertices
    }

    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    pub fn get_model(&self, model: Model) -> Option<&ModelData> {
        self.models.get(&model)
    }

    fn add_vertices(&mut self, vertices: &[Vertex], indices: &[u32]) -> IndexRange {
        let ccw = self.add_indices(indices.iter().copied());
        let cw = self.add_indices(indices.iter().rev().copied());
        self.vertices.extend_from_slice(vertices);
        IndexRange { ccw, cw }
    }

    fn add_indices(&mut self, indices: impl Iterator<Item = u32>) -> Range<u32> {
        let start_vertex = self.vertices.len() as u32;
        let start_index = self.indices.len() as u32;
        self.indices
            .extend(indices.map(|index| start_vertex + index));
        let end_index = self.indices.len() as u32;

        start_index..end_index
    }

    fn push_rect(&mut self) -> ModelData {
        let vertex = |x, y| Vertex {
            position: [x, y, 0.0],
            tex_coord: [x + 0.5, y + 0.5],
            normal: [0.0, 0.0, 1.0],
        };

        let corners = [
            vertex(-0.5, -0.5),
            vertex(0.5, -0.5),
            vertex(0.5, 0.5),
            vertex(-0.5, 0.5),
        ];

        let indices = [0, 1, 2, 2, 3, 0];
        let range = self.add_vertices(&corners, &indices);

        ModelData {
            indices: range,
            texture: None,
        }
    }

    fn push_cube(&mut self) -> ModelData {
        let normals = [
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(-1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            Vector3::new(0.0, -1.0, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(0.0, 0.0, -1.0),
        ];

        let mut vertices = Vec::with_capacity(4 * 6);
        let mut indices = Vec::with_capacity(6 * 6);

        for &normal in &normals {
            let quad = Quad {
                size: [1.0; 2].into(),
                normal,
                center: Point3::from_vec(0.5 * normal),
                tex_start: [0.0, 0.0],
                tex_end: [1.0, 1.0],
            };

            let face = CubeFace::from(quad);

            let start_vertex = vertices.len() as u32;
            vertices.extend_from_slice(&face.vertices);
            let offset_indices = CubeFace::INDICES.iter().map(|i| *i + start_vertex);
            indices.extend(offset_indices);
        }

        let range = self.add_vertices(&vertices, &indices);

        ModelData {
            indices: range,
            texture: None,
        }
    }

    fn push_circle(&mut self, resolution: u32) -> ModelData {
        let vertex = |x, y| Vertex {
            position: [x, y, 0.0],
            tex_coord: [x + 0.5, y + 0.5],
            normal: [0.0, 0.0, 1.0],
        };

        let mut vertices = Vec::with_capacity(resolution as usize);
        let mut indices = Vec::with_capacity(3 * (resolution as usize - 1));

        let theta = 2.0 * PI / resolution as f32;
        let (sin, cos) = theta.sin_cos();

        let mut dx = 0.5;
        let mut dy = 0.0;

        for _ in 0..resolution {
            vertices.push(vertex(dx, dy));

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

        let range = self.add_vertices(&vertices, &indices);

        ModelData {
            indices: range,
            texture: None,
        }
    }

    fn push_image(
        &mut self,
        path: impl AsRef<Path>,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<ModelData> {
        let image = image::open(&path)
            .with_context(|| format!("failed to open image '{}'", path.as_ref().display()))?
            .into_rgba();

        let (width, height) = image.dimensions();

        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let is_transparent = |col: i32, row: i32| {
            if col < 0 || col >= width as i32 || row < 0 || row >= height as i32 {
                true
            } else {
                let [_, _, _, alpha] = image.get_pixel(col as u32, row as u32).0;
                alpha != 255
            }
        };

        let mut add_face = |quad: Quad| {
            let face = CubeFace::from(quad);

            let start_vertex = vertices.len() as u32;
            vertices.extend_from_slice(&face.vertices);

            let offset_indices = CubeFace::INDICES.iter().map(|i| *i + start_vertex);
            indices.extend(offset_indices);
        };

        for col in 0..width {
            for row in 0..height {
                if !is_transparent(col as i32, row as i32) {
                    let quad = |normal: [f32; 3]| {
                        let normal = Vector3::from(normal);

                        let x = col as f32 - width as f32 / 2.0;
                        let z = (height - row - 1) as f32;

                        let center = Point3::new(x + 0.5, 0.0, z + 0.5) * VOXEL_SIZE
                            + 0.5 * VOXEL_SIZE * normal;

                        let u = (col as f32 + 0.1) / width as f32;
                        let v = (row as f32 + 0.1) / height as f32;

                        Quad {
                            normal,
                            size: [VOXEL_SIZE; 2].into(),
                            center,
                            tex_start: [u, v],
                            tex_end: [u, v],
                        }
                    };

                    let deltas = [[-1, 0], [1, 0], [0, -1], [0, 1]];

                    for &[dx, dy] in &deltas {
                        if is_transparent(col as i32 + dx, row as i32 + dy) {
                            add_face(quad([dx as f32, 0.0, -dy as f32]));
                        }
                    }

                    add_face(quad([0.0, 1.0, 0.0]));
                    add_face(quad([0.0, -1.0, 0.0]));
                }
            }
        }

        let n = indices.len();
        eprintln!(
            "{} vertices, {} indices ({} triangles or {} faces)",
            vertices.len(),
            n,
            n / 3,
            n / 6
        );

        let range = self.add_vertices(&vertices, &indices);
        let texture = super::texture::from_image(&image, device, encoder);

        Ok(ModelData {
            indices: range,
            texture: Some(Arc::new(texture)),
        })
    }
}

struct CubeFace {
    vertices: [Vertex; 4],
}

#[derive(Copy, Clone)]
struct Quad {
    size: Vector2<f32>,
    normal: Vector3<f32>,
    center: Point3<f32>,
    tex_start: [f32; 2],
    tex_end: [f32; 2],
}

impl From<Quad> for CubeFace {
    fn from(quad: Quad) -> CubeFace {
        let right = match Vector3::unit_z().cross(quad.normal) {
            product if product.is_zero() => Vector3::unit_y().cross(quad.normal),
            product => product,
        };
        let up = quad.normal.cross(right);

        let delta_right = right * quad.size.x;
        let delta_up = up * quad.size.y;

        let bottom_left: Point3<f32> = quad.center - 0.5 * delta_right - 0.5 * delta_up;

        let [u0, v0] = quad.tex_start;
        let [u1, v1] = quad.tex_end;

        CubeFace {
            vertices: [
                Vertex {
                    position: bottom_left.into(),
                    normal: quad.normal.into(),
                    tex_coord: [u0, v1],
                },
                Vertex {
                    position: (bottom_left + delta_right).into(),
                    normal: quad.normal.into(),
                    tex_coord: [u1, v1],
                },
                Vertex {
                    position: (bottom_left + delta_right + delta_up).into(),
                    normal: quad.normal.into(),
                    tex_coord: [u1, v0],
                },
                Vertex {
                    position: (bottom_left + delta_up).into(),
                    normal: quad.normal.into(),
                    tex_coord: [u0, v0],
                },
            ],
        }
    }
}

impl CubeFace {
    const INDICES: [u32; 6] = [0, 1, 2, 2, 3, 0];
}
