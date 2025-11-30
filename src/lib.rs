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
        AhoyPlugin, AhoySystems, CharacterController, PickupConfig,
        camera::{CharacterControllerCamera, CharacterControllerCameraOf},
        input::{Crouch, DropObject, Jump, Movement, PullObject, RotateCamera, ThrowObject},
        pickup,
    };
}

use crate::{input::AccumulatedInput, prelude::*};
use avian_pickup::AvianPickupPlugin;
pub use avian_pickup::{
    self as pickup,
    prelude::{
        AvianPickupActor as PickupConfig, AvianPickupActorHoldConfig as PickupHoldConfig,
        AvianPickupActorPullConfig as PickupPullConfig,
        AvianPickupActorThrowConfig as PickupThrowConfig,
    },
};
use avian3d::{
    character_controller::move_and_slide::MoveHitData,
    parry::shape::{Capsule, SharedShape},
};
use bevy_ecs::{
    intern::Interned, lifecycle::HookContext, relationship::RelationshipSourceCollection as _,
    schedule::ScheduleLabel, world::DeferredWorld,
};
use bevy_time::Stopwatch;
use core::time::Duration;
use std::sync::Arc;

pub mod camera;
mod dynamics;
mod fixed_update_utils;
pub mod input;
mod kcc;
mod pickup_glue;

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
            (
                AhoySystems::MoveCharacters,
                AhoySystems::ApplyForcesToDynamicRigidBodies,
            )
                .chain()
                .in_set(PhysicsSystems::First),
        )
        .add_plugins((
            camera::plugin,
            input::plugin,
            kcc::plugin(self.schedule),
            fixed_update_utils::plugin,
            pickup_glue::plugin,
            dynamics::plugin(self.schedule),
            AvianPickupPlugin::default(),
        ));
    }
}

/// System set used by all systems of `bevy_ahoy`.
#[derive(SystemSet, Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum AhoySystems {
    MoveCharacters,
    ApplyForcesToDynamicRigidBodies,
}

#[derive(Component, Clone, Reflect, Debug)]
#[reflect(Component)]
#[require(
    AccumulatedInput,
    CharacterControllerState,
    TranslationInterpolation,
    RigidBody = RigidBody::Kinematic,
    Collider = Collider::cylinder(0.7, 1.8),
    CustomPositionIntegration,
    Transform,
    SpeculativeMargin::ZERO,
)]
#[component(on_add=CharacterController::on_add)]
pub struct CharacterController {
    pub crouch_height: f32,
    pub filter: SpatialQueryFilter,
    pub standing_view_height: f32,
    pub crouch_view_height: f32,
    pub ground_distance: f32,
    pub step_down_detection_distance: f32,
    pub min_walk_cos: f32,
    pub stop_speed: f32,
    pub friction_hz: f32,
    pub acceleration_hz: f32,
    pub air_acceleration_hz: f32,
    pub gravity: f32,
    pub step_size: f32,
    pub crouch_speed_scale: f32,
    pub speed: f32,
    pub air_speed: f32,
    pub move_and_slide: MoveAndSlideConfig,
    pub max_speed: f32,
    pub jump_height: f32,
    pub max_air_speed: f32,
    pub unground_speed: f32,
    pub coyote_time: Duration,
    pub jump_input_buffer: Duration,
    pub step_from_air: bool,
    pub step_into_air: bool,
    pub min_step_ledge_space: f32,
}

impl Default for CharacterController {
    fn default() -> Self {
        Self {
            crouch_height: 1.3,
            filter: SpatialQueryFilter::default(),
            standing_view_height: 1.7,
            crouch_view_height: 1.2,
            ground_distance: 0.05,
            min_walk_cos: 0.766,
            stop_speed: 2.54,
            friction_hz: 4.0,
            acceleration_hz: 5.0,
            air_acceleration_hz: 12.0,
            gravity: 20.3,
            step_size: 1.0,
            crouch_speed_scale: 1.0 / 3.0,
            speed: 10.0,
            air_speed: 1.5,
            move_and_slide: MoveAndSlideConfig {
                skin_width: 0.0075,
                ..default()
            },
            max_speed: 100.0,
            jump_height: 1.5,
            max_air_speed: 0.76,
            unground_speed: 10.0,
            step_down_detection_distance: 0.2,
            min_step_ledge_space: 0.5,
            coyote_time: Duration::from_millis(150),
            jump_input_buffer: Duration::from_millis(150),
            step_from_air: true,
            step_into_air: false,
        }
    }
}

