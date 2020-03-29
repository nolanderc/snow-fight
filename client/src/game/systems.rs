pub mod camera;
pub mod render;

use std::sync::Arc;
use std::time::Instant;

use winit::event::{MouseButton, VirtualKeyCode};
use winit::window::Window;

use cgmath::Point2;

use logic::legion::prelude::*;
use logic::System;

use crate::renderer::{Camera, Size};

pub struct WindowState {
    handle: Arc<Window>,
    pub size: Size,
    pressed_keys: Vec<VirtualKeyCode>,
    mouse_buttons: Vec<MouseButton>,
    pub mouse_position: Point2<f32>,
}

struct FpsMeter {
    last_tick: Instant,
    frames: u32,
}

pub fn init_world(world: &mut World) {
    world.resources.insert(FpsMeter::new());
    world.resources.insert(camera::Controller::new());
    world.resources.insert(Camera {
        position: [0.0, -5.0, 2.0].into(),
        focus: [0.0, 0.0, 0.0].into(),
        fov: 70.0,
    });
    world.resources.insert(render::RenderOptions::default());
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
