use avian3d::prelude::*;
use bevy::{
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    gltf::GltfPlugin,
    input::common_conditions::input_just_pressed,
    light::CascadeShadowConfigBuilder,
    log::{LogPlugin, tracing_subscriber::field::MakeExt},
    pbr::Atmosphere,
    prelude::*,
    scene::SceneInstanceReady,
    window::{CursorGrabMode, CursorOptions, WindowResolution},
};
use bevy_ahoy::{kcc::CharacterControllerState, prelude::*};
use bevy_enhanced_input::prelude::{Release, *};
use bevy_mod_mipmap_generator::{MipmapGeneratorPlugin, generate_mipmaps};
use bevy_trenchbroom::{physics::SceneCollidersReady, prelude::*};
use bevy_trenchbroom_avian::AvianPhysicsBackend;
use core::ops::Deref;

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins
                .set(GltfPlugin {
                    use_model_forward_direction: true,
                    ..default()
                })
                .set(LogPlugin {
                    filter: format!(
                        concat!(
                            "{default},",
                            "symphonia_bundle_mp3::demuxer=warn,",
                            "symphonia_format_caf::demuxer=warn,",
                            "symphonia_format_isompf4::demuxer=warn,",
                            "symphonia_format_mkv::demuxer=warn,",
                            "symphonia_format_ogg::demuxer=warn,",
                            "symphonia_format_riff::demuxer=warn,",
                            "symphonia_format_wav::demuxer=warn,",
                            "bevy_trenchbroom::physics=off,",
                            "calloop::loop_logic=error,",
                        ),
                        default = bevy::log::DEFAULT_FILTER
                    ),
                    fmt_layer: |_| {
                        Some(Box::new(
                            bevy::log::tracing_subscriber::fmt::Layer::default()
                                .map_fmt_fields(MakeExt::debug_alt)
                                .with_writer(std::io::stderr),
                        ))
                    },
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Window {
                        resolution: WindowResolution::new(1920, 1080),
                        ..default()
                    }
                    .into(),
                    ..default()
                }),
            PhysicsPlugins::default(),
            EnhancedInputPlugin,
            AhoyPlugin::default(),
            MipmapGeneratorPlugin,
            TrenchBroomPlugins(
                TrenchBroomConfig::new("bevy_ahoy").default_solid_spawn_hooks(|| {
                    SpawnHooks::new()
                        .convex_collider()
                        .smooth_by_default_angle()
                }),
            ),
            TrenchBroomPhysicsPlugin::new(AvianPhysicsBackend),
        ))
        .add_input_context::<PlayerInput>()
        .add_systems(Startup, (setup, setup_ui))
        .add_observer(reset_player)
        .add_systems(
            Update,
            (
                update_debug_text,
                generate_mipmaps::<StandardMaterial>,
                capture_cursor.run_if(input_just_pressed(MouseButton::Left)),
                release_cursor.run_if(input_just_pressed(KeyCode::Escape)),
            ),
        )
        .run()
}

fn setup(mut commands: Commands, assets: Res<AssetServer>) {
    commands
        .spawn(SceneRoot(assets.load("maps/playground.map#Scene")))
        .observe(tweak_materials);
    commands.spawn((
        Camera3d::default(),
        EnvironmentMapLight {
            diffuse_map: assets.load("environment_maps/voortrekker_interior_1k_diffuse.ktx2"),
            specular_map: assets.load("environment_maps/voortrekker_interior_1k_specular.ktx2"),
            intensity: 2000.0,
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov: 70.0_f32.to_radians(),
            ..default()
        }),
        Atmosphere::EARTH,
    ));
    commands.spawn((
        Transform::from_xyz(0.0, 1.0, 0.0).looking_at(vec3(1.0, -2.0, -2.0), Vec3::Y),
        DirectionalLight {
            shadows_enabled: true,
            ..default()
        },
        CascadeShadowConfigBuilder {
            maximum_distance: 500.0,
            overlap_proportion: 0.5,
            ..default()
        }
        .build(),
    ));
}

#[derive(Component, Reflect, Debug)]
#[reflect(Component)]
struct Player;

#[point_class(base(Transform, Visibility))]
#[component(on_add = SpawnPlayer::on_add)]
struct SpawnPlayer;

impl SpawnPlayer {
    fn on_add(mut world: DeferredWorld, ctx: HookContext) {
        if world.is_scene_world() {
            return;
        }
        if world.try_query::<&Player>().unwrap().single(&world).is_ok() {
            return;
        }
        let Some(transform) = world.get::<Transform>(ctx.entity).copied() else {
            return;
        };
        let player = world
            .commands()
            .spawn((
                Player,
                transform,
                PlayerInput,
                CharacterController::default(),
                RigidBody::Kinematic,
                Collider::cylinder(0.7, 1.8),
                // For debugging
                CollidingEntities::default(),
            ))
            .id();
        let camera = world
            .try_query_filtered::<Entity, With<Camera3d>>()
            .unwrap()
            .single(&world)
            .unwrap();
        world
            .commands()
            .entity(camera)
            .insert(CharacterControllerCameraOf(player));
    }
}

