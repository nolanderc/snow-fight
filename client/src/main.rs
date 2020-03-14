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
mod systems;

use message::Connection;
use options::Options;
use renderer::{Renderer, RendererConfig};
use systems::{CameraController, WindowState};

use anyhow::{Context, Result};
use cgmath::prelude::*;
use logic::components::Position;
use logic::legion::prelude::*;
use protocol::Init;
use std::sync::{mpsc, Arc};
use std::thread;
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
    world
        .resources
        .get_mut::<CameraController>()
        .unwrap()
        .target = Some(player);

    let schedule = logic::add_systems(Default::default())
        .add_system(systems::fps_display())
        .add_system(systems::player_movement())
        .add_system(systems::update_camera())
        .flush()
        .add_system(systems::render());

    let mut schedule = schedule.build();

    loop {
        while match events.try_recv() {
            Err(mpsc::TryRecvError::Empty) => false,
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err(anyhow!("event loop disconnected"))
            }
            Ok(event) => match handle_event(event, &mut world) {
                ShouldExit::Exit => return Ok(()),
                ShouldExit::Continue => true,
            },
        } {}

        schedule.execute(&mut world);
    }
}

enum ShouldExit {
    Exit,
    Continue,
}

fn handle_event(event: Event, world: &mut World) -> ShouldExit {
    match event {
        Event::Resized(size) => {
            let mut window = world.resources.get_mut::<WindowState>().unwrap();
            window.size = size;

            let mut renderer = world.resources.get_mut::<Renderer>().unwrap();
            renderer.set_size(size.width, size.height);
        }
        Event::KeyDown { key, .. } => {
            {
                let mut window = world.resources.get_mut::<WindowState>().unwrap();
                window.key_pressed(key);
            }

            match key {
                VirtualKeyCode::Tab => switch_closest(world),
                _ => {}
            }
        }
        Event::KeyUp { key, .. } => {
            let mut window = world.resources.get_mut::<WindowState>().unwrap();
            window.key_released(key);

            match key {
                VirtualKeyCode::Escape => return ShouldExit::Exit,
                _ => {}
            }
        }

        Event::MouseDown { button } => {
            let mut window = world.resources.get_mut::<WindowState>().unwrap();
            window.button_pressed(button);
        }
        Event::MouseUp { button } => {
            let mut window = world.resources.get_mut::<WindowState>().unwrap();
            window.button_released(button);
        }

        Event::MouseMotion { delta_x, delta_y } => {
            rotate_camera(world, delta_x, delta_y);
        }

        Event::MouseScroll { delta_y, .. } => {
            let window = world.resources.get::<WindowState>().unwrap();
            if window.key_down(VirtualKeyCode::Space) {
                let mut controller = world.resources.get_mut::<CameraController>().unwrap();
                controller.distance_impulse(-0.01 * delta_y)
            }
        }

        _ => {}
    }

    ShouldExit::Continue
}

fn rotate_camera(world: &mut World, dx: f32, dy: f32) {
    let window = world.resources.get::<WindowState>().unwrap();
    let mut controller = world.resources.get_mut::<CameraController>().unwrap();

    if window.key_down(VirtualKeyCode::Space) {
        let rx = 4.0 * dx / window.size.width as f32;
        let ry = 4.0 * dy / window.size.height as f32;

        if window.button_down(MouseButton::Left) {
            controller.rotation_impulse(-rx, ry);
        } else if window.button_down(MouseButton::Right) {
            controller.distance_impulse(2.0 * ry);
        }
    }
}

fn switch_closest(world: &mut World) {
    let target = world.resources.get::<CameraController>().unwrap().target;
    if let Some(target) = target {
        if let Some(new) = find_closest(target, &*world) {
            world
                .resources
                .get_mut::<CameraController>()
                .unwrap()
                .target = Some(new);
        }
    }
}

fn find_closest(target: Entity, world: &World) -> Option<Entity> {
    let center = **world.get_component::<Position>(target)?;

    let mut new = None;
    let mut closest = f32::max_value();

    let query = Read::<Position>::query();
    let positions = query.iter_entities_immutable(world);

    for (entity, position) in positions {
        let distance = position.distance(center);
        if entity != target && distance < closest {
            new = Some(entity);
            closest = distance;
        }
    }

    new
}
