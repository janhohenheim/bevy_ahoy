use avian3d::{collision::collider::trimesh_builder::TrimeshBuilder, prelude::*};
use bevy::{
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    gltf::GltfPlugin,
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    input::common_conditions::input_just_pressed,
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};
use bevy_ahoy::{PickupHoldConfig, PickupPullConfig, prelude::*};
use bevy_enhanced_input::prelude::{Press, *};
use bevy_trenchbroom::prelude::*;
use bevy_trenchbroom_avian::AvianPhysicsBackend;
use core::ops::Deref;

use crate::util::ExampleUtilPlugin;
use std::time::Duration;

mod util;

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins
                .set(GltfPlugin {
                    use_model_forward_direction: true,
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Window {
                        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "macos")))]
                        present_mode: bevy::window::PresentMode::Mailbox,
                        ..default()
                    }
                    .into(),
                    ..default()
                })
                .set(ImagePlugin {
                    default_sampler: ImageSamplerDescriptor {
                        address_mode_u: ImageAddressMode::Repeat,
                        address_mode_v: ImageAddressMode::Repeat,
                        address_mode_w: ImageAddressMode::Repeat,
                        anisotropy_clamp: 16,
                        ..ImageSamplerDescriptor::linear()
                    },
                }),
            PhysicsPlugins::default(),
            EnhancedInputPlugin,
            AhoyPlugin::default(),
            TrenchBroomPlugins(
                TrenchBroomConfig::new("bevy_ahoy")
                    .default_solid_scene_hooks(|| {
                        SceneHooks::new()
                            .convex_collider()
                            .smooth_by_default_angle()
                    })
                    .texture_sampler(ImageSampler::Descriptor(ImageSamplerDescriptor {
                        address_mode_u: ImageAddressMode::Repeat,
                        address_mode_v: ImageAddressMode::Repeat,
                        address_mode_w: ImageAddressMode::Repeat,
                        anisotropy_clamp: 16,
                        ..ImageSamplerDescriptor::linear()
                    })),
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
        .add_systems(FixedUpdate, move_trains)
        .add_observer(spawn_player)
        // NPC Stuff
        .add_input_context::<Npc>()
        .add_systems(Startup, spawn_npc)
        .add_systems(Update, update_npc)
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
    insert: On<Add, SpawnPlayer>,
    players: Query<Entity, With<Player>>,
    spawner: Query<&Transform>,
    camera: Single<Entity, With<Camera3d>>,
    mut commands: Commands,
) {
    if !players.is_empty() {
        return;
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
        CharacterControllerCameraOf::new(player),
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
                    ActionSettings { consume_input: false, ..default() },
                    DeadZone::default(),
                    Bindings::spawn((
                        Cardinal::wasd_keys(),
                        Axial::left_stick()
                    ))
                ),
                (
                    Action::<Jump>::new(),
                    ActionSettings { consume_input: false, ..default() },
                    Press::default(),
                    bindings![
                        KeyCode::Space,
                        GamepadButton::South,
                        Binding::mouse_wheel(),
                    ],
                ),
                (
                    Action::<Tac>::new(),
                    ActionSettings { consume_input: false, ..default() },
                    Press::default(),
                    bindings![
                        KeyCode::Space,
                        GamepadButton::South,
                        Binding::mouse_wheel(),
                    ],
                ),
                (
                    Action::<Crane>::new(),
                    ActionSettings { consume_input: false, ..default() },
                    Press::default(),
                    bindings![
                        KeyCode::Space,
                        GamepadButton::South,
                        Binding::mouse_wheel(),
                    ],
                ),
                (
                    Action::<Mantle>::new(),
                    ActionSettings { consume_input: false, ..default() },
                    Hold::new(0.2),
                    bindings![
                        KeyCode::Space,
                        GamepadButton::South,
                    ],
                ),
                (
                    Action::<Crouch>::new(),
                    ActionSettings { consume_input: false, ..default() },
                    bindings![KeyCode::ControlLeft, GamepadButton::LeftTrigger],
                ),
                (
                    Action::<SwimUp>::new(),
                    ActionSettings { consume_input: false, ..default() },
                    bindings![KeyCode::Space, GamepadButton::South],
                ),
                (
                    Action::<PullObject>::new(),
                    ActionSettings { consume_input: false, ..default() },
                    Press::default(),
                    bindings![MouseButton::Right],
                ),
                (
                    Action::<DropObject>::new(),
                    ActionSettings { consume_input: false, ..default() },
                    Press::default(),
                    bindings![MouseButton::Right],
                ),
                (
                    Action::<ThrowObject>::new(),
                    ActionSettings { consume_input: false, ..default() },
                    Press::default(),
                    bindings![MouseButton::Left],
                ),
                (
                    Action::<RotateCamera>::new(),
                    ActionSettings { consume_input: false, ..default() },
                    Scale::splat(0.05),
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
            .with_default_density(300.0),
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

#[solid_class(base(Transform, Visibility))]
#[derive(Default)]
#[require(RigidBody::Kinematic, TransformInterpolation, GlobalTransform)]
struct FuncTrain {
    target: String,
    speed: f32,
    rotation: Vec3,
}

#[point_class(base(Transform, Visibility))]
#[derive(Default)]
#[require(GlobalTransform)]
struct PathCorner {
    #[class(must_set)]
    targetname: String,
    target: String,
}

fn move_trains(
    mut trains: Query<(
        &GlobalTransform,
        &mut LinearVelocity,
        &mut AngularVelocity,
        &mut FuncTrain,
    )>,
    corners: Query<(&GlobalTransform, &PathCorner)>,
) {
    for (train_transform, mut train_vel, mut train_ang_vel, mut train) in &mut trains {
        train_ang_vel.0 = train.rotation;
        if train.target.is_empty() {
            continue;
        }
        let Some((corner_transform, corner)) = corners
            .iter()
            .find(|(_, corner)| corner.targetname == train.target)
        else {
            error!("PathCorner not found for target: {}", train.target);
            continue;
        };
        if train_transform
            .translation()
            .distance_squared(corner_transform.translation())
            < 0.1
        {
            train.target = corner.target.clone();
            continue;
        }

        let to_corner = corner_transform.translation() - train_transform.translation();
        train_vel.0 = to_corner.normalize_or_zero() * train.speed;
    }
}

#[solid_class(base(Transform, Visibility))]
#[component(on_add = on_add_water)]
#[derive(Default)]
pub struct Water {
    speed: f32,
}

fn on_add_water(mut world: DeferredWorld, ctx: HookContext) {
    if world.is_scene_world() {
        return;
    }
    let speed = world.get::<Water>(ctx.entity).unwrap().speed;
    world
        .commands()
        .entity(ctx.entity)
        .insert(bevy_ahoy::prelude::Water { speed });
}

#[solid_class(base(Transform, Visibility))]
#[component(on_add = on_add_ice)]
#[derive(Default)]
pub struct Ice {
    friction: f32,
}

fn on_add_ice(mut world: DeferredWorld, ctx: HookContext) {
    if world.is_scene_world() {
        return;
    }
    let friction = world.get::<Ice>(ctx.entity).unwrap().friction;
    world
        .commands()
        .entity(ctx.entity)
        .insert(Friction::new(friction));
}

//NPC Stuff
const NPC_SPAWN_POINT: Vec3 = Vec3::new(-55.0, 45.0, 1.0);

#[derive(Component, Default)]
#[component(on_add = Npc::on_add)]
#[require(
    CharacterController,
    RigidBody::Kinematic,
    Collider::cylinder(0.7, 1.8),
    Mass(90.0)
)]
struct Npc {
    step: usize,
    timer: Timer,
}

