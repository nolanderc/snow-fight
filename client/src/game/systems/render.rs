use cgmath::{Point2, Point3, Vector3};

use logic::components::{Model, Position};
use logic::legion::prelude::*;
use logic::tile_map::{TileKind, TileMap};

use super::Mouse;
use crate::renderer::{Camera, Frame, Instance, Renderer};

pub fn system() -> logic::System {
    let query = <(Read<Position>, Read<Model>)>::query();

    SystemBuilder::new("renderer")
        .write_resource::<Renderer>()
        .read_resource::<Camera>()
        .read_resource::<Mouse>()
        .read_resource::<TileMap>()
        .with_query(query)
        .build(move |_, world, resources, query| {
            let (renderer, camera, mouse, tile_map) = resources;

            let size = renderer.size();
            let mut frame = renderer.next_frame();
            frame.set_camera(**camera);

            draw_ground(&mut frame, &tile_map);

            for (position, model) in query.iter(world) {
                draw_entity(&mut frame, position.0, *model);
            }

            let screen = mouse.position_screen(size);
            let cursor_dir = camera.cast_ray(size, screen);
            let tile = ray_to_tile(camera.position, cursor_dir);

            draw_tile_indicator(&mut frame, tile);

            frame.submit();
            renderer.cleanup();
        })
}

fn draw_entity(frame: &mut Frame, position: Point3<f32>, model: Model) {
    let mut instance = Instance {
        position,
        scale: [1.0, 1.0, 1.0].into(),
        color: [1.0; 3],
    };

    match model {
        Model::Rect => {
            instance.position.z += 0.01;
            instance.color = [1.0, 0.0, 0.0];
        }

        Model::Circle => {
            instance.position.z += 0.01;
            instance.scale = [0.9; 3].into();
            instance.color = [0.0, 1.0, 0.0];
        }

        _ => {}
    };

    frame.draw(model, instance);
}

fn ray_to_tile(origin: Point3<f32>, direction: Vector3<f32>) -> Point2<i32> {
    let dt = -origin.z / direction.z;
    let pointer = origin + dt * direction;

    Point2 {
        x: pointer.x.round() as i32,
        y: pointer.y.round() as i32,
    }
}

fn draw_ground(frame: &mut Frame, map: &TileMap) {
    for (position, tile) in map.iter() {
        let color = match tile.kind {
            TileKind::Sand => [1.0, 0.8, 0.0],
            TileKind::Grass => [0.1, 0.8, 0.1],
            TileKind::Water => [0.0, 0.0, 1.0],
        };

        let instance = Instance {
            position: [position.x as f32, position.y as f32, 0.0].into(),
            scale: [1.0, 1.0, 1.0].into(),
            color,
        };

        frame.draw(Model::Rect, instance);
    }
}

fn draw_tile_indicator(frame: &mut Frame, tile: Point2<i32>) {
    frame.draw(
        Model::Rect,
        Instance {
            position: Point3::new(tile.x as f32, tile.y as f32, 0.01),
            scale: [1.0; 3].into(),
            color: [0.9, 0.9, 0.1],
        },
    );
}
