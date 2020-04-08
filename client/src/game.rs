mod camera;
mod render;

use crate::renderer::{Camera, Renderer, RendererConfig, Size};
use camera::Controller;
use render::RenderOptions;

use anyhow::Result;

use cgmath::prelude::*;
use cgmath::{Point2, Point3, Vector3};

use logic::components::*;
use logic::legion::prelude::*;
use logic::tags;

use std::f32::consts::PI;
use std::sync::Arc;
use std::time::Instant;

const TITLE: &str = "Overengineering";

use winit::{
    dpi::PhysicalSize,
    event::{MouseButton, ScanCode, VirtualKeyCode},
    window::Window,
};

pub struct Game {
    world: World,
    executor: logic::Executor,

    fps_meter: FpsMeter,

    renderer: Renderer,
    render_options: RenderOptions,
    camera: Camera,
    controller: Controller,

    window: WindowState,

    should_exit: bool,

    player: Entity,
    selected: Option<Entity>,
}

struct FpsMeter {
    last_tick: Instant,
    frames: u32,
}

pub struct WindowState {
    handle: Arc<Window>,
    pub size: Size,
    pressed_keys: Vec<VirtualKeyCode>,
    mouse_buttons: Vec<MouseButton>,
    pub mouse_position: Point2<f32>,
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
    pub async fn new(window: Window) -> Result<Game> {
        let window = Arc::new(window);

        let size = window.inner_size();

        let renderer = Renderer::new(
            &window,
            RendererConfig {
                width: size.width,
                height: size.height,
                samples: 1,
            },
        )
        .await?;

        let mut world = logic::create_world();
        let mut controller = Controller::new();

        let schedule = logic::add_systems(Default::default());
        let executor = logic::Executor::new(schedule);

        let player = logic::add_player(&mut world);
        controller.target = Some(player);

        let camera = Camera {
            position: [0.0, -5.0, 2.0].into(),
            focus: [0.0, 0.0, 0.0].into(),
            fov: 70.0,
        };

        Ok(Game {
            world,
            executor,

            fps_meter: FpsMeter::new(),

            window: WindowState::new(window),

            renderer,
            render_options: Default::default(),
            camera,
            controller,

            should_exit: false,

            player,
            selected: None,
        })
    }

    pub fn is_running(&self) -> bool {
        !self.should_exit
    }

    pub fn handle_event(&mut self, event: Event) {
        match event {
            Event::Resized(PhysicalSize { width, height }) => self.resize(Size { width, height }),
            Event::KeyDown { key, scancode } => {
                self.window.key_pressed(key);
                self.key_down(key, scancode);
            }
            Event::KeyUp { key, scancode } => {
                self.window.key_released(key);
                self.key_up(key, scancode);
            }
            Event::MouseDown { button } => {
                self.window.button_pressed(button);
                self.button_down(button);
            }
            Event::MouseUp { button } => {
                self.window.button_released(button);
                self.button_up(button);
            }
            Event::CursorMoved { x, y } => {
                self.window.mouse_position = [x, y].into();
                self.cursor_moved([x, y].into());
            }
            Event::MouseMotion { delta_x, delta_y } => {
                self.rotate_camera(delta_x, delta_y);
            }
            Event::MouseScroll { delta_y, .. } => {
                if self.window.key_down(VirtualKeyCode::Space) {
                    self.controller.distance_impulse(-0.01 * delta_y)
                }
            }

            _ => {}
        }
    }

    fn resize(&mut self, size: Size) {
        self.window.size = size;
        self.renderer.set_size(size.width, size.height);
    }

