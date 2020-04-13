use cgmath::{prelude::*, Point3};
use legion::prelude::*;

use crate::components::*;
use crate::tags::Static;

pub fn throw(world: &mut World, entity: Entity, target: Point3<f32>) {
    let held = world
        .get_component_mut::<WorldInteraction>(entity)
        .unwrap()
        .holding
        .take();

    if let Some(held) = held {
        let position = *world.get_component::<Position>(held).unwrap();
        let delta = target - position.0;

        let collision_listener = CollisionListener::new();

        let acc = Acceleration([0.0, 0.0, -10.0].into());
        let time = delta.magnitude() / 30.0;
        let velocity = Velocity(delta / time - 0.5 * acc.0 * time);

        world.add_component(held, velocity);
        world.add_component(held, collision_listener);
        world.add_component(held, Projectile { damage: 1 });
        world.add_component(held, acc);
        world.remove_tag::<Static>(held);
    }
}
