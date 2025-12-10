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
        input::{
            Crane, Crouch, DropObject, Jump, Mantle, Movement, PullObject, RotateCamera, SwimUp,
            Tac, ThrowObject, YankCamera,
        },
        pickup,
        water::{Water, WaterLevel, WaterState},
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
mod water;

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
                .before(PhysicsSystems::First),
        )
        .add_plugins((
            camera::plugin,
            input::plugin,
            kcc::plugin(self.schedule),
            water::plugin,
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
    WaterState,
    CustomPositionIntegration,
    Transform,
    SpeculativeMargin::ZERO,
    CollidingEntities,
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
    pub water_acceleration_hz: f32,
    pub water_slowdown: f32,
    pub gravity: f32,
    pub water_gravity: f32,
    pub step_size: f32,
    pub crane_height: f32,
    pub crouch_speed_scale: f32,
    pub speed: f32,
    pub air_speed: f32,
    pub move_and_slide: MoveAndSlideConfig,
    pub max_speed: f32,
    pub jump_height: f32,
    pub tac_power: f32,
    pub tac_jump_factor: f32,
    pub tac_input_buffer: Duration,
    pub ledge_jump_power: f32,
    pub ledge_jump_factor: f32,
    pub max_tac_cos: f32,
    pub max_air_wish_speed: f32,
    pub tac_cooldown: Duration,
    pub unground_speed: f32,
    pub coyote_time: Duration,
    pub jump_input_buffer: Duration,
    pub jump_crane_chain_time: Duration,
    pub crane_input_buffer: Duration,
    pub mantle_input_buffer: Duration,
    pub min_step_ledge_space: f32,
    pub min_crane_ledge_space: f32,
    pub min_mantle_ledge_space: f32,
    pub mantle_height: f32,
    pub min_crane_cos: f32,
    pub min_mantle_cos: f32,
    pub crane_speed: f32,
    pub mantle_speed: f32,
    pub min_ledge_grab_space: Cuboid,
    pub climb_pull_up_height: f32,
    pub max_ledge_grab_distance: f32,
}

impl Default for CharacterController {
    fn default() -> Self {
        Self {
            crouch_height: 1.3,
            filter: SpatialQueryFilter::default(),
            standing_view_height: 1.7,
            crouch_view_height: 1.2,
            ground_distance: 0.05,
            min_walk_cos: 40.0_f32.to_radians().cos(),
            stop_speed: 2.54,
            friction_hz: 6.0,
            acceleration_hz: 8.0,
            air_acceleration_hz: 12.0,
            water_acceleration_hz: 12.0,
            water_slowdown: 0.6,
            gravity: 29.0,
            water_gravity: 2.4,
            step_size: 0.7,
            crouch_speed_scale: 1.0 / 3.0,
            speed: 12.0,
            air_speed: 1.5,
            move_and_slide: MoveAndSlideConfig {
                skin_width: 0.015,
                ..default()
            },
            max_speed: 100.0,
            jump_height: 1.8,
            tac_power: 0.755,
            tac_jump_factor: 1.0,
            ledge_jump_power: 1.5,
            ledge_jump_factor: 0.8,
            tac_input_buffer: Duration::from_millis(150),
            max_tac_cos: 40.0_f32.to_radians().cos(),
            max_air_wish_speed: 0.76,
            tac_cooldown: Duration::from_millis(300),
            unground_speed: 10.0,
            step_down_detection_distance: 0.2,
            min_crane_cos: 50.0_f32.to_radians().cos(),
            min_mantle_cos: 50.0_f32.to_radians().cos(),
            min_step_ledge_space: 0.2,
            min_crane_ledge_space: 0.35,
            min_mantle_ledge_space: 0.5,
            coyote_time: Duration::from_millis(100),
            jump_input_buffer: Duration::from_millis(150),
            jump_crane_chain_time: Duration::from_millis(140),
            crane_input_buffer: Duration::from_millis(200),
            mantle_input_buffer: Duration::from_millis(50),
            crane_height: 1.5,
            mantle_height: 1.0,
            crane_speed: 11.0,
            mantle_speed: 5.0,
            min_ledge_grab_space: Cuboid::new(0.2, 0.1, 0.2),
            climb_pull_up_height: 0.3,
            max_ledge_grab_distance: 0.3,
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

        let (crouch_height, min_ledge_grab_space) = {
            let Some(kcc) = world.get::<Self>(ctx.entity) else {
                return;
            };
            (kcc.crouch_height, kcc.min_ledge_grab_space)
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

        state.hand_collider = Collider::from(min_ledge_grab_space);
    }
}

#[derive(Component, Clone, Reflect, Debug)]
#[reflect(Component)]
pub struct CharacterControllerState {
    pub orientation: Transform,
    pub base_velocity: Vec3,
    #[reflect(ignore)]
    pub standing_collider: Collider,
    #[reflect(ignore)]
    pub crouching_collider: Collider,
    #[reflect(ignore)]
    pub hand_collider: Collider,
    pub grounded: Option<MoveHitData>,
    pub crouching: bool,
    pub tac_velocity: f32,
    pub touching_entities: Vec<TouchingEntity>,
    pub last_ground: Stopwatch,
    pub last_tac: Stopwatch,
    pub last_step_up: Stopwatch,
    pub last_step_down: Stopwatch,
    pub crane_height_left: Option<f32>,
    pub mantle_progress: Option<MantleProgress>,
}

impl Default for CharacterControllerState {
    fn default() -> Self {
        Self {
            base_velocity: Vec3::ZERO,
            orientation: Transform::IDENTITY,
            // late initialized
            standing_collider: default(),
            crouching_collider: default(),
            hand_collider: default(),
            grounded: None,
            crouching: false,
            tac_velocity: 0.0,
            touching_entities: Vec::new(),
            last_ground: max_stopwatch(),
            last_tac: max_stopwatch(),
            last_step_up: max_stopwatch(),
            last_step_down: max_stopwatch(),
            crane_height_left: None,
            mantle_progress: None,
        }
    }
}

#[derive(Clone, Copy, Reflect, Debug)]
pub struct MantleProgress {
    pub wall_normal: Dir3,
    pub ledge_position: Vec3,
    pub height_left: f32,
    pub wall_entity: Entity,
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

    pub fn pos_to_head_dist(&self) -> f32 {
        self.collider().shape_scaled().compute_local_aabb().maxs.y
    }

    pub fn pos_to_feet_dist(&self) -> f32 {
        self.collider().shape_scaled().compute_local_aabb().mins.y
    }

    pub fn radius(&self) -> f32 {
        match self.collider().shape_scaled().as_typed_shape() {
            avian3d::parry::shape::TypedShape::Ball(ball) => ball.radius,
            avian3d::parry::shape::TypedShape::Cuboid(cuboid) => cuboid.half_extents.max(),
            avian3d::parry::shape::TypedShape::Capsule(capsule) => capsule.radius,
            avian3d::parry::shape::TypedShape::Segment(segment) => segment.length() / 2.0,
            avian3d::parry::shape::TypedShape::Triangle(triangle) => triangle.circumcircle().1,
            avian3d::parry::shape::TypedShape::Voxels(voxels) => {
                voxels.local_bounding_sphere().radius()
            }
            avian3d::parry::shape::TypedShape::TriMesh(tri_mesh) => {
                tri_mesh.local_bounding_sphere().radius()
            }
            avian3d::parry::shape::TypedShape::Polyline(polyline) => {
                polyline.local_bounding_sphere().radius()
            }
            avian3d::parry::shape::TypedShape::HalfSpace(_half_space) => f32::INFINITY,
            avian3d::parry::shape::TypedShape::HeightField(height_field) => {
                height_field.local_bounding_sphere().radius()
            }
            avian3d::parry::shape::TypedShape::Compound(compound) => {
                compound.local_bounding_sphere().radius()
            }
            avian3d::parry::shape::TypedShape::ConvexPolyhedron(convex_polyhedron) => {
                convex_polyhedron.local_bounding_sphere().radius()
            }
            avian3d::parry::shape::TypedShape::Cylinder(cylinder) => cylinder.radius,
            avian3d::parry::shape::TypedShape::Cone(cone) => cone.radius,
            avian3d::parry::shape::TypedShape::RoundCuboid(round_shape) => {
                round_shape.border_radius + round_shape.inner_shape.half_extents.max()
            }
            avian3d::parry::shape::TypedShape::RoundTriangle(round_shape) => {
                round_shape.border_radius + round_shape.inner_shape.circumcircle().1
            }
            avian3d::parry::shape::TypedShape::RoundCylinder(round_shape) => {
                round_shape.border_radius + round_shape.inner_shape.radius
            }
            avian3d::parry::shape::TypedShape::RoundCone(round_shape) => {
                round_shape.border_radius + round_shape.inner_shape.radius
            }
            avian3d::parry::shape::TypedShape::RoundConvexPolyhedron(round_shape) => {
                round_shape.border_radius + round_shape.inner_shape.local_bounding_sphere().radius()
            }
            avian3d::parry::shape::TypedShape::Custom(shape) => {
                shape.compute_local_bounding_sphere().radius()
            }
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