    fn key_down(&mut self, key: VirtualKeyCode, scancode: ScanCode) {
        match key {
            VirtualKeyCode::Tab => self.switch_closest(),
            VirtualKeyCode::F1 => {
                self.render_options.render_bounds ^= true;
            }
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
                self.controller.rotation_impulse(PI / 2.0);
            }
            qwerty::E => {
                self.controller.rotation_impulse(-PI / 2.0);
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

    fn button_down(&mut self, button: MouseButton) {
        match button {
            MouseButton::Right => {
                let held = self
                    .world
                    .get_component_mut::<WorldInteraction>(self.player)
                    .unwrap()
                    .holding
                    .take();

                if let Some(held) = held {
                    let (origin, direction) = self.mouse_ray();
                    let dt = -origin.z / direction.z;
                    let floor = origin + dt * direction;

                    let position = *self.world.get_component::<Position>(self.player).unwrap();
                    let delta = floor - position.0;
                    let direction = Vector3::new(delta.x, delta.y, 0.0).normalize();

                    let velocity = Velocity(20.0 * direction);
                    let collision_listener = CollisionListener::new();

                    self.world.add_component(held, velocity);
                    self.world.add_component(held, collision_listener);
                    self.world.add_component(held, Projectile { damage: 1 });
                    self.world.add_tag(held, tags::Moveable);
                }
            }

            _ => {}
        }
    }

    fn button_up(&mut self, _button: MouseButton) {}

    fn cursor_moved(&mut self, _position: Point2<f32>) {}

    pub fn tick(&mut self) {
        self.update_selected();
        self.update_breaking();
        self.executor.tick(&mut self.world);
        self.update_camera();
        self.render();
        self.update_fps();
    }

    fn update_fps(&mut self) {
        if let Some(fps) = self.fps_meter.tick() {
            let new_title = format!("{} @ {} fps", TITLE, fps.round());
            self.window.handle.set_title(&new_title);
        }
    }

    fn rotate_camera(&mut self, dx: f32, dy: f32) {
        if self.window.key_down(VirtualKeyCode::Space) {
            if self.window.button_down(MouseButton::Left) {
                let rx = 4.0 * dx / self.window.size.width as f32;
                self.controller.rotation_impulse(-rx);
            } else if self.window.button_down(MouseButton::Right) {
                let ry = 8.0 * dy / self.window.size.height as f32;
                self.controller.distance_impulse(ry);
            }
        }
    }

    fn switch_closest(&mut self) {
        if let Some(target) = self.controller.target {
            if let Some(new) = self.find_closest(target) {
                self.controller.target = Some(new);
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

    fn update_selected(&mut self) {
        let (origin, direction) = self.mouse_ray();
        self.selected = self.ray_pick_entity(origin, direction);
    }

    fn ray_pick_entity(&self, origin: Point3<f32>, direction: Vector3<f32>) -> Option<Entity> {
        <(Read<Position>, Read<Collision>)>::query()
            .filter(component::<Breakable>())
            .iter_entities_immutable(&self.world)
            .filter_map(|(entity, (position, collision))| {
                let bounds = collision.bounds.translate(position.0.to_vec());

                match bounds.ray_intersection(origin, direction) {
                    Some(intersection) if intersection.distance > 0.0 => {
                        Some((intersection.distance, entity))
                    }
                    _ => None,
                }
            })
            .min_by(|(a_distance, _), (b_distance, _)| {
                a_distance
                    .partial_cmp(&b_distance)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(_, target)| target)
    }

    fn update_breaking(&mut self) {
        let is_breaking = self.window.button_down(MouseButton::Left);

        self.world
            .get_component_mut::<WorldInteraction>(self.player)
            .unwrap()
            .breaking = if is_breaking { self.selected } else { None };
    }

    fn mouse_ray(&self) -> (Point3<f32>, Vector3<f32>) {
        let direction = self
            .camera
            .cast_ray(self.window.size, self.window.mouse_screen());
        (self.camera.position, direction)
    }
}

impl FpsMeter {
    pub fn new() -> Self {
        FpsMeter {
            last_tick: Instant::now(),
            frames: 0,
        }
    }

    pub fn tick(&mut self) -> Option<f32> {
        self.frames += 1;
        let now = Instant::now();
        let seconds = now.saturating_duration_since(self.last_tick).as_secs_f32();
        if seconds > 0.5 {
            let frames_per_second = self.frames as f32 / seconds;

            self.last_tick = now;
            self.frames = 0;

            Some(frames_per_second)
        } else {
            None
        }
    }
}

impl WindowState {
    pub fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        WindowState {
            handle: window,
            size: Size {
                width: size.width,
                height: size.height,
            },
            pressed_keys: Vec::new(),
            mouse_buttons: Vec::new(),
            mouse_position: [size.width as f32 / 2.0, size.height as f32 / 2.0].into(),
        }
    }

    pub fn key_pressed(&mut self, key: VirtualKeyCode) {
        self.pressed_keys.push(key);
    }

    pub fn key_released(&mut self, key: VirtualKeyCode) {
        self.pressed_keys.retain(|pressed| *pressed != key);
    }

    pub fn button_pressed(&mut self, button: MouseButton) {
        self.mouse_buttons.push(button);
    }

    pub fn button_released(&mut self, button: MouseButton) {
        self.mouse_buttons.retain(|pressed| *pressed != button);
    }

    pub fn key_down(&self, key: VirtualKeyCode) -> bool {
        self.pressed_keys.contains(&key)
    }

    pub fn button_down(&self, button: MouseButton) -> bool {
        self.mouse_buttons.contains(&button)
    }

    pub fn mouse_screen(&self) -> Point2<f32> {
        let mut screen = 2.0 * self.mouse_position;
        screen.x /= self.size.width as f32;
        screen.x -= 1.0;
        screen.y /= self.size.height as f32;
        screen.y -= 1.0;
        screen
    }
}
