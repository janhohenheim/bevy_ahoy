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
        AhoyPlugins, AhoySystems, AirJumping, CharacterController, CharacterControllerState,
        Dashing, GravityScaling, WallRunning,
        camera::{CharacterControllerCamera, CharacterControllerCameraOf},
        input::{
            Climbdown, Crane, Crouch, Dash, GlobalMovement, Jump, Mantle, Movement, RotateCamera,
            SwimUp, Tac, YankCamera,
        },
        water::{Water, WaterLevel, WaterState},
    };

    #[cfg(feature = "pickup")]
    pub use crate::{
        PickupConfig,
        input::{DropObject, PullObject, ThrowObject},
        pickup,
    };
}

#[cfg(feature = "pickup")]
pub use crate::pickup_glue::AhoyPickupGluePlugin;
#[cfg(feature = "pickup")]
use avian_pickup::AvianPickupPlugin;
#[cfg(feature = "pickup")]
pub use avian_pickup::{
    self as pickup,
    prelude::{
        AvianPickupActor as PickupConfig, AvianPickupActorHoldConfig as PickupHoldConfig,
        AvianPickupActorPullConfig as PickupPullConfig,
        AvianPickupActorThrowConfig as PickupThrowConfig,
    },
};

pub use crate::{
    camera::AhoyCameraPlugin, dynamics::AhoyDynamicPlugin,
    fixed_update_utils::AhoyFixedUpdateUtilsPlugin, input::AhoyInputPlugin, kcc::AhoyKccPlugin,
    water::AhoyWaterPlugin,
};
use crate::{input::AccumulatedInput, prelude::*};
use avian3d::character_controller::move_and_slide::MoveHitData;
use bevy_app::PluginGroupBuilder;
use bevy_ecs::{entity::MapEntities, intern::Interned, schedule::ScheduleLabel};
use bevy_time::Stopwatch;
use core::time::Duration;

pub mod camera;
mod dynamics;
mod fixed_update_utils;
pub mod input;
mod kcc;
#[cfg(feature = "pickup")]
mod pickup_glue;
mod water;

/// Plugin group for Ahoy's internal plugins.
///
/// It requires you to add [`PhysicsPlugins`] and [`EnhancedInputPlugin`] to work properly.
///
/// Also adds [`AvianPickupPlugin`] if `pickup` feature is enabled.
pub struct AhoyPlugins {
    schedule: Interned<dyn ScheduleLabel>,
}

impl AhoyPlugins {
    /// Create a new plugin group in the given schedule. The default is [`FixedPostUpdate`].
    pub fn new(schedule: impl ScheduleLabel) -> Self {
        Self {
            schedule: schedule.intern(),
        }
    }
}

impl Default for AhoyPlugins {
    fn default() -> Self {
        Self {
            schedule: FixedPostUpdate.intern(),
        }
    }
}

impl PluginGroup for AhoyPlugins {
    fn build(self) -> PluginGroupBuilder {
        let plugin = PluginGroupBuilder::start::<Self>()
            .add(AhoySchedulePlugin {
                schedule: self.schedule,
            })
            .add(AhoyCameraPlugin)
            .add(AhoyInputPlugin)
            .add(AhoyKccPlugin {
                schedule: self.schedule,
            })
            .add(AhoyWaterPlugin)
            .add(AhoyFixedUpdateUtilsPlugin)
            .add(AhoyDynamicPlugin {
                schedule: self.schedule,
            });

        #[cfg(feature = "pickup")]
        let plugin = plugin
            .add(AvianPickupPlugin::default())
            .add(AhoyPickupGluePlugin);

        plugin
    }
}

/// Plugin to setup schedule for [`AhoySystems`].
pub struct AhoySchedulePlugin {
    pub schedule: Interned<dyn ScheduleLabel>,
}

impl Plugin for AhoySchedulePlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            self.schedule,
            (
                AhoySystems::MoveCharacters,
                AhoySystems::ApplyForcesToDynamicRigidBodies,
            )
                .chain()
                .before(PhysicsSystems::First),
        );
    }
}

/// System set used by all systems of `bevy_ahoy`.
#[derive(SystemSet, Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum AhoySystems {
    MoveCharacters,
    ApplyForcesToDynamicRigidBodies,
}