impl Npc {
    fn on_add(mut world: DeferredWorld, ctx: HookContext) {
        let Some(collider) = world
            .get::<Collider>(ctx.entity)
            .map(|c| c.shape_scaled().clone())
        else {
            return;
        };
        let mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(TrimeshBuilder::new(collider).build().unwrap());
        let material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(Color::WHITE);
        world
            .commands()
            .entity(ctx.entity)
            .insert((Mesh3d(mesh), MeshMaterial3d(material)));
        world.commands().entity(ctx.entity).insert(actions!(Npc[
            (
                Action::<GlobalMovement>::new(),
                ActionMock {
                    state: ActionState::Fired,
                    value: Vec3::ZERO.into(),
                    span: Duration::from_secs(2).into(),
                    enabled: false
                }
            ),
            (
                Action::<Jump>::new(),
                ActionMock {
                    state: ActionState::Fired,
                    value: true.into(),
                    span: Duration::from_secs(2).into(),
                    enabled: false
                }
            ),
        ]));
    }
}

fn spawn_npc(mut commands: Commands) {
    commands.spawn((Npc::default(), Transform::from_translation(NPC_SPAWN_POINT)));
}

fn update_npc(
    time: Res<Time>,
    mut npcs: Query<(&mut Npc, &Actions<Npc>)>,
    mut action_mocks: Query<&mut ActionMock>,
    global_movements: Query<(), With<Action<GlobalMovement>>>,
    jumps: Query<(), With<Action<Jump>>>,
) {
    for (mut npc, actions) in &mut npcs {
        npc.timer.tick(time.delta());
        if npc.timer.is_finished() {
            if npc.timer.duration() != Duration::ZERO {
                npc.step += 1;
            }
            //brain durations
            let duration = match npc.step {
                0..=4 => 1.0,
                5 | 7 | 9 => 0.2,
                6 | 8 | 10 => 0.8,
                _ => {
                    npc.step = 0;
                    1.0
                }
            };
            npc.timer.set_duration(Duration::from_secs_f32(duration));
            npc.timer.reset();
        }

        //brain sequences : front, back, left, right, stop, jump*3
        let (move_vec, jump) = match npc.step {
            0 => (Vec3::NEG_Z, false),
            1 => (Vec3::Z, false),
            2 => (Vec3::NEG_X, false),
            3 => (Vec3::X, false),
            5..=9 => (Vec3::ZERO, true),
            _ => (Vec3::ZERO, false),
        };

        for action_entity in actions {
            if let Ok(mut mock) = action_mocks.get_mut(action_entity) {
                if global_movements.contains(action_entity) {
                    mock.enabled = move_vec != Vec3::ZERO;
                    if mock.enabled {
                        mock.value = move_vec.into();
                    }
                } else if jumps.contains(action_entity) {
                    mock.enabled = jump;
                }
            }
        }
    }
}
