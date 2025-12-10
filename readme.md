# Bevy Ahoy!

[![crates.io](https://img.shields.io/crates/v/bevy_ahoy)](https://crates.io/crates/bevy_ahoy)
[![docs.rs](https://docs.rs/bevy_ahoy/badge.svg)](https://docs.rs/bevy_ahoy)


> \[!IMPORTANT\]  
> Bevy Ahoy makes use of an unreleased avian version.
>
> To ensure it works you'll need to add a patch in your Cargo.toml
> ```toml
> [patch.crates-io]
> avian3d = { git = "https://github.com/avianphysics/avian" }
> ```

A fun 3D Kinematic Character Controller for [Bevy](https://github.com/bevyengine/bevy) + [Avian](https://github.com/avianphysics/avian) + [BEI](https://github.com/simgine/bevy_enhanced_input).

<https://github.com/user-attachments/assets/ad1648d9-8a3d-43a6-9e20-886d6bb2eaad>

*Surf gameplay by Boonie, who kindly converted the Utopia surf map to a compatible format. See assets/maps/license.md for licensing information*

## What does that mean?

*Character controller* means that this crate allows you to move characters in a video game around. This can be either player characters or NPCs.

*Kinematic* means that the controller intentionally does not fully obey the simulation rules of the physics engine. That means that the character will, for example, ignore any forces.
This tradeoff allows Ahoy to fully define its own separate model of how a character should move regardless of what the laws of physics say. The goal is not realism, the goal is ~ fun ~

## Features / Roadmap

- [x] **Walking / Running**: press keys to move, change the velocity if you want your character to run
- [x] **Jumping**: Jump up to a given height. No need to fiddle with speeds. Decide for yourself via BEI if you want release to jump, autojump, etc.
- [x] **Crouching**: crouch to reduce your controller's height and point of view, uncrouch only if there's enough space for it
- [x] **Stair stepping**: walk automatically onto objects that have a very low height, such as stair steps, small rocks, etc.
- [x] **Ramp walking**: walk up ramps under a certain angle of steepness. Fall down if the ramp is too steep.
- [x] **Ground snapping**: walk down ramps and stairs instead of flying off of them
- [x] **Quake/Source movement tech**: air strafe, surf, bunny hopping, etc.
- [x] **Push objects**: Move into dynamic rigid bodies to apply force to them
- [x] **First person camera controller**: Add `CharacterControllerCameraOf` to a camera to have out-of-the-box first person camera behavior
- [x] **Coyote Time**: Jump a tiny bit after walking off a ledge for a better jump feeling
- [x] **Input Buffering**: Press the jump button a bit before actually hitting the ground to immediately jump
- [x] **Be pushed**: Stand next to a moving kinematic rigid body to be pushed by it. Put the player in peril by surrounding them with approaching walls!
- [x] **Moving platforms**: Step onto moving kinematic rigid bodies to move with them. Useful for elevators, conveyor belts, etc.
- [ ] **Events**: observe events for jump start, landing, stair stepping, etc. to add sound effects, particles, damage the character, etc.
- [ ] **Wall running**: run along walls for a given distance and jump off of them
- [ ] **Double jump**: jump a second time in the air with a different feel from the first jump. Can be chained with wall running.
- [ ] **Mantling**: Hold the jump button near the ledge while either on the ground or in the air to grab it and climb up on it
- [ ] **Water**: Dive up and down in water, move slower, and jump differently out of it
- [ ] **Surface friction**: Set the friction differently on individual surfaces to make them slippery or extra grippy
- [ ] **Ladders**: Walk or jump to a ladder to hold onto it, then move to climb up and down on it or jump to get off early. Step up the surface when you reach the end of the ladder.

## Usage

```rust
use bevy::prelude::*;
use bevy_enhanced_input::prelude::*;
use bevy_ahoy::prelude::*;

// app.add_plugins((
//    DefaultPlugins,
//    EnhancedInputPlugin,
//    PhysicsPlugins::default(),
//    AhoyPlugin::default(),
// ))
// .add_input_context::<PlayerInput>();

#[derive(Component)]
struct PlayerInput;

fn spawn_player(mut commands: Commands) {
    // Spawn the player entity
    let player = commands
        .spawn((
            // The character controller configuration
            CharacterController::default(),
            Transform::from_xyz(0.0, 20.0, 0.0),
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
        ))
        .id();

    // Spawn the camera
    commands.spawn((
        Camera3d::default(),
        // Enable the optional builtin camera controller
        CharacterControllerCameraOf::new(player),
    ));
}
```

## Inspiration

- The underlying move-and-slide uses Avian's implementation, the inspirations for which are [listed in the implementing PR](https://github.com/avianphysics/avian/pull/894).
- The core principles of the KCC derive from the Quake KCC. I highly recommend reading [myria666/qMovementDoc](https://github.com/myria666/qMovementDoc) if you want to know how it works :)
- The specific implementation flavor of the KCC is heavily inspired by Counter Strike: Source

## Alternatives / Comparison

- [tnua](https://github.com/idanarye/bevy-tnua): Floating dynamic character controller. Use this if you want more cartoony physics, or want your character to be affected by forces in a fully simulated way. Supports both Avian and Rapier.
- [bevy_fps_controller](https://github.com/qhdwight/bevy_fps_controller): KCC also inspired by Source. Does not integrate with BEI or with fixed update based workflows. Supports both Avian and Rapier.

## Design Philosophy

KCCs are incredibly closely tied to their games. At the same time, a lot of games or prototypes need something that Just Works.
For this reason, many KCC libraries try to be extremely configurable. However, I found that in my personal projects, I never 
really vibed with all that API baggage. For most use-cases, I just wanted a sensible set of defaults that Just Worked, thank you kindly. On the other hand, features that the library didn't plan for, like wall running, often lead me to explore the guts of the
library anyways, and a fork was more practical than indirectly charming the library into doing my bidding through its configuration.

As such, I designed Bevy Ahoy to be what I wished existed when I started Bevy: something simple that I don't need to spend much time learning, that I can just plug in and use if I need basic first person movement. To enable this, I consciously decided to limit the
configurability of Ahoy. If you need specific features to your game that Ahoy doesn't bring out of the box, I encourage you to fork 
it. Feel free to open an issue or ping me on the Bevy Discord if you need help with that :)

With that said, here are some goals of Ahoy:
- Require minimum setup for the common case
- Handle most terrain you throw at it
- Handle common collider shapes: Cuboids, cylinders, spheres, and in a pinch capsules
  - Sorry, Parry is not very good at capsules. You may want to use a cylinder instead for now :/
  - Other shapes may or may not work, at your discretion
- Be tightly integrated with `bevy_enhanced_input`
  - If you don't use BEI already, you really should :)
  - This allows Ahoy to neatly abstract away some nasty internal business like input accumulation,
    while allowing you to bind its behaviors to whatever you want. 
  - Plus, BEI has a lovely lovely input mocking API, allowing us to treat player and NPC input the same way.
- Be tightly integrated with Avian
  - Supporting multiple physics engines directly brings with it the need to create a big layer of abstractions and some extra glue crates, which makes forking the library for your own needs much more complex.
  - Additionally, I prefer to upstream things I need directly to Avian, to make the Avian ecosystem a better place for everyone.
    I don't have the time or energy to do that for multiple physics engines, and I don't want to "polyfill" APIs that only some engines support.
  - I also just really like Avian <3
- Give that flowy-snappy-freeing movement you know and love from the Source Engine and early id Tech games like Quake.
  - This includes cool movement tech like air strafing and surfing.
- Work for first-person and third-person games

In contrast, here are some deliberate non-goals:
- Deep configurability: just fork it instead, it should hopefully be simple.
- Be engine-agnostic
- Code specifically for disabling tech like air strafing
- Support schedules outside Bevy's fixed timestep. 
  - You can configure the schedule, but it must run as part of the fixed main loop to correctly work with Ahoy.
- Work without `bevy_enhanced_input`
- Other up-axis than Y
  - This means that top-down games must also use Y as up!
- Work as a dynamic character controller
- Support every possible collider shape
- Reproduce the behavior of Quake or Source exactly.
- Work in 2D

## Compatibility

| bevy        | bevy_ahoy              |
|-------------|------------------------|
| 0.17        | `main`                 |
