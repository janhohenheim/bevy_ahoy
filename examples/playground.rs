use avian3d::prelude::*;
use bevy::{
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    gltf::GltfPlugin,
    input::common_conditions::input_just_pressed,
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};
use bevy_ahoy::{PickupHoldConfig, PickupPullConfig, PickupThrowConfig, prelude::*};
use bevy_enhanced_input::prelude::{Press, *};
use bevy_trenchbroom::{physics::SceneCollidersReady, prelude::*};
use bevy_trenchbroom_avian::AvianPhysicsBackend;
use core::ops::Deref;

use crate::util::ExampleUtilPlugin;

mod util;

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins.set(GltfPlugin {
                use_model_forward_direction: true,
                ..default()
            }),
            PhysicsPlugins::default(),
            EnhancedInputPlugin,
            AhoyPlugin::default(),
            TrenchBroomPlugins(
                TrenchBroomConfig::new("bevy_ahoy").default_solid_scene_hooks(|| {
                    SceneHooks::new()
                        .convex_collider()
                        .smooth_by_default_angle()
                }),
            ),
            TrenchBroomPhysicsPlugin::new(AvianPhysicsBackend),
            ExampleUtilPlugin,
        ))
        .add_input_context::<PlayerInput>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                capture_cursor.run_if(input_just_pressed(MouseButton::Left)),
                release_cursor.run_if(input_just_pressed(KeyCode::Escape)),
            ),
        )
        .add_observer(spawn_player)
        .run()
}

fn setup(mut commands: Commands, assets: Res<AssetServer>) {
    commands.spawn(SceneRoot(assets.load("maps/playground.map#Scene")));
    commands.spawn(Camera3d::default());
}

#[derive(Component, Reflect, Debug)]
#[reflect(Component)]
struct Player;

#[point_class(base(Transform, Visibility))]
struct SpawnPlayer;

fn spawn_player(
    insert: On<Insert, SpawnPlayer>,
    players: Query<Entity, With<Player>>,
    spawner: Query<&Transform>,
    camera: Single<Entity, With<Camera3d>>,
    mut commands: Commands,
) {
    for player in players {
        // Respawn the player on hot-reloads
        commands.entity(player).despawn();
    }
    let Ok(transform) = spawner.get(insert.entity).copied() else {
        return;
    };
    let player = commands
        .spawn((
            Player,
            transform,
            CollisionLayers::new(CollisionLayer::Player, LayerMask::ALL),
            PlayerInput,
            CharacterController::default(),
            RigidBody::Kinematic,
            Collider::cylinder(0.7, 1.8),
            Mass(90.0),
        ))
        .id();
    commands.entity(camera.into_inner()).insert((
        CharacterControllerCameraOf(player),
        PickupConfig {
            prop_filter: SpatialQueryFilter::from_mask(CollisionLayer::Prop),
            actor_filter: SpatialQueryFilter::from_mask(CollisionLayer::Player),
            obstacle_filter: SpatialQueryFilter::from_mask(CollisionLayer::Default),
            hold: PickupHoldConfig {
                preferred_distance: 0.9,
                linear_velocity_easing: 0.8,
                ..default()
            },
            pull: PickupPullConfig {
                max_prop_mass: 1000.0,
                ..default()
            },
            ..default()
        },
    ));
}

#[derive(Component, Default)]
#[component(on_add = PlayerInput::on_add)]
pub(crate) struct PlayerInput;

impl PlayerInput {
    fn on_add(mut world: DeferredWorld, ctx: HookContext) {
        world
            .commands()
            .entity(ctx.entity)
            .insert(actions!(PlayerInput[
                (
                    Action::<Movement>::new(),
                    DeadZone::default(),
                    Bindings::spawn((
                        Cardinal::wasd_keys(),
                        Axial::left_stick()
                    ))
                ),
                (
                    Action::<Jump>::new(),
                    Press::default(),
                    bindings![
                        KeyCode::Space,
                        GamepadButton::South,
                        Binding::mouse_wheel(),
                    ],
                ),
                (
                    Action::<Crouch>::new(),
                    bindings![KeyCode::ControlLeft, GamepadButton::LeftTrigger],
                ),
                (
                    Action::<PullObject>::new(),
                    Press::default(),
                    bindings![MouseButton::Right],
                ),
                (
                    Action::<DropObject>::new(),
                    Press::default(),
                    bindings![MouseButton::Right],
                ),
                (
                    Action::<ThrowObject>::new(),
                    Press::default(),
                    bindings![MouseButton::Left],
                ),
                (
                    Action::<RotateCamera>::new(),
                    Scale::splat(0.04),
                    Bindings::spawn((
                        Spawn(Binding::mouse_motion()),
                        Axial::right_stick()
                    ))
                ),
            ]));
    }
}

#[point_class(base(Transform, Visibility), model("models/cube.glb"))]
#[component(on_add = on_add_prop::<Self>)]
#[derive(Default, Deref)]
struct Cube {
    dynamic: bool,
}

#[point_class(base(Transform, Visibility), model("models/cone.glb"))]
#[component(on_add = on_add_prop::<Self>)]
#[derive(Default, Deref)]
struct Cone {
    dynamic: bool,
}

#[point_class(base(Transform, Visibility), model("models/cylinder.glb"))]
#[component(on_add = on_add_prop::<Self>)]
#[derive(Default, Deref)]
struct Cylinder {
    dynamic: bool,
}

#[point_class(base(Transform, Visibility), model("models/capsule.glb"))]
#[component(on_add = on_add_prop::<Self>)]
#[derive(Default, Deref)]
struct Capsule {
    dynamic: bool,
}

#[point_class(base(Transform, Visibility), model("models/sphere.glb"))]
#[component(on_add = on_add_prop::<Self>)]
#[derive(Default, Deref)]
struct Sphere {
    dynamic: bool,
}

fn on_add_prop<T: QuakeClass + Deref<Target = bool>>(mut world: DeferredWorld, ctx: HookContext) {
    if world.is_scene_world() {
        return;
    }
    let dynamic = *world.get::<T>(ctx.entity).unwrap().deref();
    let assets = world.resource::<AssetServer>().clone();
    world.commands().entity(ctx.entity).insert((
        SceneRoot(
            assets.load(GltfAssetLabel::Scene(0).from_asset(T::CLASS_INFO.model_path().unwrap())),
        ),
        ColliderConstructorHierarchy::new(ColliderConstructor::ConvexHullFromMesh)
            .with_default_layers(CollisionLayers::new(CollisionLayer::Prop, LayerMask::ALL))
            .with_default_density(100.0),
        if dynamic {
            RigidBody::Dynamic
        } else {
            RigidBody::Static
        },
        TransformInterpolation,
    ));
}

fn capture_cursor(mut cursor: Single<&mut CursorOptions>) {
    cursor.grab_mode = CursorGrabMode::Locked;
    cursor.visible = false;
}

fn release_cursor(mut cursor: Single<&mut CursorOptions>) {
    cursor.visible = true;
    cursor.grab_mode = CursorGrabMode::None;
}

#[derive(Debug, PhysicsLayer, Default)]
enum CollisionLayer {
    #[default]
    Default,
    Player,
    Prop,
}