#[derive(Component, Clone, Reflect, PartialEq, Debug)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize", reflect(Serialize, Deserialize))]
#[reflect(Component)]
#[require(
    AccumulatedInput,
    CharacterControllerState,
    CharacterControllerDerivedProps,
    CharacterControllerOutput,
    TranslationInterpolation,
    RigidBody = RigidBody::Kinematic,
    WaterState,
    CustomPositionIntegration,
    Transform,
    SpeculativeMargin::ZERO,
    CollidingEntities,
)]
pub struct CharacterController {
    pub crouch_height: f32,
    pub filter: SpatialQueryFilter,
    pub standing_view_height: f32,
    pub crouch_view_height: f32,
    pub ground_distance: f32,
    pub step_down_detection_distance: f32,
    pub min_walk_cos: f32,
    pub stop_speed: f32,
    pub air_friction: Friction,
    pub friction_hz: f32,
    pub acceleration_hz: f32,
    pub air_acceleration_hz: f32,
    pub water_acceleration_hz: f32,
    pub water_slowdown: f32,
    /// Downward acceleration in m/s². Jump heights stay the same when this changes, only airtime does;
    /// [`GravityScaling`] scales do change them.
    pub gravity: f32,
    pub water_gravity: f32,
    pub step_size: f32,
    pub crane_height: f32,
    pub crouch_speed_scale: f32,
    pub speed: f32,
    pub air_speed: f32,
    pub move_and_slide: MoveAndSlideConfig,
    pub max_speed: f32,
    /// Peak height of a ground jump in meters, from where the feet left the ground.
    /// Exact only without [`GravityScaling`]; e.g. a low `apex_scale` overshoots it.
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
    pub climbdown_input_buffer: Duration,
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
    pub climb_reverse_sin: f32,
    pub climb_sensitivity: f32,
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
            air_friction: Friction::default(),
            friction_hz: 12.0,
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
            climbdown_input_buffer: Duration::from_millis(150),
            crane_height: 1.5,
            mantle_height: 1.0,
            crane_speed: 11.0,
            mantle_speed: 5.0,
            min_ledge_grab_space: Cuboid::new(0.2, 0.1, 0.2),
            climb_pull_up_height: 0.3,
            max_ledge_grab_distance: 0.3,
            climb_reverse_sin: 40.0_f32.to_radians().sin(),
            climb_sensitivity: 2.5,
        }
    }
}

/// Add alongside [`CharacterController`] to allow jumping in the air.
#[derive(Component, Clone, Reflect, PartialEq, Debug)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize", reflect(Serialize, Deserialize))]
#[reflect(Component)]
#[require(AirJumpState)]
pub struct AirJumping {
    /// Jumps allowed before landing. Touching a wall while [`WallRunning`] refills them.
    pub count: u8,
    /// Height gained in meters, from where the jump happens; lower than [`CharacterController::jump_height`]
    /// makes air jumps feel distinct. Exact only without [`GravityScaling`].
    pub height: f32,
}

impl Default for AirJumping {
    fn default() -> Self {
        Self {
            count: 1,
            height: 1.2,
        }
    }
}

/// Add alongside [`CharacterController`] to scale gravity over the airborne arc, e.g. to hang at the apex.
/// Scales also change jump height, since jump speed is derived from unscaled gravity.
#[derive(Component, Clone, Reflect, PartialEq, Debug)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize", reflect(Serialize, Deserialize))]
#[reflect(Component)]
pub struct GravityScaling {
    /// Gravity multiplier while going up. Above 1 gives lower, snappier jumps; below 1 higher, floatier ones.
    pub rise_scale: f32,
    /// Gravity multiplier near the peak. Below 1 hangs in the air there; 1 disables the hang.
    pub apex_scale: f32,
    /// Vertical speed in m/s under which you count as at the peak. Wider means a longer hang.
    pub apex_threshold: f32,
    /// Gravity multiplier while falling. Above 1 drops faster; it doesn't change peak height.
    pub fall_scale: f32,
}

impl Default for GravityScaling {
    fn default() -> Self {
        Self {
            rise_scale: 1.0,
            apex_scale: 0.4,
            apex_threshold: 3.0,
            fall_scale: 1.0,
        }
    }
}

/// Add alongside [`CharacterController`] to allow running along walls.
#[derive(Component, Clone, Reflect, PartialEq, Debug)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize", reflect(Serialize, Deserialize))]
#[reflect(Component)]
#[require(WallRunState)]
pub struct WallRunning {
    /// Horizontal speed along the wall in m/s needed to start and keep running.
    /// Above [`CharacterController::speed`], only extra momentum (dashes, bhops) gets you on walls.
    pub min_speed: f32,
    /// Longest a single run lasts before you drop off.
    pub max_duration: Duration,
    /// Multiplier on [`CharacterController::gravity`] while running.
    /// `0.0` runs perfectly level, `1.0` falls as fast as normal.
    pub gravity_scale: f32,
    /// How close in meters a wall must be to start or keep running on it.
    pub detection_distance: f32,
    /// Horizontal speed in m/s away from the wall when jumping off.
    pub jump_push: f32,
    /// Height gained in meters when jumping off. Exact only without [`GravityScaling`].
    pub jump_height: f32,
    /// Time after leaving a wall (or tic-tacing) before another run can start.
    pub reattach_cooldown: Duration,
}

