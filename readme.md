# Bevy Ahoy!

[![crates.io](https://img.shields.io/crates/v/bevy_ahoy)](https://crates.io/crates/bevy_ahoy)
[![docs.rs](https://docs.rs/bevy_ahoy/badge.svg)](https://docs.rs/bevy_ahoy)

A fun 3D Kinematic Character Controller for [Bevy](https://github.com/bevyengine/bevy) + [Avian](https://github.com/avianphysics/avian) + [BEI](https://github.com/simgine/bevy_enhanced_input).

## What does that mean?

*Character controller* means that this crate allows you to move characters in a video game around. This can be either player characters or NPCs.

*Kinematic* means that the controller intentionally does not fully obey the simulation rules of the physics engine. That means that the character will for example ignore any forces.
This tradeoff allows Ahoy to fully define its own separate model of how a character should move regardless of what the laws of physics say. The goal is not realism, the goal is ~ fun ~

## Features / Roadmap

- [x] **Walking / Running**: press keys to move, change the velocity if you want your character to run
- [x] **Jumping**: Jump up to a given height. No need to fiddle with speeds. Decide for yourself via BEI if you want release to jump, autojump, etc.
- [x] **Crouching**: crouch to reduce your controller's height and point of view, uncrouch only if there's enough space for it
- [x] **Gravity**: fine-tune it to your heart's content
- [x] **Stair stepping**: walk automatically onto objects that have a very low height, such as stair steps, small rocks, etc.
- [x] **Ramp walking**: walk up ramps under a certain angle of steepness. Fall down if the ramp is too steep.
- [x] **Ground snapping**: walk down ramps and stairs instead of flying off of them
- [x] **Quake/Source movement tech**: air strafe, surf, bunny hopping, etc.
- [ ] **Push objects**: Move into dynamic rigid bodies to apply force to them
- [ ] **Be pushed**: Stand next to a moving kinematic rigid body to be pushed by it. Put the player in peril by surrounding them with approaching walls!
- [ ] **Moving platforms**: Step onto moving kinematic rigid bodies to move with them. Useful for elevators, conveyor belts, etc.
- [x] **First person camera controller**: Add `CameraOfCharacterController` to a camera to have out-of-the-box first person camera behavior
- [ ] **Events**: observe events for jump start, landing, stair stepping, etc. to add sound effects, particles, damage the character, etc.
- [ ] **Wall running**: run along walls for a given distance and jump off of them
- [ ] **Double jump**: jump a second time in the air with a different feel from the first jump. Can be chained with wall running.
- [ ] **Mantling**: Hold the jump button near ledge while either on the ground or in air to grab it and climb up on it
- [ ] **Water**: Dive up and down in water, move slower, and jump differently out of it
- [ ] **Surface friction**: Set the friction differently on individual surfaces to make them slippery or extra grippy
- [ ] **Ladders**: Walk or jump to a ladder to hold onto it, then move to climb up and down on it or jump to get off early. Step up the surface when you reach the end of the ladder.
- [ ] **Coyote Time**: Jump a tiiny bit after walking off a ledge for better jump feeling
- [ ] **Input Buffering**: Press the jump button a bit before actually hitting the ground to immediatly jump

## Usage

TODO ;)

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
  - This allows Ahoy to neatly abstracts away some nasty internal business like input accumulation,
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

| bevy        | bevy_bae               |
|-------------|------------------------|
| 0.17        | `main`                 |
