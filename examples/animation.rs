use crate::util::{CameraExt, EntityExt, ExampleUtilPlugin, StableGround};
use avian3d::prelude::*;
use bevy::{
    color::palettes::tailwind,
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    gltf::{GltfPlugin, convert_coordinates::GltfConvertCoordinates},
    image::{ImageAddressMode, ImageSamplerDescriptor},
    input::common_conditions::input_just_pressed,
    prelude::*,
    scene::SceneInstanceReady,
    window::{CursorGrabMode, CursorOptions, WindowResolution},
};
use bevy_ahoy::{CharacterLook, prelude::*};
use bevy_enhanced_input::prelude::*;
use bevy_third_person_camera::*;
use bevy_trenchbroom::prelude::*;
use bevy_trenchbroom_avian::AvianPhysicsBackend;
use std::time::Duration;

mod util;

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins
                .set(GltfPlugin {
                    convert_coordinates: GltfConvertCoordinates {
                        rotate_scene_entity: true,
                        rotate_meshes: true,
                    },
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
            TrenchBroomPlugins(
                TrenchBroomConfig::new("bevy_ahoy_animation")
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
                    ),
            ),
            TrenchBroomPhysicsPlugin::new(AvianPhysicsBackend),
            PhysicsPlugins::default(),
            EnhancedInputPlugin,
            AhoyPlugins::default(),
            ThirdPersonCameraPlugin,
            ExampleUtilPlugin,
        ))
        .add_input_context::<PlayerInput>()
        .insert_resource(ClearColor(tailwind::SKY_200.into()))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            prepare_animations
                .chain()
                .run_if(resource_exists::<PlayerModel>.and(run_once)),
        )
        .add_systems(
            Update,
            (rotate_player, calcucate_animations, animate).chain(),
        )
        .add_observer(spawn_player)
        .add_systems(
            Update,
            (
                capture_cursor.run_if(input_just_pressed(MouseButton::Left)),
                release_cursor.run_if(input_just_pressed(KeyCode::Escape)),
            ),
        )
        .run()
}

fn setup(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(PlayerModel(assets.load("models/ual.glb#Scene0")));
    commands.spawn(SceneRoot(assets.load("maps/utopia.map#Scene")));
    commands.spawn((
        Camera3d::default(),
        ThirdPersonCamera {
            zoom: Zoom::new(1.0, 30.0),
            cursor_lock_key: KeyCode::KeyL,
            ..default()
        },
    ));
}

#[derive(Component, Reflect, Debug, Default)]
#[reflect(Component)]
#[require(
    PlayerInput,
    CharacterController {
        acceleration_hz: 10.0,
        air_acceleration_hz: 150.0,
        speed: 100.0,
        jump_height: 5.0,
        gravity: 23.0,
        crouch_height: 2.0,
        friction_hz: 4.0,
        ..default()
    },
    RigidBody::Kinematic,
    Collider::cylinder(0.7, 1.8),
    CollisionLayers::new(
        [CollisionLayer::Player],
        LayerMask::ALL,
    ),
    StableGround::default(),
)]
struct Player {
    pub animation: Animations,
}

#[derive(Component, Default, Deref)]
pub struct PreviousPosition(pub Vec3);

#[derive(Resource)]
pub struct PlayerModel(Handle<Gltf>);

#[point_class(base(Transform, Visibility))]
#[reflect(Component)]
struct SpawnPlayer;

fn spawn_player(
    insert: On<Insert, SpawnPlayer>,
    players: Query<Entity, With<Player>>,
    player_model: Res<PlayerModel>,
    spawner: Query<&Transform>,
    gltfs: Res<Assets<Gltf>>,
    mut commands: Commands,
) {
    for player in players {
        // Respawn the player on hot-reloads
        commands.entity(player).despawn();
    }
    let Ok(transform) = spawner.get(insert.entity).copied() else {
        return;
    };

    let Some(model) = gltfs.get(&player_model.0) else {
        return;
    };

    commands
        .spawn((
            Player::default(),
            transform,
            PreviousPosition(transform.translation),
            ThirdPersonCameraTarget,
        ))
        .with_children(|parent| {
            parent.spawn((
                SceneRoot(model.scenes[0].clone()),
                Transform::from_xyz(0.0, -1.0, 0.0),
            ));
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
                    Action::<RotateCamera>::new(),
                    Bindings::spawn((
                        Spawn((Binding::mouse_motion(), Scale::splat(0.07))),
                        Axial::right_stick().with((Scale::splat(4.0), DeadZone::default())),
                    )),
                ),
                (
                    Action::<Jump>::new(),
                    bindings![KeyCode::Space,  GamepadButton::South],
                ),
                (
                    Action::<Crouch>::new(),
                    bindings![KeyCode::ControlLeft, GamepadButton::LeftTrigger2],
                ),
            ]));
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
}

