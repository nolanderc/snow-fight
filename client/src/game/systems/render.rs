use cgmath::{prelude::*, Point3};

use logic::collision::AlignedBox;
use logic::components::{CollisionBox, Model, Position};
use logic::legion::prelude::*;
use logic::tile_map::{TileCoord, TileKind, TileMap};

use super::WindowState;
use crate::renderer::{Camera, Frame, Instance, Renderer};

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

pub fn render() -> logic::System {
    let models = <(Read<Position>, Read<Model>)>::query();
    let bounding_boxes = <(Read<Position>, Read<CollisionBox>)>::query();

    SystemBuilder::new("renderer")
        .write_resource::<Renderer>()
        .read_resource::<Camera>()
        .read_resource::<WindowState>()
        .read_resource::<TileMap>()
        .read_resource::<RenderOptions>()
        .with_query(models)
        .with_query(bounding_boxes)
        .build(move |_, world, resources, queries| {
            let (renderer, camera, window, map, render_options) = resources;
            let (models, bounding_boxes) = queries;

            let size = renderer.size();
            let mut frame = renderer.next_frame();
            frame.set_camera(**camera);

            draw_ground(&mut frame, &map);

            for (position, model) in models.iter(world) {
                draw_entity(&mut frame, position.0, *model);
            }

            if render_options.render_bounds {
                for (position, collision) in bounding_boxes.iter(world) {
                    let bounds = collision.0.translate(position.0.to_vec());
                    draw_bounding_box(&mut frame, bounds, [1.0; 3]);
                }
            }

            let cursor_dir = camera.cast_ray(size, window.mouse_screen());
            let tile = TileCoord::from_ray(camera.position, cursor_dir);
            let progress = map
                .get(tile)
                .and_then(|tile| tile.slot.as_ref().map(|slot| slot.durability))
                .unwrap_or(1.0);
            draw_tile_indicator(&mut frame, tile, progress);

            frame.submit();
            renderer.cleanup();
        })
}

fn draw_entity(frame: &mut Frame, position: Point3<f32>, model: Model) {
    let instance = match model {
        Model::Rect => Instance::new(position).with_color([1.0, 0.0, 0.0]),

        Model::Circle => Instance::new(position)
            .with_scale([0.9; 3])
            .with_color([0.0, 1.0, 0.0]),

        _ => Instance::new(position),
    };

    frame.draw(model, instance);
}

fn draw_ground(frame: &mut Frame, map: &TileMap) {
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

fn draw_tile_indicator(frame: &mut Frame, tile: TileCoord, progress: f32) {
    frame.draw(
        Model::Circle,
        Instance::new([tile.x as f32, tile.y as f32, 0.01])
            .with_color([0.9, 0.9, 0.1])
            .with_scale([progress; 3]),
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
