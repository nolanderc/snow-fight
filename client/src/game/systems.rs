use std::f32::consts::PI;
use std::sync::Arc;
use std::time::Instant;

const TAU: f32 = 2.0 * PI;

use winit::dpi::PhysicalSize;
use winit::event::{MouseButton, VirtualKeyCode};
use winit::window::Window;

use cgmath::{Point2, Point3, Vector3};

use logic::components::{Model, Position};
use logic::legion::prelude::*;
use logic::resources::TimeStep;

use crate::renderer::{Camera, Instance, Renderer};

type System = logic::System;

pub struct WindowState {
    handle: Arc<Window>,
    pub size: PhysicalSize<u32>,
    pressed_keys: Vec<VirtualKeyCode>,
    mouse_buttons: Vec<MouseButton>,
}

#[derive(Debug)]
pub struct Mouse {
    pub position: Point2<f32>,
}

struct FpsMeter {
    last_tick: Instant,
    frames: u32,
}

pub struct CameraController {
    pub target: Option<Entity>,

    theta: f32,
    phi: f32,
    distance: f32,

    theta_target: f32,
    phi_target: f32,
    distance_target: f32,
}

pub fn init_world(world: &mut World) {
    world.resources.insert(FpsMeter::new());
    world.resources.insert(CameraController::new());
    world.resources.insert(Camera {
        position: [0.0, -5.0, 2.0].into(),
        focus: [0.0, 0.0, 0.0].into(),
        fov: 70.0,
    });
}

pub fn fps_display() -> System {
    SystemBuilder::new("fps_display")
        .write_resource::<FpsMeter>()
        .read_resource::<WindowState>()
        .build(move |_, _, (meter, window), _| {
            if let Some(fps) = meter.tick() {
                let new_title = format!("{} @ {} fps", super::TITLE, fps.round());
                window.handle.set_title(&new_title);
            }
        })
}

pub fn update_camera() -> System {
    SystemBuilder::new("update_camera")
        .read_resource::<TimeStep>()
        .write_resource::<CameraController>()
        .write_resource::<Camera>()
        .read_component::<Position>()
        .build(move |_, world, (dt, controller, camera), _| {
            controller.apply_velocity(**dt);

            let focus = controller
                .target
                .and_then(|entity| world.get_component::<Position>(entity));

            let direction = controller.direction();
            let distance = controller.distance;

            if let Some(focus) = focus {
                let forward = Vector3::new(direction.x, direction.y, 0.0);
                let offset = Vector3::new(0.0, 0.0, 0.5) - 0.5 * distance * forward;

                let focus = **focus + offset;
                let delta = focus - camera.focus;
                let restore = 1.0 - 0.5f32.powf(dt.secs_f32() / 0.05);
                camera.focus += restore * delta;
            }

            camera.position = camera.focus - distance * direction;
        })
}

pub fn render() -> logic::System {
    let query = <(Read<Position>, Read<Model>)>::query();

    SystemBuilder::new("renderer")
        .write_resource::<Renderer>()
        .read_resource::<Camera>()
        .read_resource::<Mouse>()
        .with_query(query)
        .build(move |_, world, resources, query| {
            let (renderer, camera, mouse) = resources;

            let size = renderer.size();
            let mut frame = renderer.next_frame();

            frame.set_camera(**camera);

            frame.draw(
                Model::Rect,
                Instance {
                    position: [0.0, 0.0, 0.0].into(),
                    scale: [1000.0; 3].into(),
                    color: [0.1, 0.5, 0.1],
                },
            );

            for (position, model) in query.iter(world) {
                let mut instance = Instance {
                    position: position.0,
                    scale: [1.0, 1.0, 1.0].into(),
                    color: [1.0; 3],
                };

                match *model {
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

                frame.draw(*model, instance);
            }

            let mut screen = 2.0 * mouse.position;
            screen.x /= size.width as f32;
            screen.x -= 1.0;
            screen.y /= size.height as f32;
            screen.y -= 1.0;

            screen *= -1.0;

            let cursor_dir = camera.cast_ray(size, screen);
            let dt = -camera.position.z / cursor_dir.z;
            let pointer = camera.position + dt * cursor_dir;

            let tile = Point3 {
                x: pointer.x.round(),
                y: pointer.y.round(),
                z: 0.0,
            };

            frame.draw(
                Model::Circle,
                Instance {
                    position: tile + Vector3::new(0.0, 0.0, 0.01),
                    scale: [1.0; 3].into(),
                    color: [0.9, 0.9, 0.1],
                },
            );


            frame.submit();
            renderer.cleanup();
        })
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
            size,
            pressed_keys: Vec::new(),
            mouse_buttons: Vec::new(),
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
}

impl CameraController {
    const DISTANCE_CLOSE: f32 = 3.0;
    const DISTANCE_FAR: f32 = 8.0;

    /// After how many senconds half of the exceeded distance should have restored.
    const ROTATION_HALF_TIME: f32 = 0.1;
    const DISTANCE_HALF_TIME: f32 = 0.05;

    pub fn new() -> Self {
        CameraController {
            target: None,

            theta: (-90f32).to_radians(),
            phi: 0.05,
            distance: Self::DISTANCE_CLOSE,

            theta_target: (-90f32).to_radians(),
            phi_target: 35f32.to_radians(),
            distance_target: (Self::DISTANCE_CLOSE + Self::DISTANCE_FAR) / 2.0,
        }
    }

    pub fn rotation_impulse(&mut self, dx: f32) {
        self.theta_target += dx;
        if self.theta_target > TAU {
            self.theta_target -= TAU;
            self.theta -= TAU;
        } else if self.theta_target < 0.0 {
            self.theta_target += TAU;
            self.theta += TAU;
        }
    }

    pub fn distance_impulse(&mut self, amount: f32) {
        self.distance_target = (self.distance_target + amount)
            .max(Self::DISTANCE_CLOSE)
            .min(Self::DISTANCE_FAR);
    }

    pub(self) fn apply_velocity(&mut self, dt: TimeStep) {
        let dt = dt.secs_f32();

        let rotation_falloff = 1.0 - 0.5f32.powf(dt / Self::ROTATION_HALF_TIME);
        self.theta += rotation_falloff * (self.theta_target - self.theta);
        self.phi += rotation_falloff * (self.phi_target - self.phi);

        let distance_falloff = 1.0 - 0.5f32.powf(dt / Self::DISTANCE_HALF_TIME);
        self.distance += distance_falloff * (self.distance_target - self.distance);
    }

    /// Get the direction in which the camera is facing.
    pub fn direction(&self) -> Vector3<f32> {
        let (sin_theta, cos_theta) = self.theta.sin_cos();
        let (sin_phi, cos_phi) = self.phi.sin_cos();

        let dx = cos_theta * cos_phi;
        let dy = sin_theta * cos_phi;
        let dz = sin_phi;

        [-dx, -dy, -dz].into()
    }
}

impl Default for Mouse {
    fn default() -> Self {
        Mouse {
            position: [0.0, 0.0].into(),
        }
    }
}
