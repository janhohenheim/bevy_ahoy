use avian3d::prelude::*;
use bevy::{
    input::common_conditions::input_just_pressed,
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};
use bevy_ahoy::prelude::*;
use bevy_enhanced_input::prelude::*;

use crate::util::ExampleUtilPlugin;

mod util;

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins,
            PhysicsPlugins::default(),
            EnhancedInputPlugin,
            AhoyPlugin::default(),
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
        .run()
}

fn setup(mut commands: Commands, assets: Res<AssetServer>) {
    // Player
    let player = commands
        .spawn((
            // The character controller configuration
            CharacterController::default(),
            // Configure inputs
            PlayerInput,
            actions!(PlayerInput[
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
                    Action::<RotateCamera>::new(),
                    Scale::splat(0.04),
                    Bindings::spawn((
                        Spawn(Binding::mouse_motion()),
                        Axial::right_stick()
                    ))
                ),
            ]),
            Transform::from_xyz(0.0, 20.0, 0.0),
        ))
        .id();

    // Camera
    commands.spawn((
        Camera3d::default(),
        // Enable optional builtin camera controller
        CharacterControllerCameraOf::new(player),
    ));

    // Light
    commands.spawn((
        Transform::from_xyz(0.0, 1.0, 0.0).looking_at(vec3(1.0, -2.0, -2.0), Vec3::Y),
        DirectionalLight {
            shadows_enabled: true,
            ..default()
        },
    ));

    // Level
    commands.spawn((
        SceneRoot(assets.load("maps/playground.glb#Scene0")),
        RigidBody::Static,
        ColliderConstructorHierarchy::new(ColliderConstructor::ConvexHullFromMesh),
    ));
}

#[derive(Component, Default)]
pub(crate) struct PlayerInput;

fn capture_cursor(mut cursor: Single<&mut CursorOptions>) {
    cursor.grab_mode = CursorGrabMode::Locked;
    cursor.visible = false;
}

fn release_cursor(mut cursor: Single<&mut CursorOptions>) {
    cursor.visible = true;
    cursor.grab_mode = CursorGrabMode::None;
}
