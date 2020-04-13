//! Author(s):
//! - Christofer Nolander (cnol@kth.se)

#![allow(clippy::single_match)]

#[macro_use]
extern crate anyhow;

mod game;
mod message;
mod oneshot;
mod options;
mod renderer;

use game::{Event, Game};
use message::Connection;
use options::Options;

use anyhow::{Context, Result};
use protocol::Init;
use std::sync::mpsc;
use std::thread;
use structopt::StructOpt;

use winit::{
    event::{
        DeviceEvent, ElementState, Event as WinitEvent, KeyboardInput, MouseScrollDelta,
        WindowEvent,
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

    let connection = connect(options)?;

    thread::spawn(move || {
        if let Err(e) = run(window, event_rx, connection).context("game loop exited") {
            log::error!("{:?}", e);
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
    let mut builder = env_logger::Builder::new();
    builder.filter_level(log::LevelFilter::Info);

    for filter in &options.log_level {
        builder.filter(filter.module.as_deref(), filter.level);
    }

    builder.init();
}

/// Connect to the server.
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

/// Run the game logic and graphics frontend.
fn run(window: Window, events: mpsc::Receiver<Event>, connection: Connection) -> Result<()> {
    let mut game = futures::executor::block_on(Game::new(window, connection))?;

    while game.is_running() {
        while match events.try_recv() {
            Err(mpsc::TryRecvError::Empty) => false,
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err(anyhow!("event loop disconnected"))
            }
            Ok(event) => {
                game.handle_event(event);
                true
            }
        } {}

        game.tick()?;
    }

    Ok(())
}

/// Convert a window event to a game input event and send it along the channel.
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