impl Default for WallRunning {
    fn default() -> Self {
        Self {
            min_speed: 6.0,
            max_duration: Duration::from_millis(1500),
            gravity_scale: 0.15,
            detection_distance: 0.3,
            jump_push: 8.0,
            jump_height: 1.5,
            reattach_cooldown: Duration::from_millis(300),
        }
    }
}

/// Add alongside [`CharacterController`] to allow dashing.
#[derive(Component, Clone, Reflect, PartialEq, Debug)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize", reflect(Serialize, Deserialize))]
#[reflect(Component)]
#[require(DashState)]
pub struct Dashing {
    /// Meters covered by a dash that isn't blocked or cancelled.
    pub distance: f32,
    /// How long a dash takes. Together with `distance` this sets the speed, see [`Self::speed`].
    pub duration: Duration,
    /// Time from the start of one dash until the next can begin.
    pub cooldown: Duration,
    /// How early a dash press counts, if it can't happen yet (e.g. still on cooldown).
    pub input_buffer: Duration,
    /// Dashes allowed in the air before landing. Ground dashes are unlimited, only gated by `cooldown`.
    pub max_air_dashes: u8,
}

impl Dashing {
    /// Dash speed in m/s.
    pub fn speed(&self) -> f32 {
        self.distance / self.duration.as_secs_f32().max(f32::EPSILON)
    }
}

impl Default for Dashing {
    fn default() -> Self {
        Self {
            distance: 8.0,
            duration: Duration::from_millis(200),
            cooldown: Duration::from_millis(600),
            input_buffer: Duration::from_millis(150),
            max_air_dashes: 1,
        }
    }
}

/// The look direction for the character.
///
/// Usually, this is populated by the camera.
#[derive(Component, Clone, Reflect, PartialEq, Debug, Default, MapEntities)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize", reflect(Serialize, Deserialize))]
#[reflect(Component)]
pub struct CharacterLook {
    /// The yaw the character is looking at (relative to the world).
    pub yaw: f32,
    /// The pitch the character is looking at (relative to the character).
    ///
    /// For normal movement, this is not necessary, but for actions like swimming, this affects the
    /// direction the character will swim in.
    pub pitch: f32,
}

impl CharacterLook {
    /// Converts a quaternion into the corresponding look values.
    ///
    /// Usually this `quat` comes from the camera.
    pub fn from_quat(quat: Quat) -> Self {
        let (yaw, pitch, _) = quat.to_euler(EulerRot::YXZ);
        Self { yaw, pitch }
    }

    /// Applies this look direction to `quat`.
    ///
    /// This preserves the roll of `quat`.
    pub fn apply_to_quat(&self, quat: &mut Quat) {
        let (_, _, roll) = quat.to_euler(EulerRot::YXZ);
        *quat = Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, roll);
    }

    /// Creates a [`Quat`] corresponding to this look direction.
    ///
    /// Unlike [`Self::apply_to_quat`], this does not mutate an existing [`Quat`], so there's no
    /// roll to preserve.
    pub fn to_quat(&self) -> Quat {
        Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0)
    }
}

#[derive(Component, Clone, Reflect, PartialEq, Debug)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize", reflect(Serialize, Deserialize))]
#[reflect(Component)]
pub struct CharacterControllerState {
    pub orientation: Quat,
    /// The velocity of the platform that the character is standing on (or has recently jumped off
    /// of).
    pub platform_velocity: Vec3,
    /// The angular velocity of the platform that the character is standing on (or has recently
    /// jumped off of).
    pub platform_angular_velocity: Vec3,
    pub grounded: Option<MoveHitData>,
    pub crouching: bool,
    pub tac_velocity: f32,
    pub last_ground: Stopwatch,
    pub last_tac: Stopwatch,
    pub last_step_up: Stopwatch,
    pub last_step_down: Stopwatch,
    pub crane_height_left: Option<f32>,
    /// The current state of the mantle, if a mantle is in progress.
    ///
    /// This is [`None`] if a mantle is not in progress.
    pub mantle: Option<MantleState>,
}

impl Default for CharacterControllerState {
    fn default() -> Self {
        Self {
            platform_velocity: Vec3::ZERO,
            platform_angular_velocity: Vec3::ZERO,
            orientation: Quat::IDENTITY,
            grounded: None,
            crouching: false,
            tac_velocity: 0.0,
            last_ground: max_stopwatch(),
            last_tac: max_stopwatch(),
            last_step_up: max_stopwatch(),
            last_step_down: max_stopwatch(),
            crane_height_left: None,
            mantle: None,
        }
    }
}

/// The state of a mantle in progress.
#[derive(Clone, Reflect, PartialEq, Debug)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub struct MantleState {
    pub height_left: f32,
}

