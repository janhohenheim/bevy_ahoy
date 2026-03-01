use crate::util::{ExampleUtilPlugin, StableGround};
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
use core::ops::Deref;
use std::time::Duration;

mod util;

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins
                .set(GltfPlugin {
                    convert_coordinates: GltfConvertCoordinates {
                        rotate_scene_entity: false,
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
            FixedUpdate,
            (rotate_player, calcucate_animations, animate)
                .chain()
                .in_set(AhoySystems::MoveCharacters),
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
    commands.insert_resource(PlayerModel(assets.load("models/ual.glb")));
    commands.spawn(SceneRoot(assets.load("maps/playground.map#Scene")));
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
        speed: 20.0,
        jump_height: 2.0,
        gravity: 20.0,
        crouch_height: 2.0,
        ..default()
    },
    RigidBody::Kinematic,
    Collider::cylinder(0.7, 1.8),
    CollisionLayers::new(
        [CollisionLayer::Player],
        LayerMask::ALL,
    ),
    StableGround::default(),
    CharacterLook::default(),
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
    _: On<Insert, SpawnPlayer>,
    player_model: Res<PlayerModel>,
    gltfs: Res<Assets<Gltf>>,
    spawners: Query<(Entity, &Transform), With<SpawnPlayer>>,
    players: Query<Entity, With<Player>>,
    mut commands: Commands,
) {
    let Some(gltf) = gltfs.get(&player_model.0) else {
        return;
    };

    // If we get here, the model is ready.
    for (_spawner_entity, transform) in &spawners {
        // Despawn old players if necessary
        for p in &players {
            commands.entity(p).despawn();
        }

        commands
            .spawn((
                Player::default(),
                *transform,
                PreviousPosition(transform.translation),
                ThirdPersonCameraTarget,
            ))
            .with_children(|parent| {
                parent
                    .spawn((
                        SceneRoot(gltf.scenes[0].clone()),
                        Transform::from_xyz(0.0, -1.0, 0.0),
                    ))
                    .observe(prepare_animations);
            });

        // for some reason this panics, no idea why
        // commands.entity(spawner_entity).despawn();
    }
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
                    Scale::splat(0.3),
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

#[solid_class(base(Transform, Visibility))]
#[component(on_add = on_add_water)]
pub struct Water {
    speed: f32,
}

impl Default for Water {
    fn default() -> Self {
        Self { speed: 1.0 }
    }
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

// Rotation
fn rotate_player(
    movement: Single<&Action<Movement>>,
    camera: Single<&Transform, With<Camera3d>>,
    mut player_q: Query<(&mut Transform, &mut CharacterLook), Without<Camera3d>>,
) {
    let movement = *movement.into_inner();

    for (mut pos, mut look) in player_q.iter_mut() {
        let input_dir = camera.movement_direction(*movement);

        if input_dir.length_squared() > 0.1 {
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
    _: On<SceneInstanceReady>,
    gltfs: Res<Assets<Gltf>>,
    children_q: Query<&Children>,
    player_model: Res<PlayerModel>,
    animation_player_q: Query<Entity, With<AnimationPlayer>>,
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
            return;
        };

        // we list acnimations here in the same order they are listed in AnimationState enum
        let clips = vec![
            gltf.named_animations["Idle_Loop"].clone(),
            gltf.named_animations["Jog_Fwd_Loop"].clone(),
            gltf.named_animations["Jump_Loop"].clone(),
            gltf.named_animations["Crouch_Fwd_Loop"].clone(),
            gltf.named_animations["Crouch_Idle_Loop"].clone(),
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
    const IDLE_ANIMATION_THRESHOLD: f32 = 0.1;

    for (ahoy_state, pos, mut prev_pos, mut player) in players.iter_mut() {
        let animation = &mut player.animation;

        let displacement = pos.translation - prev_pos.0;
        let velocity = displacement / time.delta_secs();
        let horizontal_speed = Vec3::new(velocity.x, 0.0, velocity.z).length().abs();
        // let _vertical_speed = velocity.y;
        prev_pos.0 = pos.translation;

        let grounded = ahoy_state.grounded.is_some();

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
                    AnimationState::Run(s) | AnimationState::Crouch(s) => {
                        active.set_speed(s.clamp(0.0, 2.0))
                    }
                    _ => active.set_speed(1.0),
                };
            }

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
}

/// The order is important here because we use it as indexes for animation node vec
#[derive(Component, Default, Reflect, Clone, Copy, PartialEq, Debug)]
#[reflect(Component)]
pub enum AnimationState {
    #[default]
    StandIdle,
    Run(f32),
    Jump,
    Crouch(f32),
    CrouchIdle,
}
impl AnimationState {
    pub fn clip_index(&self) -> usize {
        match self {
            AnimationState::StandIdle => 0,
            AnimationState::Run(_) => 1,
            AnimationState::Jump => 2,
            AnimationState::Crouch(_) => 3,
            AnimationState::CrouchIdle => 4,
        }
    }
    fn is_jumping(&self) -> bool {
        matches!(self, AnimationState::Jump)
    }
}

trait CameraExt {
    fn movement_direction(&self, input: Vec2) -> Vec3;
}
impl CameraExt for Transform {
    /// Get movement direction as normalized verticality-agnostic vector
    fn movement_direction(&self, input: Vec2) -> Vec3 {
        let forward = self.forward();
        let forward_flat = Vec3::new(forward.x, 0.0, forward.z);
        let right = forward_flat.cross(Vec3::Y).normalize_or_zero();
        let direction = (right * input.x) + (forward_flat * input.y);
        direction.normalize_or_zero()
    }
}

trait EntityExt {
    fn get_recursive<T: Component>(
        &self,
        children_q: Query<&Children>,
        component_q: Query<Entity, With<T>>,
    ) -> Option<Entity>;
}
impl EntityExt for Entity {
    fn get_recursive<T: Component>(
        &self,
        children_q: Query<&Children>,
        component_q: Query<Entity, With<T>>,
    ) -> Option<Entity> {
        if component_q.get(*self).is_ok() {
            return Some(*self);
        }

        if let Ok(children) = children_q.get(*self) {
            for child in children.iter() {
                if let Some(e) = child.get_recursive(children_q, component_q) {
                    return Some(e);
                }
            }
        }

        None
    }
}