// CONTROLS
fn rotate_player(
    movement: Single<&Action<Movement>>,
    camera: Single<&Transform, With<Camera3d>>,
    mut player_q: Query<(&mut Transform, &mut CharacterLook), Without<Camera3d>>,
) {
    let movement = *movement.into_inner();

    for (mut pos, mut look) in player_q.iter_mut() {
        let input_dir = camera.movement_direction(*movement);

        if input_dir.length_squared() > 0.01 {
            // set ahoy KCC direction
            let (yaw, pitch, _) = camera.rotation.to_euler(EulerRot::YXZ);
            *look = CharacterLook { yaw, pitch };

            // rotate model
            let rotation = Quat::from_rotation_y(input_dir.x.atan2(input_dir.z));
            pos.rotation = pos.rotation.slerp(rotation, 0.2);
        }
    }
}

// ANIMATION

/// Build animation graph when scene loads
fn prepare_animations(
    gltfs: Res<Assets<Gltf>>,
    children_q: Query<&Children>,
    animation_player_q: Query<Entity, With<AnimationPlayer>>,
    player_model: Res<PlayerModel>,
    mut animation_players: Query<&mut AnimationPlayer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut players: Query<(Entity, &mut Player)>,
    mut commands: Commands,
) {
    for (e, mut player) in &mut players {
        let Some(animation_player_e) = e.get_recursive(children_q, animation_player_q) else {
            return;
        };
        let Ok(mut animation_player) = animation_players.get_mut(animation_player_e) else {
            return;
        };
        let Some(gltf) = gltfs.get(&player_model.0) else {
            info!("no gltf");
            return;
        };

        // we list acnimations here in the same order they are listed in AnimationState enum
        let clips = vec![
            gltf.named_animations["Idle_Loop"].clone(),
            gltf.named_animations["Jog_Fwd_Loop"].clone(),
            gltf.named_animations["Sprint_Loop"].clone(),
            gltf.named_animations["Jump_Start"].clone(),
            gltf.named_animations["Jump_Loop"].clone(),
            gltf.named_animations["Jump_Land"].clone(),
            gltf.named_animations["Crouch_Fwd_Loop"].clone(),
            gltf.named_animations["Crouch_Idle_Loop"].clone(),
            gltf.named_animations["Roll"].clone(),
        ];

        let (graph, nodes) = AnimationGraph::from_clips(clips);
        let graph_handle = graphs.add(graph);

        commands
            .entity(animation_player_e)
            .insert(AnimationGraphHandle(graph_handle))
            .insert(AnimationTransitions::default());

        let idle_node = nodes[0];
        animation_player.play(idle_node).repeat();

        player.animation = Animations {
            current: AnimationState::StandIdle,
            requested: None,
            nodes,
            animation_player_e,
        };
    }
}

fn calcucate_animations(
    time: Res<Time>,
    mut players: Query<(
        &CharacterControllerState,
        &Transform,
        &mut PreviousPosition,
        &mut Player,
    )>,
) {
    const IDLE_ANIMATION_THRESHOLD: f32 = 0.5;

    for (ahoy_state, pos, mut prev_pos, mut player) in players.iter_mut() {
        let animation = &mut player.animation;

        let displacement = pos.translation - prev_pos.0;
        let velocity = displacement / time.delta_secs();
        let horizontal_speed = Vec3::new(velocity.x, 0.0, velocity.z).length().abs();
        let _vertical_speed = velocity.y;
        prev_pos.0 = pos.translation;

        let grounded = ahoy_state.grounded.is_some();

        // MANTLE
        if ahoy_state.mantle.is_some() {
            // TODO: mantle
            continue;
        }

        // in the air animation
        if !grounded {
            animation.request(AnimationState::Jump);
            continue;
        }

        // at this point we are grounded
        if grounded && animation.current.is_jumping() {
            if horizontal_speed > IDLE_ANIMATION_THRESHOLD {
                animation.request(AnimationState::Run(horizontal_speed));
            } else {
                animation.request(AnimationState::Jump);
            }
            continue;
        }

        // CROUCH
        if ahoy_state.crouching {
            if horizontal_speed > IDLE_ANIMATION_THRESHOLD {
                animation.request(AnimationState::Crouch(horizontal_speed));
            } else {
                animation.request(AnimationState::CrouchIdle);
            }
            continue;
        }

        // and finally RUN\IDLE
        if horizontal_speed > IDLE_ANIMATION_THRESHOLD {
            animation.request(AnimationState::Run(horizontal_speed));
        } else {
            animation.request(AnimationState::StandIdle);
        }
    }
}

