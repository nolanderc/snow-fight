use cgmath::Vector3;
use logic::components::Position;
use logic::legion::prelude::*;
use logic::resources::TimeStep;
use logic::System;

use crate::renderer::Camera;

use std::f32::consts::PI;
const TAU: f32 = 2.0 * PI;

pub struct Controller {
    pub target: Option<Entity>,

    theta: f32,
    phi: f32,
    distance: f32,

    theta_target: f32,
    phi_target: f32,
    distance_target: f32,
}

pub fn update() -> System {
    SystemBuilder::new("update_camera")
        .read_resource::<TimeStep>()
        .write_resource::<Controller>()
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

impl Controller {
    const DISTANCE_CLOSE: f32 = 3.0;
    const DISTANCE_FAR: f32 = 8.0;

    /// After how many senconds half of the exceeded distance should have restored.
    const ROTATION_HALF_TIME: f32 = 0.1;
    const DISTANCE_HALF_TIME: f32 = 0.05;

    pub fn new() -> Self {
        Controller {
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
