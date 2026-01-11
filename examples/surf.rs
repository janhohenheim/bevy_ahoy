use crate::util::ExampleUtilPlugin;
use avian3d::prelude::*;
use bevy::{
    color::palettes::tailwind,
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    gltf::GltfPlugin,
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    input::common_conditions::input_just_pressed,
    prelude::*,
    window::{CursorGrabMode, CursorOptions, WindowResolution},
};
use bevy_ahoy::prelude::*;
use bevy_enhanced_input::prelude::*;
use bevy_time::Stopwatch;
use bevy_trenchbroom::prelude::*;
use bevy_trenchbroom_avian::AvianPhysicsBackend;

mod util;

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins
                .set(GltfPlugin {
                    use_model_forward_direction: true,
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
                })
                .set(WindowPlugin {
                    primary_window: Window {
                        #[cfg(target_arch = "wasm32")]
                        resolution: WindowResolution::new(1280, 720),
                        #[cfg(not(target_arch = "wasm32"))]
                        resolution: WindowResolution::new(1920, 1080),
                        fit_canvas_to_parent: true,
                        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "macos")))]
                        present_mode: bevy::window::PresentMode::Mailbox,
                        ..default()
                    }
                    .into(),
                    ..default()
                }),
            PhysicsPlugins::default(),
            EnhancedInputPlugin,
            AhoyPlugin::default(),
            TrenchBroomPlugins(
                TrenchBroomConfig::new("bevy_ahoy_surf")
                    .default_solid_scene_hooks(|| {
                        SceneHooks::new()
                            .convex_collider()
                            .smooth_by_default_angle()
                    })
                    .auto_remove_textures(
                        [
                            "clip",
                            "skip",
                            "__TB_empty",
                            "utopia/nodraw",
                            "tools/tool_trigger",
                        ]
                        .into_iter()
                        .map(String::from)
                        .collect::<std::collections::HashSet<_>>(),
                    )
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
        .insert_resource(ClearColor(tailwind::SKY_200.into()))
        .init_resource::<Checkpoint>()
        .add_systems(Startup, (setup, setup_velocity_text))
        .add_observer(spawn_player)
        .add_observer(setup_time)
        .add_observer(reset_time)
        .add_observer(save_checkpoint)
        .add_observer(load_checkpoint)
        .add_systems(
            Update,
            (
                capture_cursor.run_if(input_just_pressed(MouseButton::Left)),
                release_cursor.run_if(input_just_pressed(KeyCode::Escape)),
                update_time,
                update_velocity_text,
            ),
        )
        .run()
}

fn setup(mut commands: Commands, assets: Res<AssetServer>) {
    commands.spawn(SceneRoot(assets.load("maps/utopia.map#Scene")));
    commands.spawn(Camera3d::default());
}

#[derive(Component, Reflect, Debug)]
#[reflect(Component)]
#[require(
    PlayerInput,
    CharacterController {
        acceleration_hz: 10.0,
        air_acceleration_hz: 150.0,
        speed: 6.0,
        gravity: 23.0,
        friction_hz: 4.0,
        ..default()
    },
    RigidBody::Kinematic,
    Collider::cylinder(0.7, 1.8),
    CollisionLayers::new(
        [CollisionLayer::Player],
        LayerMask::ALL,
    )
)]
struct Player;

#[point_class(base(Transform, Visibility))]
#[reflect(Component)]
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
    let player = commands.spawn((Player, transform)).id();
    commands
        .entity(camera.into_inner())
        .insert(CharacterControllerCameraOf {
            yank_speed: 80.0_f32.to_radians(),
            ..CharacterControllerCameraOf::new(player)
        });
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
                    bindings![KeyCode::Space,  GamepadButton::South],
                ),
                (
                    Action::<Crouch>::new(),
                    bindings![KeyCode::ControlLeft, GamepadButton::LeftTrigger2],
                ),
                (
                    Action::<YankCamera>::new(),
                    bindings![MouseButton::Right]
                ),
                (
                    Action::<YankCamera>::new(),
                    Scale::splat(-1.0),
                    bindings![MouseButton::Left],
                ),
                (
                    Action::<RotateCamera>::new(),
                    Bindings::spawn((
                        Spawn((Binding::mouse_motion(), Scale::splat(0.05))),
                        Axial::right_stick().with((Scale::splat(4.0), DeadZone::default())),
                    ))
                ),
               (
                    Action::<SaveCheckpoint>::new(),
                    bindings![KeyCode::Digit1, GamepadButton::DPadDown],
                ),
                (
                    Action::<LoadCheckpoint>::new(),
                    bindings![KeyCode::Digit2, GamepadButton::DPadUp],
                ),
            ]));
    }
}

#[solid_class(base(Transform, Visibility), hooks(SceneHooks::new()))]
struct FuncIllusionary;

#[solid_class(base(Transform, Visibility))]
#[component(on_add = Self::on_add_prop)]
#[require(
    Sensor,
    CollisionEventsEnabled,
    CollisionLayers::new(
        [CollisionLayer::Sensor],
        [CollisionLayer::Player],
    )
)]
struct TriggerTeleport;

impl TriggerTeleport {
    fn on_add_prop(mut world: DeferredWorld, ctx: HookContext) {
        if world.is_scene_world() {
            return;
        }
        world.commands().spawn(
            Observer::new(
                |_: On<CollisionStart>,
                 mut commands: Commands,
                 reset: Single<Entity, With<Action<util::Reset>>>| {
                    commands
                        .entity(*reset)
                        .insert(ActionMock::once(ActionState::Fired, true));
                },
            )
            .with_entity(ctx.entity),
        );
    }
}

