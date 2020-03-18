use anyhow::{Context, Result};
use logic::components::Model;
use std::collections::HashMap;
use std::f32::consts::PI;
use std::ops::Range;
use std::path::Path;

use super::Vertex;

const VOXEL_SIZE: f32 = 1.0 / 16.0;

pub(super) struct ModelRegistry {
    models: HashMap<Model, ModelData>,
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}

pub struct ModelData {
    pub(super) indices: Range<u32>,
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

    pub fn vertices(&self) -> &[Vertex] {
        &self.vertices
    }

    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    pub fn get_model(&self, model: Model) -> Option<&ModelData> {
        self.models.get(&model)
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
            vertex(-0.5, -0.5),
            vertex(0.5, -0.5),
            vertex(0.5, 0.5),
            vertex(-0.5, 0.5),
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

        self.add_vertices(&vertices, &indices)
    }

    fn push_image(&mut self, path: impl AsRef<Path>) -> Result<ModelData> {
        let image = image::open(&path)
            .with_context(|| format!("failed to open image '{}'", path.as_ref().display()))?
            .into_rgba();

        let (width, height) = image.dimensions();

        let real_width = width as f32 * VOXEL_SIZE;
        let real_depth = VOXEL_SIZE;

        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        for (col, row, color) in image.enumerate_pixels() {
            let [red, green, blue, alpha] = color.0;

            let normalize = |byte| byte as f32 / 255.0;
            let color = [normalize(red), normalize(green), normalize(blue)];

            let x = col as f32 * VOXEL_SIZE;
            let z = (height - row - 1) as f32 * VOXEL_SIZE;

            if alpha != 0 {
                let voxel = Voxel::new(
                    [x - real_width / 2.0, -real_depth / 2.0, z],
                    color,
                    VOXEL_SIZE,
                );

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
