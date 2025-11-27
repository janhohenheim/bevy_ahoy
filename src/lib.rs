#![doc = include_str!("../readme.md")]

/// Everything you need to get started with `bevy_ahoy`
pub mod prelude {
    pub(crate) use {
        avian3d::prelude::*,
        bevy_app::prelude::*,
        bevy_derive::{Deref, DerefMut},
        bevy_ecs::prelude::*,
        bevy_enhanced_input::prelude::*,
        bevy_math::prelude::*,
        bevy_reflect::prelude::*,
        bevy_time::prelude::*,
        bevy_transform::prelude::*,
        bevy_utils::prelude::*,
    };

    pub use crate::{
        AhoyPlugin, AhoySystems,
        camera::{CharacterControllerCamera, CharacterControllerCameraOf},
        input::{Crouch, Jump, Movement, RotateCamera},
        kcc::CharacterController,
    };
}

use bevy_ecs::{intern::Interned, schedule::ScheduleLabel};

use crate::prelude::*;

pub mod camera;
mod fixed_update_utils;
pub mod input;
pub mod kcc;

/// Also requires you to add [`PhysicsPlugins`] and [`EnhancedInputPlugin`] to work properly.
pub struct AhoyPlugin {
    schedule: Interned<dyn ScheduleLabel>,
}

impl AhoyPlugin {
    /// Create a new plugin in the given schedule. The default is [`FixedPostUpdate`].
    pub fn new(schedule: impl ScheduleLabel) -> Self {
        Self {
            schedule: schedule.intern(),
        }
    }
}

impl Default for AhoyPlugin {
    fn default() -> Self {
        Self {
            schedule: FixedPostUpdate.intern(),
        }
    }
}

impl Plugin for AhoyPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            self.schedule,
            (AhoySystems::MoveCharacters)
                .chain()
                .in_set(PhysicsSystems::First),
        )
        .add_plugins((
            camera::plugin,
            input::plugin,
            kcc::plugin(self.schedule),
            fixed_update_utils::plugin,
        ));
    }
}

/// System set used by all systems of `bevy_ahoy`.
#[derive(SystemSet, Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum AhoySystems {
    MoveCharacters,
}