#[solid_class(base(Transform, Visibility))]
#[component(on_add = Self::on_add_prop)]
#[derive(Default)]
#[require(
    Sensor,
    CollisionEventsEnabled,
    CollisionLayers::new(
        [CollisionLayer::Sensor],
        [CollisionLayer::Player],
    )
)]
struct TriggerPush {
    speed: f32,
}

impl TriggerPush {
    fn on_add_prop(mut world: DeferredWorld, ctx: HookContext) {
        if world.is_scene_world() {
            return;
        }
        world.commands().spawn(
            Observer::new(
                |start: On<CollisionStart>,
                 push: Query<&TriggerPush>,
                 mut velocity: Single<&mut LinearVelocity, With<Player>>| {
                    let Ok(push) = push.get(start.collider1) else {
                        return;
                    };
                    let Ok((dir, vel)) = Dir3::new_and_length(velocity.0) else {
                        return;
                    };
                    velocity.0 = dir * (vel + push.speed);
                },
            )
            .with_entity(ctx.entity),
        );
    }
}

fn capture_cursor(mut cursor: Single<&mut CursorOptions>) {
    cursor.grab_mode = CursorGrabMode::Locked;
    cursor.visible = false;
}

fn release_cursor(mut cursor: Single<&mut CursorOptions>) {
    cursor.visible = true;
    cursor.grab_mode = CursorGrabMode::None;
}

#[derive(PhysicsLayer, Default)]
enum CollisionLayer {
    #[default]
    Default,
    Player,
    Sensor,
}

#[derive(Component, Default, Deref, DerefMut)]
struct TimeText(Stopwatch);

fn setup_time(_add: On<Add, Player>, mut commands: Commands) {
    commands.spawn((
        Node {
            justify_self: JustifySelf::Center,
            justify_content: JustifyContent::Center,
            top: px(20.0),
            ..default()
        },
        Text::new("Time: 00:00:000"),
        TimeText::default(),
    ));
}

fn update_time(mut time_texts: Query<(&mut Text, &mut TimeText)>, time: Res<Time>) {
    for (mut text, mut stopwatch) in time_texts.iter_mut() {
        stopwatch.tick(time.delta());
        text.0 = format!(
            "Time: {:02}:{:02}:{:03}",
            stopwatch.elapsed().as_secs() / 60,
            stopwatch.elapsed().as_secs() % 60,
            stopwatch.elapsed().as_millis() % 1000
        );
    }
}

fn reset_time(_reset: On<Fire<util::Reset>>, mut stopwatch: Single<&mut TimeText>) {
    stopwatch.reset();
}

fn setup_velocity_text(mut commands: Commands) {
    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        children![(
            Node {
                top: px(75.0),
                ..default()
            },
            Text::new("Loading... (this may take a while)"),
            TextColor(Color::WHITE.with_alpha(0.5)),
            VelocityText
        )],
    ));
}

#[derive(Component, Reflect, Debug)]
#[reflect(Component)]
pub(crate) struct VelocityText;

fn update_velocity_text(
    mut text: Single<&mut Text, With<VelocityText>>,
    velocity: Single<&LinearVelocity, With<CharacterController>>,
) {
    text.0 = format!("{:.3}", velocity.xz().length());
}

#[derive(Debug, InputAction)]
#[action_output(bool)]
pub struct SaveCheckpoint;

#[derive(Debug, InputAction)]
#[action_output(bool)]
pub struct LoadCheckpoint;

#[derive(Clone, Debug, Reflect, Default, Resource)]
#[reflect(Resource, Clone, Debug)]
struct Checkpoint {
    transform: Transform,
    velocity: LinearVelocity,
    state: CharacterControllerState,
    camera_rotation: Quat,
    time: Stopwatch,
}

impl Checkpoint {
    pub fn new(
        transform: Transform,
        velocity: LinearVelocity,
        state: CharacterControllerState,
        camera_rotation: Quat,
        time: Stopwatch,
    ) -> Self {
        Self {
            transform,
            velocity,
            state,
            camera_rotation,
            time,
        }
    }
}

fn save_checkpoint(
    _trigger: On<Start<SaveCheckpoint>>,
    mut cmd: Commands,
    player: Single<(&Transform, &LinearVelocity, &CharacterControllerState), With<Player>>,
    camera: Single<&Transform, (With<Camera3d>, Without<Player>)>,
    time: Single<&TimeText>,
) {
    let (tf, velocity, state) = *player;
    // Might wanna have a history instead of just the last one?
    cmd.insert_resource(Checkpoint::new(
        *tf,
        *velocity,
        state.clone(),
        camera.rotation,
        (**time).clone(),
    ));
    info!("Checkpoint saved!");
}

fn load_checkpoint(
    _trigger: On<Start<LoadCheckpoint>>,
    checkpoint: Option<Res<Checkpoint>>,
    player: Single<
        (
            &mut Transform,
            &mut LinearVelocity,
            &mut CharacterControllerState,
        ),
        With<Player>,
    >,
    mut camera: Single<&mut Transform, (With<Camera3d>, Without<Player>)>,
    mut time: Single<&mut TimeText>,
) {
    let Some(cp) = checkpoint else {
        return;
    };

    let (mut transform, mut velocity, mut state) = player.into_inner();

    *transform = cp.transform;
    *velocity = cp.velocity;
    *state = cp.state.clone();
    camera.rotation = cp.camera_rotation;

    // not sure if we wanna track time at all if checkpoints/practice systems are active?
    ***time = cp.time.clone();

    info!("Checkpoint loaded!");
}
