mod systems;

use crate::renderer::{Camera, Renderer, RendererConfig, Size};
use systems::{camera::Controller, render::RenderOptions, WindowState};

use anyhow::Result;
use cgmath::prelude::*;
use cgmath::{Point2, Point3, Vector3};
use logic::components::{Direction, Movement, Position, WorldInteraction};
use logic::legion::prelude::*;
use logic::tile_map::TileCoord;
use std::f32::consts::PI;
use std::sync::Arc;

const TITLE: &str = "Overengineering";

use winit::{
    dpi::PhysicalSize,
    event::{MouseButton, ScanCode, VirtualKeyCode},
    window::Window,
};

pub struct Game {
    world: World,
    schedule: Schedule,
    player: Entity,
    should_exit: bool,
}

#[derive(Debug, Copy, Clone)]
pub enum Event {
    Redraw,
    Resized(PhysicalSize<u32>),
    KeyDown {
        key: VirtualKeyCode,
        scancode: ScanCode,
    },
    KeyUp {
        key: VirtualKeyCode,
        scancode: ScanCode,
    },
    CursorMoved {
        x: f32,
        y: f32,
    },
    MouseMotion {
        delta_x: f32,
        delta_y: f32,
    },
    MouseDown {
        button: MouseButton,
    },
    MouseUp {
        button: MouseButton,
    },
    MouseScroll {
        delta_x: f32,
        delta_y: f32,
    },
}

mod qwerty {
    #![cfg(target_os = "macos")]

    pub const Q: u32 = 12;
    pub const W: u32 = 13;
    pub const E: u32 = 14;

    pub const A: u32 = 0;
    pub const S: u32 = 1;
    pub const D: u32 = 2;
}

impl Game {
    pub fn new(window: Window) -> Result<Game> {
        let window = Arc::new(window);

        let size = window.inner_size();

        let renderer = Renderer::new(
            &window,
            RendererConfig {
                width: size.width,
                height: size.height,
                samples: 1,
            },
        )?;

        let mut world = logic::create_world();
        systems::init_world(&mut world);
        world.resources.insert(renderer);
        world.resources.insert(WindowState::new(window));

        let player = logic::add_player(&mut world);
        world.resources.get_mut::<Controller>().unwrap().target = Some(player);

        let schedule = logic::add_systems(Default::default())
            .add_system(systems::fps_display())
            .add_system(systems::camera::update())
            .flush()
            .add_system(systems::render::render());

        let schedule = schedule.build();

        Ok(Game {
            world,
            schedule,
            player,
            should_exit: false,
        })
    }

    pub fn is_running(&self) -> bool {
        !self.should_exit
    }

    pub fn handle_event(&mut self, event: Event) {
        match event {
            Event::Resized(PhysicalSize { width, height }) => self.resize(Size { width, height }),
            Event::KeyDown { key, scancode } => {
                self.key_down(key, scancode);
                let mut window = self.world.resources.get_mut::<WindowState>().unwrap();
                window.key_pressed(key);
            }
            Event::KeyUp { key, scancode } => {
                self.key_up(key, scancode);
                let mut window = self.world.resources.get_mut::<WindowState>().unwrap();
                window.key_released(key);
            }
            Event::MouseDown { button } => {
                self.button_down(button);
                let mut window = self.world.resources.get_mut::<WindowState>().unwrap();
                window.button_pressed(button);
            }
            Event::MouseUp { button } => {
                self.button_up(button);
                let mut window = self.world.resources.get_mut::<WindowState>().unwrap();
                window.button_released(button);
            }
            Event::CursorMoved { x, y } => {
                self.cursor_moved([x, y].into());
                let mut window = self.world.resources.get_mut::<WindowState>().unwrap();
                window.mouse_position = [x, y].into();
            }
            Event::MouseMotion { delta_x, delta_y } => {
                self.rotate_camera(delta_x, delta_y);
            }
            Event::MouseScroll { delta_y, .. } => {
                let window = self.world.resources.get::<WindowState>().unwrap();
                if window.key_down(VirtualKeyCode::Space) {
                    let mut controller = self.world.resources.get_mut::<Controller>().unwrap();
                    controller.distance_impulse(-0.01 * delta_y)
                }
            }

            _ => {}
        }
    }

    fn resize(&mut self, size: Size) {
        let mut window = self.world.resources.get_mut::<WindowState>().unwrap();
        window.size = size;

        let mut renderer = self.world.resources.get_mut::<Renderer>().unwrap();
        renderer.set_size(size.width, size.height);
    }

