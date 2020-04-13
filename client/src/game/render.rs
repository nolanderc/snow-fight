use cgmath::{prelude::*, Point3, Vector3};

use logic::collision::AlignedBox;
use logic::components::{Breakable, Collision, Health, Model, Position};
use logic::legion::prelude::*;
use logic::tile_map::{TileKind, TileMap};

use crate::renderer::{Frame, Instance};

pub struct RenderOptions {
    pub render_bounds: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        RenderOptions {
            render_bounds: false,
        }
    }
}

impl super::Game {
    pub(super) fn render(&mut self) {
        let mut frame = self.renderer.next_frame(self.camera);

        self.render_ground(&mut frame);
        self.render_entities(&mut frame);
        self.render_breaking_progress(&mut frame);
        self.render_health(&mut frame);

        if self.render_options.render_bounds {
            self.render_bounding_boxes(&mut frame);
        }

        self.renderer.submit(frame);
        self.renderer.cleanup();
    }

    fn render_ground(&self, frame: &mut Frame) {
        let map = <Read<TileMap>>::fetch(&self.world.resources);
        for (position, tile) in map.iter() {
            let color = match tile.kind {
                TileKind::Sand => [1.0, 0.8, 0.0],
                TileKind::Grass => [0.1, 0.8, 0.1],
                TileKind::Water => [0.0, 0.0, 1.0],
            };

            let position = [position.x as f32, position.y as f32, 0.0];
            frame.draw(Model::Rect, Instance::new(position).with_color(color));
        }
    }

    fn render_entities(&self, frame: &mut Frame) {
        let models = <(Read<Position>, Read<Model>)>::query();
        for (entity, (position, model)) in models.iter_entities_immutable(&self.world) {
            let color = if Some(entity) == self.selected {
                [0.5, 0.5, 0.0]
            } else {
                [0.0; 3]
            };

            draw_entity(frame, position.0, *model, color);
        }
    }

    fn render_breaking_progress(&self, frame: &mut Frame) {
        <(Read<Position>, Read<Breakable>)>::query()
            .iter_immutable(&self.world)
            .for_each(|(position, breakable)| {
                draw_indicator(frame, position.0, breakable.durability);
            });
    }

    fn render_health(&self, frame: &mut Frame) {
        <(Read<Position>, Read<Health>, TryRead<Collision>)>::query()
            .iter_immutable(&self.world)
            .for_each(|(position, health, collision)| {
                let top = collision.map(|coll| coll.bounds.high.z).unwrap_or(2.0);
                draw_health_bar(
                    frame,
                    position.0 + Vector3::new(0.0, 0.0, top + 0.4),
                    health.points as f32 / health.max_points as f32,
                );
            });
    }

    fn render_bounding_boxes(&self, frame: &mut Frame) {
        let bounding_boxes = <(Read<Position>, Read<Collision>)>::query();
        for (position, collision) in bounding_boxes.iter_immutable(&self.world) {
            let bounds = collision.bounds.translate(position.0.to_vec());
            draw_bounding_box(frame, bounds, [1.0; 3]);
        }
    }
}

fn draw_entity(frame: &mut Frame, position: Point3<f32>, model: Model, color: [f32; 3]) {
    let instance = match model {
        Model::Circle => Instance::new(position).with_scale([0.9; 3]),

        _ => Instance::new(position),
    };

    frame.draw(model, instance.with_color(color));
}

fn draw_indicator(frame: &mut Frame, point: Point3<f32>, progress: f32) {
    frame.draw(
        Model::Circle,
        Instance::new(point + Vector3::new(0.0, 0.0, 0.01))
            .with_color([0.9, 0.9, 0.1])
            .with_scale([progress; 3]),
    );
}

fn draw_health_bar(frame: &mut Frame, position: Point3<f32>, amount: f32) {
    let width = 0.75;
    let size = 1.0 / 8.0;

    let offset = 0.5 * width * (1.0 - amount);

    frame.draw(
        Model::Cube,
        Instance::new(position)
            .with_color([1.0, 0.0, 0.0])
            .with_scale([width - 0.001, size, size]),
    );

    frame.draw(
        Model::Cube,
        Instance::new(position - Vector3::new(offset, 0.0, 0.0))
            .with_color([0.0, 1.0, 0.0])
            .with_scale([width * amount, 1.1 * size, 1.1 * size]),
    );
}

fn draw_bounding_box(frame: &mut Frame, bounds: AlignedBox, color: [f32; 3]) {
    let size = bounds.high - bounds.low;
    let center = bounds.low + 0.5 * size;
    frame.draw(
        Model::Cube,
        Instance::new(center).with_scale(size).with_color(color),
    )
}
