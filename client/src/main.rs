//! Author(s):
//! - Christofer Nolander (cnol@kth.se)

#![allow(clippy::single_match)]

#[macro_use]
extern crate anyhow;

const TITLE: &str = "Overengineering";

mod message;
mod oneshot;
mod options;
mod renderer;

use message::Connection;
use options::Options;
use renderer::{Camera, Rect, Renderer, RendererConfig};

use anyhow::{Context, Result};
use protocol::Init;
use std::collections::HashSet;
use std::sync::mpsc;
use std::thread;
use std::time;
use structopt::StructOpt;

use winit::{
    dpi::PhysicalSize,
    event::{
        DeviceEvent, ElementState, Event as WinitEvent, KeyboardInput, MouseButton,
        MouseScrollDelta, ScanCode, VirtualKeyCode, WindowEvent,
    },
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};

fn main() -> Result<()> {
    let options = Options::from_args();
    let options = Box::leak(Box::new(options));

    setup_logger(options);

    let event_loop = EventLoop::new();
    let window = Window::new(&event_loop)?;
    let (mut event_tx, event_rx) = mpsc::channel();

    thread::spawn(move || {
        if let Err(e) = run(window, event_rx).context("game loop exited") {
            log::error!("{:#}", e);
        }
    });

    event_loop.run(move |event, _, flow| {
        match dispatch_winit_event(event, &mut event_tx).context("failed to dispatch event") {
            Ok(control) => *flow = control,
            Err(e) => {
                log::error!("{:#}", e);
                *flow = ControlFlow::Exit;
            }
        }
    })
}

/// Setup logging facilities.
fn setup_logger(options: &Options) {
    env_logger::Builder::new()
        .filter_level(options.log_level)
        .init();
}

/// Connect to the server.
#[allow(dead_code)]
fn connect(options: &Options) -> Result<Connection> {
    log::info!(
        "Connecting to server on [{}:{}]...",
        options.addr,
        options.port
    );

    let mut connection = Connection::establish((options.addr, options.port).into())?;

    let init = connection.send(Init {
        nickname: "Zynapse".to_owned(),
    });

    println!("Received: {:?}", init.wait()?);

    Ok(connection)
}

#[derive(Debug, Copy, Clone)]
enum Event {
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

fn dispatch_winit_event(
    event: WinitEvent<()>,
    events: &mut mpsc::Sender<Event>,
) -> Result<ControlFlow> {
    match event {
        WinitEvent::LoopDestroyed => {
            log::info!("closing event loop...");
        }
        WinitEvent::RedrawRequested(_) => {
            events.send(Event::Redraw)?;
        }
        WinitEvent::WindowEvent { event, .. } => match event {
            WindowEvent::CloseRequested | WindowEvent::Destroyed => return Ok(ControlFlow::Exit),
            WindowEvent::Resized(size) => {
                events.send(Event::Resized(size))?;
            }
            WindowEvent::CursorMoved { position, .. } => {
                events.send(Event::CursorMoved {
                    x: position.x as f32,
                    y: position.y as f32,
                })?;
            }
            WindowEvent::KeyboardInput { input, .. } => {
                let KeyboardInput {
                    virtual_keycode,
                    state,
                    scancode,
                    ..
                } = input;

                if let Some(key) = virtual_keycode {
                    let event = match state {
                        ElementState::Pressed => Event::KeyDown { key, scancode },
                        ElementState::Released => Event::KeyUp { key, scancode },
                    };
                    events.send(event)?;
                }
            }
            WindowEvent::MouseInput { button, state, .. } => {
                let event = match state {
                    ElementState::Pressed => Event::MouseDown { button },
                    ElementState::Released => Event::MouseUp { button },
                };
                events.send(event)?;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (delta_x, delta_y) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x, y),
                    MouseScrollDelta::PixelDelta(pos) => (pos.x as f32, pos.y as f32),
                };
                events.send(Event::MouseScroll { delta_x, delta_y })?;
            }
            _ => {}
        },
        WinitEvent::DeviceEvent { event, .. } => match event {
            DeviceEvent::MouseMotion { delta } => {
                events.send(Event::MouseMotion {
                    delta_x: delta.0 as f32,
                    delta_y: delta.1 as f32,
                })?;
            }
            _ => {}
        },
        _ => {}
    }

    Ok(ControlFlow::Wait)
}

