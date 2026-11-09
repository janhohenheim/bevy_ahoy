use avian3d::prelude::*;
use bevy::{
    input::common_conditions::input_just_pressed,
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};
use bevy_ahoy::prelude::*;
use bevy_enhanced_input::prelude::*;

use crate::util::{ExampleUtilPlugin, StableGround};

mod util;

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins,
            PhysicsPlugins::default(),
            EnhancedInputPlugin,
            AhoyPlugins::default(),
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

fn setup(mut commands: Commands) {
    // Spawn the player
	let player = commands.spawn_scene(
		bsn! {
            // Add the character controller configuration. We'll use the default settings for now.
			CharacterController
			// The KCC currently behaves best when using a cylinder
			Collider::cylinder(0.7, 1.8)
			Transform::from_xyz(0.0, 20.0, 0.0)
			StableGround
			PlayerInput
			Actions<PlayerInput> [
				Action<Movement>::new()
				// Normalize the input vector
				DeadZone
				Bindings [
					{preset::cardinal::wasd_keys()}
					--
					{preset::axial::left_stick()}
				]
				--
				Action<Jump>::new()
				Bindings [
					Binding(KeyCode::Space)
					--
					Binding(GamepadButton::South)
				]
				--
				Action<Crouch>::new()
				Bindings [
					Binding(KeyCode::ControlLeft)
					--
					Binding(GamepadButton::LeftTrigger2)
				]
				--
				Action<RotateCamera>::new()
				Bindings [
                    // tweak mouse and right stick sensitivity
                    // in Scale::splat values
					Binding::mouse_motion()
					~Scale::splat(0.07)
					--
					{preset::axial::new(
						(GamepadAxis::RightStickY, Scale::splat(4.0), DeadZone::default()),
						(GamepadAxis::RightStickX, Scale::splat(4.0), DeadZone::default())
					)}
				]
			]
		}
	).id();

    // Spawn the player camera
    commands.spawn((
        Camera3d::default(),
        // Enable the optional builtin camera controller
        CharacterControllerCameraOf::new(player),
    ));

	// Spawn a directional light
	commands.spawn_scene(bsn! {
		Transform::from_xyz(0.0, 1.0, 0.0).looking_at(vec3(1.0, -2.0, -2.0), Vec3::Y)
		DirectionalLight {
			shadow_maps_enabled: true
		}
	});

    // Spawn the level. This can be done in whatever way you prefer: spawn individual colliders, load a scene, use Skein, use bevy_trenchbroom, etc.
    // Ahoy will deal with it all.
    // Here we load a glTF file and create a convex hull collider for each mesh.
	commands.spawn_scene(bsn! {
		WorldAssetRoot("maps/playground.glb#Scene0")
		RigidBody::Static
		ColliderConstructorHierarchy::new(ColliderConstructor::ConvexHullFromMesh)
	});
}

#[derive(Component, Default, Clone)]
pub(crate) struct PlayerInput;

fn capture_cursor(mut cursor: Single<&mut CursorOptions>) {
    cursor.grab_mode = CursorGrabMode::Locked;
    cursor.visible = false;
}

fn release_cursor(mut cursor: Single<&mut CursorOptions>) {
    cursor.visible = true;
    cursor.grab_mode = CursorGrabMode::None;
}