#[derive(Component, Clone, Reflect, PartialEq, Debug, Default)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize", reflect(Serialize, Deserialize))]
#[reflect(Component)]
pub struct AirJumpState {
    pub used: u8,
}

#[derive(Component, Clone, Reflect, PartialEq, Debug)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize", reflect(Serialize, Deserialize))]
#[reflect(Component)]
pub struct DashState {
    /// [`None`] if a dash is not in progress.
    pub progress: Option<DashProgress>,
    pub last: Stopwatch,
    pub air_dashes_used: u8,
}

impl Default for DashState {
    fn default() -> Self {
        Self {
            progress: None,
            last: max_stopwatch(),
            air_dashes_used: 0,
        }
    }
}

#[derive(Component, Clone, Reflect, PartialEq, Debug)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize", reflect(Serialize, Deserialize))]
#[reflect(Component)]
pub struct WallRunState {
    /// [`None`] if a wall run is not in progress.
    pub progress: Option<WallRunProgress>,
    /// Time since the last wall run ended.
    pub last: Stopwatch,
    /// Set when a run times out, so we don't immediately reattach. Cleared on landing or wall jump.
    pub exhausted: bool,
}

impl Default for WallRunState {
    fn default() -> Self {
        Self {
            progress: None,
            last: max_stopwatch(),
            exhausted: false,
        }
    }
}

/// The state of a wall run in progress.
#[derive(Clone, Reflect, PartialEq, Debug)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub struct WallRunProgress {
    pub normal: Dir3,
    pub elapsed: Duration,
}

/// The state of a dash in progress.
#[derive(Clone, Reflect, PartialEq, Debug)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub struct DashProgress {
    pub direction: Dir3,
    pub time_left: Duration,
}

fn max_stopwatch() -> Stopwatch {
    let mut watch = Stopwatch::new();
    watch.set_elapsed(Duration::MAX);
    watch
}

/// Properties derived for a [`CharacterController`] that are constant for a character.
#[derive(Component, Clone, Debug, Default)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub struct CharacterControllerDerivedProps {
    /// The collider for the primary movement used when the character is standing.
    pub standing_collider: Collider,
    /// The collider for the primary movement used when the character is crouching.
    pub crouching_collider: Collider,
    /// The collider representing the hands for mantling.
    pub hand_collider: Collider,
}

impl CharacterControllerDerivedProps {
    pub fn collider(&self, state: &CharacterControllerState) -> &Collider {
        if state.crouching {
            &self.crouching_collider
        } else {
            &self.standing_collider
        }
    }

    pub fn pos_to_head_dist(&self, state: &CharacterControllerState) -> f32 {
        self.collider(state)
            .shape_scaled()
            .compute_local_aabb()
            .maxs
            .y
    }

    pub fn pos_to_feet_dist(&self, state: &CharacterControllerState) -> f32 {
        self.collider(state)
            .shape_scaled()
            .compute_local_aabb()
            .mins
            .y
    }

    pub fn radius(&self, state: &CharacterControllerState) -> f32 {
        match self.collider(state).shape_scaled().as_typed_shape() {
            avian3d::parry::shape::TypedShape::Ball(ball) => ball.radius,
            avian3d::parry::shape::TypedShape::Cuboid(cuboid) => cuboid.half_extents.max_element(),
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
                round_shape.border_radius + round_shape.inner_shape.half_extents.max_element()
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

/// Properties computed during movement useful for gameplay systems.
///
/// Note this only includes results that are "transient" for a frame (or in other words, is
/// exclusively an output). For example, while "crouching" is technically a result of movement, it
/// is also used as input in the next frame.
#[derive(Component, Reflect, PartialEq, Debug, Default)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize", reflect(Serialize, Deserialize))]
#[reflect(Component)]
pub struct CharacterControllerOutput {
    /// Details about an in progress mantle.
    ///
    /// This is [`None`] if a mantle is not in progress.
    pub mantle: Option<MantleOutput>,
    /// The entities this character is touching.
    pub touching_entities: Vec<TouchingEntity>,
}

/// Properties computing while mantling.
///
/// These are properties about the mantle that are transient and are not needed for future updates.
#[derive(Clone, Reflect, PartialEq, Debug)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize", reflect(Serialize, Deserialize))]
pub struct MantleOutput {
    /// The normal of the wall being mantled.
    pub wall_normal: Dir3,
    /// The position of the ledge on the wall.
    pub ledge_position: Vec3,
    /// The wall that is being mantled.
    pub wall_entity: Entity,
}

/// Data related to a hit during [`MoveAndSlide::move_and_slide`].
#[derive(Clone, Reflect, PartialEq, Debug)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize", reflect(Serialize, Deserialize))]
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