impl CharacterController {
    pub fn on_add(mut world: DeferredWorld, ctx: HookContext) {
        {
            let Some(mut kcc) = world.get_mut::<Self>(ctx.entity) else {
                return;
            };
            kcc.filter.excluded_entities.add(ctx.entity);
        }

        let crouch_height = {
            let Some(kcc) = world.get::<Self>(ctx.entity) else {
                return;
            };
            kcc.crouch_height
        };

        let Some(collider) = world.entity(ctx.entity).get::<Collider>().cloned() else {
            return;
        };
        let standing_aabb = collider.aabb(default(), Rotation::default());
        let standing_height = standing_aabb.max.y - standing_aabb.min.y;

        let Some(mut state) = world.get_mut::<CharacterControllerState>(ctx.entity) else {
            return;
        };
        state.standing_collider = collider.clone();

        let frac = crouch_height / standing_height;

        let mut crouching_collider = Collider::from(SharedShape(Arc::from(
            state.standing_collider.shape().clone_dyn(),
        )));

        if crouching_collider.shape().as_capsule().is_some() {
            let capsule = crouching_collider
                .shape_mut()
                .make_mut()
                .as_capsule_mut()
                .unwrap();
            let radius = capsule.radius;
            let new_height = (crouch_height - radius).max(0.0);
            *capsule = Capsule::new_y(new_height / 2.0, radius);
        } else {
            // note: well-behaved shapes like cylinders and cuboids will not actually subdivide when scaled, yay
            crouching_collider.set_scale(vec3(1.0, frac, 1.0), 16);
        }
        state.crouching_collider = Collider::compound(vec![(
            Vec3::Y * (crouch_height - standing_height) / 2.0,
            Rotation::default(),
            crouching_collider,
        )]);
    }
}

#[derive(Component, Clone, Reflect, Debug)]
#[reflect(Component)]
pub struct CharacterControllerState {
    pub base_velocity: Vec3,
    #[reflect(ignore)]
    pub standing_collider: Collider,
    #[reflect(ignore)]
    pub crouching_collider: Collider,
    pub grounded: Option<MoveHitData>,
    pub crouching: bool,
    pub touching_entities: Vec<TouchingEntity>,
    pub last_ground: Stopwatch,
    pub last_step_up: Stopwatch,
    pub last_step_down: Stopwatch,
}

impl Default for CharacterControllerState {
    fn default() -> Self {
        Self {
            base_velocity: Vec3::ZERO,
            // late initialized
            standing_collider: default(),
            crouching_collider: default(),
            grounded: None,
            crouching: false,
            touching_entities: Vec::new(),
            last_ground: max_stopwatch(),
            last_step_up: max_stopwatch(),
            last_step_down: max_stopwatch(),
        }
    }
}

fn max_stopwatch() -> Stopwatch {
    let mut watch = Stopwatch::new();
    watch.set_elapsed(Duration::MAX);
    watch
}

impl CharacterControllerState {
    pub fn collider(&self) -> &Collider {
        if self.crouching {
            &self.crouching_collider
        } else {
            &self.standing_collider
        }
    }
}

/// Data related to a hit during [`MoveAndSlide::move_and_slide`].
#[derive(Clone, Reflect, PartialEq, Debug)]
pub struct TouchingEntity {
    /// The entity of the collider that was hit by the shape.
    pub entity: Entity,

    /// The maximum distance that is safe to move in the given direction so that the collider
    /// still keeps a distance of `skin_width` to the other colliders.
    ///
    /// This is `0.0` when any of the following is true:
    ///
    /// - The collider started off intersecting another collider.
    /// - The collider is moving toward another collider that is already closer than `skin_width`.
    ///
    /// If you want to know the real distance to the next collision, use [`Self::collision_distance`].
    pub distance: f32,

    /// The hit point on the shape that was hit, expressed in world space.
    pub point: Vec3,

    /// The outward surface normal on the hit shape at `point`, expressed in world space.
    pub normal: Dir3,

    /// The position of the collider at the time of the move and slide iteration.
    pub character_position: Vec3,

    /// The velocity of the collider at the time of the move and slide iteration.
    pub character_velocity: Vec3,

    /// The raw distance to the next collision, not respecting skin width.
    /// To move the shape, use [`Self::distance`] instead.
    #[doc(alias = "time_of_impact")]
    pub collision_distance: f32,
}
impl From<MoveAndSlideHitData<'_>> for TouchingEntity {
    fn from(value: MoveAndSlideHitData<'_>) -> Self {
        Self {
            entity: value.entity,
            distance: value.distance,
            point: value.point,
            normal: *value.normal,
            character_position: *value.position,
            character_velocity: *value.velocity,
            collision_distance: value.collision_distance,
        }
    }
}