fn animate(
    mut players: Query<&mut Player>,
    mut animation_players: Query<&mut AnimationPlayer>,
    mut transitions_query: Query<&mut AnimationTransitions>,
) {
    for mut player in players.iter_mut() {
        let ani = &mut player.animation;
        let Ok(mut animation_player) = animation_players.get_mut(ani.animation_player_e) else {
            continue;
        };

        let Ok(mut transitions) = transitions_query.get_mut(ani.animation_player_e) else {
            continue;
        };

        if let Some(next) = ani.requested.take() {
            let node = ani.nodes[next.clip_index()];

            let duration = if next.is_jumping() { 0.1 } else { 0.3 };

            transitions
                .play(
                    &mut animation_player,
                    node,
                    Duration::from_secs_f32(duration),
                )
                .repeat();

            let current_node = ani.nodes[ani.current.clip_index()];
            if let Some(active) = animation_player.animation_mut(current_node) {
                match ani.current {
                    AnimationState::Run(s)
                    | AnimationState::Sprint(s)
                    | AnimationState::Crouch(s) => active.set_speed(s * 0.1), // damping actual speed
                    _ => active.set_speed(1.0),
                };
            }

            debug!("current: {:?}, next: {next:?}", ani.current);
            ani.current = next;
        }
    }
}

#[derive(Component, Reflect, Clone, Debug)]
#[reflect(Component)]
pub struct Animations {
    pub current: AnimationState,
    pub requested: Option<AnimationState>,
    /// ``AnimationState`` -> Graph node
    pub nodes: Vec<AnimationNodeIndex>,
    /// Entity that owns ``AnimationPlayer``
    pub animation_player_e: Entity,
}

impl Default for Animations {
    fn default() -> Self {
        Self {
            current: AnimationState::StandIdle,
            requested: None,
            nodes: Vec::new(),
            animation_player_e: Entity::PLACEHOLDER,
        }
    }
}

/// State change helpers
impl Animations {
    fn request(&mut self, state: AnimationState) {
        if self.current.clip_index() != state.clip_index() {
            self.requested = Some(state);
        } else {
            self.current = state;
        }
    }

    pub fn idle(&mut self) {
        self.request(AnimationState::StandIdle);
    }

    pub fn run(&mut self, speed: f32) {
        self.request(AnimationState::Run(speed));
    }

    pub fn sprint(&mut self, speed: f32) {
        self.request(AnimationState::Sprint(speed));
    }

    pub fn crouch(&mut self, speed: f32) {
        self.request(AnimationState::Crouch(speed));
    }

    pub fn crouch_idle(&mut self) {
        self.request(AnimationState::CrouchIdle);
    }

    pub fn fall(&mut self) {
        self.request(AnimationState::Jump);
    }

    pub fn is_falling(&self) -> bool {
        self.current.is_jumping()
    }
}

/// The order is important here because we use it as indexes for animation node vec
#[derive(Component, Default, Reflect, Clone, Copy, PartialEq, Debug)]
#[reflect(Component)]
pub enum AnimationState {
    #[default]
    StandIdle,
    Run(f32),
    Sprint(f32),
    Jump,
    Crouch(f32),
    CrouchIdle,
}
impl AnimationState {
    pub fn clip_index(&self) -> usize {
        match self {
            AnimationState::StandIdle => 0,
            AnimationState::Run(_) => 1,
            AnimationState::Sprint(_) => 2,
            AnimationState::Jump => 3,
            AnimationState::Crouch(_) => 4,
            AnimationState::CrouchIdle => 5,
        }
    }
    pub fn is_running(&self) -> bool {
        matches!(self, AnimationState::Run(_))
    }
    pub fn is_jumping(&self) -> bool {
        matches!(self, AnimationState::Jump)
    }
}