fn run(window: Window, events: mpsc::Receiver<Event>) -> Result<()> {
    let size = window.inner_size();

    let mut renderer = Renderer::new(
        &window,
        RendererConfig {
            width: size.width,
            height: size.height,
            samples: 4,
        },
    )?;

    let mut fps_timer = time::Instant::now();
    let mut frames = 0;

    let mut grid_w = 8u32;
    let mut grid_h = 8u32;

    let mut previous_frame = time::Instant::now();

    let mut camera = Camera {
        position: [0.0, -5.0, 2.0],
        focus: [0.0, 0.0, 0.0],
        fov: 70.0,
    };

    let mut theta = 90f32.to_radians();
    let mut phi = 45f32.to_radians();
    let mut distance = 5.0;

    let mut theta_vel = 0.0;
    let mut phi_vel = 0.0;
    let mut distance_vel = 0.0;

    let mut rotating = false;
    let mut zooming = false;

    let mut pressed_keys = HashSet::new();

    let mut window_size = window.inner_size();

    loop {
        frames += 1;
        let elapsed = fps_timer.elapsed();
        if elapsed.as_secs_f32() > 0.5 {
            let frames_per_second = frames as f32 / elapsed.as_secs_f32();
            fps_timer = time::Instant::now();
            frames = 0;
            window.set_title(&format!(
                "{} @ {} fps ({} triangles)",
                TITLE,
                frames_per_second.round(),
                grid_w * grid_h * 2
            ));
        }

        let delta_time = previous_frame.elapsed().as_secs_f32();
        previous_frame = time::Instant::now();

        loop {
            let event = events.try_recv();
            match event {
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err(anyhow!("event loop disconnected"))
                }
                Ok(event) => match event {
                    Event::Resized(size) => {
                        window_size = size;
                        renderer.set_size(size.width, size.height);
                    }
                    Event::KeyDown { key, .. } => {
                        pressed_keys.insert(key);
                    }
                    Event::KeyUp { key, .. } => {
                        pressed_keys.remove(&key);

                        match key {
                            VirtualKeyCode::Escape => return Ok(()),
                            VirtualKeyCode::Right => grid_w += 1,
                            VirtualKeyCode::Left => grid_w = grid_w.saturating_sub(1),
                            VirtualKeyCode::Up => grid_h += 1,
                            VirtualKeyCode::Down => grid_h = grid_h.saturating_sub(1),
                            _ => {}
                        }
                    }

                    Event::MouseDown { button } => match button {
                        MouseButton::Left => rotating = true,
                        MouseButton::Right => zooming = true,
                        _ => {}
                    },
                    Event::MouseUp { button } => match button {
                        MouseButton::Left => rotating = false,
                        MouseButton::Right => zooming = false,
                        _ => {}
                    },

                    Event::MouseMotion { delta_x, delta_y } => {
                        if rotating {
                            let rx = 10.0 * delta_x / window_size.width as f32;
                            let ry = 10.0 * delta_y / window_size.height as f32;
                            theta_vel -= rx;
                            phi_vel += ry;
                        } else if zooming {
                            let ry = 10.0 * delta_y / window_size.height as f32;
                            distance_vel += ry;
                        }
                    }

                    Event::MouseScroll { delta_y, .. } => {
                        distance_vel -= 0.05 * delta_y;
                    }

                    _ => {}
                },
            }
        }

        if pressed_keys.contains(&VirtualKeyCode::Comma) {
            camera.position[1] += 3.0 * delta_time;
        }
        if pressed_keys.contains(&VirtualKeyCode::A) {
            camera.position[0] -= 3.0 * delta_time;
        }
        if pressed_keys.contains(&VirtualKeyCode::O) {
            camera.position[1] -= 3.0 * delta_time;
        }
        if pressed_keys.contains(&VirtualKeyCode::E) {
            camera.position[0] += 3.0 * delta_time;
        }

        let mut frame = renderer.next_frame();

        theta += delta_time * theta_vel;
        phi = (phi + delta_time * phi_vel).max(-1.5).min(1.5);
        distance = (distance + delta_time * distance * distance_vel).max(0.1);

        let rotate_friction: f32 = if rotating { 0.005 } else { 0.1 };
        let zoom_friction: f32 = if zooming { 0.005 } else { 0.1 };

        theta_vel *= rotate_friction.powf(delta_time);
        phi_vel *= rotate_friction.powf(delta_time);
        distance_vel *= zoom_friction.powf(delta_time);

        let (sin_theta, cos_theta) = theta.sin_cos();
        let (sin_phi, cos_phi) = phi.sin_cos();

        let dx = cos_theta * cos_phi;
        let dy = sin_theta * cos_phi;
        let dz = sin_phi;

        camera.position = [distance * dx, distance * dy, distance * dz];

        frame.set_camera(camera);

        for col in 0..grid_w {
            for row in 0..grid_h {
                let rect = Rect {
                    x: 2.0 * col as f32 / grid_w as f32 - 1.0,
                    y: 2.0 * row as f32 / grid_h as f32 - 1.0,
                    w: 2.0 / grid_w as f32,
                    h: 2.0 / grid_h as f32,
                };

                let r = 1.0;
                let g = (col + 1) as f32 / grid_w as f32;
                let b = (row + 1) as f32 / grid_h as f32;

                frame.draw_rect(rect, [r, g, b]);
            }
        }

        frame.submit();
        renderer.cleanup();
    }
}
