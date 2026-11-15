//! Headless movement checks: `MinimalPlugins`, a 60 Hz fixed tick, one `app.update()` per tick, and the
//! controller's input written straight into `AccumulatedInput` (no bindings).
use std::time::Duration;

use avian3d::prelude::*;
use bevy::{input::InputPlugin, prelude::*, time::TimeUpdateStrategy};
use bevy_ahoy::{input::AccumulatedInput, prelude::*};
use bevy_enhanced_input::prelude::*;

const HZ: f64 = 60.0;
const HALF_HEIGHT: f32 = 0.9;

#[derive(Resource, Default)]
struct Walk(Option<Vec2>);

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        TransformPlugin,
        InputPlugin,
        AssetPlugin::default(),
        bevy::mesh::MeshPlugin,
        PhysicsPlugins::default(),
        EnhancedInputPlugin,
        AhoyPlugins::default(),
    ))
    .insert_resource(Time::<Fixed>::from_hz(HZ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
        1.0 / HZ,
    )))
    .init_resource::<Walk>()
    // Stand-in for a bound Movement action: write the controller's input before the fixed loop.
    .add_systems(
        PreUpdate,
        |walk: Res<Walk>, mut q: Query<&mut AccumulatedInput>| {
            for mut i in &mut q {
                if walk.0.is_some() {
                    i.last_movement = walk.0;
                }
            }
        },
    );
    // Ground: a 40 m slab whose top is y = 0.
    app.world_mut().spawn((
        RigidBody::Static,
        Collider::cuboid(40., 1., 40.),
        Transform::from_xyz(0., -0.5, 0.),
    ));
    app.finish();
    app.cleanup();
    app.update(); // first update has zero delta
    app
}

fn player(app: &mut App, at: Vec3) -> Entity {
    app.world_mut()
        .spawn((
            CharacterController::default(),
            Collider::cylinder(0.4, 2. * HALF_HEIGHT),
            Transform::from_translation(at),
        ))
        .id()
}

fn run(app: &mut App, secs: f32) {
    for _ in 0..(secs * HZ as f32) as usize {
        app.update();
    }
}

fn pos(app: &App, e: Entity) -> Vec3 {
    app.world().get::<Transform>(e).unwrap().translation
}

fn grounded(app: &App, e: Entity) -> bool {
    app.world()
        .get::<CharacterControllerState>(e)
        .unwrap()
        .grounded
        .is_some()
}

#[test]
fn falls_lands_and_stays() {
    let mut app = app();
    let p = player(&mut app, Vec3::new(0., 5., 0.));
    run(&mut app, 0.2);
    assert!(pos(&app, p).y < 5., "falls: {}", pos(&app, p));
    run(&mut app, 2.8);
    let a = pos(&app, p);
    run(&mut app, 1.);
    let b = pos(&app, p);
    eprintln!("landed at {a} then {b}");
    assert!(grounded(&app, p), "grounded");
    assert!(
        (a.y - HALF_HEIGHT).abs() < 0.1,
        "feet on the floor: centre y {}",
        a.y
    );
    assert!(a.distance(b) < 1e-3, "at rest: {a} -> {b}");
}

#[test]
fn walks_on_input() {
    let mut app = app();
    let p = player(&mut app, Vec3::new(0., HALF_HEIGHT + 0.05, 0.));
    run(&mut app, 1.);
    let a = pos(&app, p);
    app.world_mut().resource_mut::<Walk>().0 = Some(Vec2::Y);
    run(&mut app, 1.);
    let b = pos(&app, p);
    eprintln!("walk {a} -> {b}, horizontal {}", (b - a).xz().length());
    assert!((b - a).xz().length() > 3., "walked: {a} -> {b}");
    assert!((b.y - a.y).abs() < 0.05, "level: {a} -> {b}");
    assert!(grounded(&app, p));
}

/// Walks forward into a block of the given height whose near face is 3 m ahead; returns the
/// position before and after `secs` of walking.
fn walk_into(height: f32, secs: f32) -> (Vec3, Vec3) {
    let mut app = app();
    let p = player(&mut app, Vec3::new(0., HALF_HEIGHT + 0.05, 0.));
    run(&mut app, 0.5);
    app.world_mut().resource_mut::<Walk>().0 = Some(Vec2::Y);
    run(&mut app, 0.1);
    // Forward is whatever direction a short walk from the origin went.
    let dir = pos(&app, p).with_y(0.).normalize();
    app.world_mut().resource_mut::<Walk>().0 = None;
    // A 6 m block centred 6 m ahead along the walk direction.
    let centre = dir * 6. + Vec3::Y * (height / 2.);
    app.world_mut().spawn((
        RigidBody::Static,
        Collider::cuboid(6., height, 6.),
        Transform::from_translation(centre).looking_to(dir, Vec3::Y),
    ));
    run(&mut app, 0.5);
    let a = pos(&app, p);
    app.world_mut().resource_mut::<Walk>().0 = Some(Vec2::Y);
    run(&mut app, secs);
    (a, pos(&app, p))
}

#[test]
fn climbs_a_step_under_step_size() {
    let (a, b) = walk_into(0.4, 0.6);
    eprintln!("step 0.4: {a} -> {b}");
    assert!(
        (b.y - (0.4 + HALF_HEIGHT)).abs() < 0.1,
        "on top of the step: {b}"
    );
}

#[test]
fn a_wall_blocks() {
    let (a, b) = walk_into(1.5, 2.);
    eprintln!("wall 1.5: {a} -> {b}");
    assert!(b.y < HALF_HEIGHT + 0.1, "not on top: {b}");
    assert!(
        (b - a).xz().length() < 3.0,
        "stopped at the wall: {a} -> {b}"
    );
}

#[test]
fn rides_a_moving_platform() {
    let mut app = app();
    // Kinematic platform at x=10, top at y=1, moving +X at 2 m/s.
    let plat = app
        .world_mut()
        .spawn((
            RigidBody::Kinematic,
            Collider::cuboid(4., 0.5, 4.),
            Transform::from_xyz(10., 0.75, 0.),
            LinearVelocity(Vec3::new(2., 0., 0.)),
        ))
        .id();
    let p = player(&mut app, Vec3::new(10., 1. + HALF_HEIGHT + 0.05, 0.));
    run(&mut app, 0.5);
    let (pa, ka) = (pos(&app, p), pos(&app, plat));
    run(&mut app, 2.);
    let (pb, kb) = (pos(&app, p), pos(&app, plat));
    eprintln!("platform {ka} -> {kb}; player {pa} -> {pb}");
    assert!(grounded(&app, p), "still on it");
    assert!(
        ((pb.x - pa.x) - (kb.x - ka.x)).abs() < 0.2,
        "carried with the platform"
    );
}

#[test]
fn a_moving_wall_pushes() {
    let mut app = app();
    let p = player(&mut app, Vec3::new(0., HALF_HEIGHT + 0.05, 0.));
    // A kinematic wall at x=-3 moving +X at 2 m/s.
    app.world_mut().spawn((
        RigidBody::Kinematic,
        Collider::cuboid(1., 3., 4.),
        Transform::from_xyz(-3., 1.5, 0.),
        LinearVelocity(Vec3::new(2., 0., 0.)),
    ));
    run(&mut app, 0.5);
    let a = pos(&app, p);
    run(&mut app, 2.5);
    let b = pos(&app, p);
    eprintln!("pushed {a} -> {b}");
    assert!(b.x - a.x > 1.5, "pushed along: {a} -> {b}");
}
