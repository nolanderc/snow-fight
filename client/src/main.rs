//! Author(s):
//! - Christofer Nolander (cnol@kth.se)

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
        ElementState, Event as WinitEvent, KeyboardInput, ScanCode, VirtualKeyCode, WindowEvent,
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
    KeyDown {
        key: VirtualKeyCode,
        scancode: ScanCode,
    },
    KeyUp {
        key: VirtualKeyCode,
        scancode: ScanCode,
    },
    Resized(PhysicalSize<u32>),
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

    let mut w = 8u32;
    let mut h = 8u32;

    let mut previous_frame = time::Instant::now();

    let mut camera = Camera {
        position: [0.0, -5.0, 2.0],
        focus: [0.0, 0.0, 0.0],
        fov: 70.0,
    };

    let mut pressed_keys = HashSet::new();

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
                w * h * 2
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
                        renderer.set_size(size.width, size.height);
                    }

                    Event::KeyDown { key, .. } => {
                        pressed_keys.insert(key);
                    }
                    Event::KeyUp { key, .. } => {
                        pressed_keys.remove(&key);

                        match key {
                            VirtualKeyCode::Escape => return Ok(()),
                            VirtualKeyCode::Right => w += 1,
                            VirtualKeyCode::Left => w = w.saturating_sub(1),
                            VirtualKeyCode::Up => h += 1,
                            VirtualKeyCode::Down => h = h.saturating_sub(1),
                            _ => {}
                        }
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

        frame.set_camera(camera);

        for col in 0..w {
            for row in 0..h {
                let rect = Rect {
                    x: 2.0 * col as f32 / w as f32 - 1.0,
                    y: 2.0 * row as f32 / h as f32 - 1.0,
                    w: 2.0 / w as f32,
                    h: 2.0 / h as f32,
                };

                let r = 1.0;
                let g = (col + 1) as f32 / w as f32;
                let b = (row + 1) as f32 / h as f32;

                frame.draw_rect(rect, [r, g, b]);
            }
        }

        frame.submit();
        renderer.cleanup();
    }
}