    fn key_down(&mut self, key: VirtualKeyCode, scancode: ScanCode) {
        match key {
            VirtualKeyCode::Tab => self.switch_closest(),
            VirtualKeyCode::F1 => {
                let mut options = self.world.resources.get_mut::<RenderOptions>().unwrap();
                options.render_bounds ^= true;
            },
            _ => {}
        }

        let set_direction = |game: &mut Game, direction| {
            game.world
                .get_component_mut::<Movement>(game.player)
                .unwrap()
                .direction
                .insert(direction)
        };

        match scancode {
            qwerty::W => set_direction(self, Direction::NORTH),
            qwerty::A => set_direction(self, Direction::WEST),
            qwerty::S => set_direction(self, Direction::SOUTH),
            qwerty::D => set_direction(self, Direction::EAST),

            qwerty::Q => {
                let mut controller = self.world.resources.get_mut::<Controller>().unwrap();
                controller.rotation_impulse(PI / 2.0);
            }
            qwerty::E => {
                let mut controller = self.world.resources.get_mut::<Controller>().unwrap();
                controller.rotation_impulse(-PI / 2.0);
            }

            _ => {}
        }
    }

    fn key_up(&mut self, key: VirtualKeyCode, scancode: ScanCode) {
        match key {
            VirtualKeyCode::Escape => self.should_exit = true,
            _ => {}
        }

        let reset_direction = |game: &mut Game, direction| {
            game.world
                .get_component_mut::<Movement>(game.player)
                .unwrap()
                .direction
                .remove(direction)
        };

        match scancode {
            qwerty::W => reset_direction(self, Direction::NORTH),
            qwerty::A => reset_direction(self, Direction::WEST),
            qwerty::S => reset_direction(self, Direction::SOUTH),
            qwerty::D => reset_direction(self, Direction::EAST),

            _ => {}
        }
    }

    fn button_down(&mut self, _button: MouseButton) {}

    fn button_up(&mut self, _button: MouseButton) {}

    fn cursor_moved(&mut self, _position: Point2<f32>) {}

    pub fn tick(&mut self) {
        self.update_breaking();
        self.schedule.execute(&mut self.world)
    }

    fn rotate_camera(&mut self, dx: f32, dy: f32) {
        let window = self.world.resources.get::<WindowState>().unwrap();
        let mut controller = self.world.resources.get_mut::<Controller>().unwrap();

        if window.key_down(VirtualKeyCode::Space) {
            if window.button_down(MouseButton::Left) {
                let rx = 4.0 * dx / window.size.width as f32;
                controller.rotation_impulse(-rx);
            } else if window.button_down(MouseButton::Right) {
                let ry = 8.0 * dy / window.size.height as f32;
                controller.distance_impulse(ry);
            }
        }
    }

    fn switch_closest(&mut self) {
        let target = self.world.resources.get::<Controller>().unwrap().target;
        if let Some(target) = target {
            if let Some(new) = self.find_closest(target) {
                self.world.resources.get_mut::<Controller>().unwrap().target = Some(new);
            }
        }
    }

    fn find_closest(&self, target: Entity) -> Option<Entity> {
        let center = **self.world.get_component::<Position>(target)?;

        let mut new = None;
        let mut closest = f32::max_value();

        let query = Read::<Position>::query();
        let positions = query.iter_entities_immutable(&self.world);

        for (entity, position) in positions {
            let distance = position.distance(center);
            if entity != target && distance < closest {
                new = Some(entity);
                closest = distance;
            }
        }

        new
    }

    fn update_breaking(&mut self) {
        let (origin, direction) = self.mouse_ray();
        let tile = TileCoord::from_ray(origin, direction);

        let is_breaking = self
            .world
            .resources
            .get::<WindowState>()
            .unwrap()
            .button_down(MouseButton::Left);

        self.world
            .get_component_mut::<WorldInteraction>(self.player)
            .unwrap()
            .breaking = if is_breaking { Some(tile) } else { None };
    }

    fn mouse_ray(&self) -> (Point3<f32>, Vector3<f32>) {
        let camera = self.world.resources.get::<Camera>().unwrap();
        let window = self.world.resources.get::<WindowState>().unwrap();
        let direction = camera.cast_ray(window.size, window.mouse_screen());
        (camera.position, direction)
    }
}