fn reset_player(
    _fire: On<Fire<Reset>>,
    player: Single<(&mut Transform, &mut LinearVelocity), With<Player>>,
    spawner: Single<&Transform, (With<SpawnPlayer>, Without<Player>)>,
) {
    let (mut transform, mut velocity) = player.into_inner();
    **velocity = Vec3::ZERO;
    transform.translation = spawner.translation;
}

fn tweak_materials(
    ready: On<SceneInstanceReady>,
    children: Query<&Children>,
    materials: Query<&MeshMaterial3d<StandardMaterial>>,
    mut material_assets: ResMut<Assets<StandardMaterial>>,
) {
    for mat in materials.iter_many(children.iter_descendants(ready.entity)) {
        let mat = material_assets.get_mut(mat).unwrap();
        mat.perceptual_roughness = 0.9;
    }
}

#[derive(Component, Default)]
#[component(on_add = PlayerInput::on_add)]
pub(crate) struct PlayerInput;

#[derive(Debug, InputAction)]
#[action_output(Vec2)]
pub(crate) struct Reset;

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
                    bindings![KeyCode::ControlLeft, GamepadButton::LeftTrigger],
                ),
                (
                    Action::<Reset>::new(),
                    bindings![KeyCode::KeyR, GamepadButton::Select],
                    Release::default(),
                ),
                (
                    Action::<RotateCamera>::new(),
                    Scale::splat(0.1),
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
    world
        .commands()
        .entity(ctx.entity)
        .insert((
            SceneRoot(
                assets
                    .load(GltfAssetLabel::Scene(0).from_asset(T::CLASS_INFO.model_path().unwrap())),
            ),
            ColliderConstructorHierarchy::new(ColliderConstructor::ConvexHullFromMesh),
            if dynamic {
                RigidBody::Dynamic
            } else {
                RigidBody::Static
            },
            TransformInterpolation,
        ))
        .observe(|ready: On<SceneCollidersReady>, mut commands: Commands| {
            for collider in &ready.collider_entities {
                commands.entity(*collider).insert(ColliderDensity(100.0));
            }
        });
}

fn capture_cursor(mut cursor: Single<&mut CursorOptions>) {
    cursor.grab_mode = CursorGrabMode::Locked;
    cursor.visible = false;
}

fn release_cursor(mut cursor: Single<&mut CursorOptions>) {
    cursor.visible = true;
    cursor.grab_mode = CursorGrabMode::None;
}

fn update_debug_text(
    mut text: Single<&mut Text, With<DebugText>>,
    kcc: Single<
        (
            &CharacterControllerState,
            &LinearVelocity,
            &CollidingEntities,
            &ColliderAabb,
        ),
        With<CharacterController>,
    >,
    camera: Single<&Transform, With<Camera>>,
    names: Query<NameOrEntity>,
) {
    let (state, velocity, colliding_entities, aabb) = kcc.into_inner();
    let velocity = **velocity;
    let speed = velocity.length();
    let horizontal_speed = velocity.xz().length();
    let camera_position = camera.translation;
    let collisions = names
        .iter_many(state.touching_entities.iter())
        .map(|name| {
            name.name
                .map(|n| format!("{} ({})", name.entity, n))
                .unwrap_or_else(|| format!("{}", name.entity))
        })
        .collect::<Vec<_>>();
    let real_collisions = names
        .iter_many(colliding_entities.iter())
        .map(|name| {
            name.name
                .map(|n| format!("{} ({})", name.entity, n))
                .unwrap_or_else(|| format!("{}", name.entity))
        })
        .collect::<Vec<_>>();
    let ground = state
        .grounded
        .and_then(|ground| names.get(ground.entity).ok())
        .map(|name| {
            name.name
                .map(|n| format!("{} ({})", name.entity, n))
                .unwrap_or(format!("{}", name.entity))
        });
    text.0 = format!(
        "Speed: {speed:.3}\nHorizontal Speed: {horizontal_speed:.3}\nVelocity: [{:.3}, {:.3}, {:.3}]\nCamera Position: [{:.3}, {:.3}, {:.3}]\nCollider Aabb:\n  min:[{:.3}, {:.3}, {:.3}]\n  max:[{:.3}, {:.3}, {:.3}]\nReal Collisions: {:#?}\nCollisions: {:#?}\nGround: {:?}",
        velocity.x,
        velocity.y,
        velocity.z,
        camera_position.x,
        camera_position.y,
        camera_position.z,
        aabb.min.x,
        aabb.min.y,
        aabb.min.z,
        aabb.max.x,
        aabb.max.y,
        aabb.max.z,
        real_collisions,
        collisions,
        ground
    );
}

#[derive(Component, Reflect, Debug)]
#[reflect(Component)]
pub(crate) struct DebugText;

fn setup_ui(mut commands: Commands) {
    commands.spawn((Node::default(), Text::default(), DebugText));
    commands.spawn((
        Node {
            justify_self: JustifySelf::End,
            justify_content: JustifyContent::End,
            ..default()
        },
        Text::new(
            "Controls:\nWASD: move\nSpace: jump\nSpace (hold): autohop\nCtrl: crouch\nEsc: free mouse\nR: reset position",
        ),
    ));
}
