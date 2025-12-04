use avian3d::character_controller::move_and_slide::{self, MoveHitData};
use bevy_ecs::{
    intern::Interned,
    query::QueryData,
    schedule::ScheduleLabel,
    system::{
        SystemParam,
        lifetimeless::{Read, Write},
    },
};
use core::fmt::Debug;
use core::time::Duration;
use tracing::warn;

use crate::{CharacterControllerState, input::AccumulatedInput, prelude::*};

pub(super) fn plugin(schedule: Interned<dyn ScheduleLabel>) -> impl Fn(&mut App) {
    move |app: &mut App| {
        app.add_systems(schedule, run_kcc.in_set(AhoySystems::MoveCharacters));
    }
}

#[derive(QueryData)]
#[query_data(mutable, derive(Debug))]
struct Ctx {
    velocity: Write<LinearVelocity>,
    state: Write<CharacterControllerState>,
    transform: Write<Transform>,
    input: Write<AccumulatedInput>,
    cfg: Read<CharacterController>,
    cam: Option<Read<CharacterControllerCamera>>,
}

#[derive(QueryData)]
#[query_data(mutable, derive(Debug))]
struct ColliderComponents {
    lin_vel: Read<LinearVelocity>,
    ang_vel: Read<AngularVelocity>,
    com: Read<ComputedCenterOfMass>,
    pos: Read<Position>,
    rot: Read<Rotation>,
}

fn run_kcc(
    mut kccs: Query<Ctx>,
    cams: Query<&Transform, Without<CharacterController>>,
    time: Res<Time>,
    move_and_slide: MoveAndSlide,
    // TODO: allow this to be other KCCs
    colliders: Query<ColliderComponents, Without<CharacterController>>,
) {
    let mut colliders = colliders.transmute_lens_inner();
    let colliders = colliders.query();
    for mut ctx in &mut kccs {
        ctx.state.touching_entities.clear();
        ctx.state.last_ground.tick(time.delta());
        ctx.state.last_step_up.tick(time.delta());
        ctx.state.last_step_down.tick(time.delta());

        depenetrate_character(&move_and_slide, &mut ctx);

        update_grounded(&move_and_slide, &colliders, &time, &mut ctx);

        handle_crouching(&move_and_slide, &mut ctx);

        // here we'd handle things like spectator, dead, noclip, etc.
        start_gravity(&time, &mut ctx);

        handle_jump(&time, &colliders, &mut ctx);

        handle_mantle(&time, &colliders, &move_and_slide, &mut ctx);

        // Friction is handled before we add in any base velocity. That way, if we are on a conveyor,
        //  we don't slow when standing still, relative to the conveyor.
        if ctx.state.grounded.is_some() {
            ctx.velocity.y = 0.0;
            friction(&time, &mut ctx);
        }

        validate_velocity(&mut ctx);

        let orientation = ctx
            .cam
            .and_then(|e| cams.get(e.get()).copied().ok())
            .unwrap_or(*ctx.transform);
        let wish_velocity = calculate_wish_velocity(orientation, &ctx);
        if ctx.state.grounded.is_some() {
            ground_move(wish_velocity, &time, &move_and_slide, &mut ctx);
        } else {
            air_move(wish_velocity, &time, &move_and_slide, &mut ctx);
        }

        update_grounded(&move_and_slide, &colliders, &time, &mut ctx);
        validate_velocity(&mut ctx);

        finish_gravity(&time, &mut ctx);

        if ctx.state.grounded.is_some() {
            ctx.velocity.y = ctx.state.base_velocity.y;
            ctx.state.last_ground.reset();
        }
        // TODO: check_falling();
    }
}

fn depenetrate_character(move_and_slide: &MoveAndSlide, ctx: &mut CtxItem) {
    let offset = move_and_slide.depenetrate(
        ctx.state.collider(),
        ctx.transform.translation,
        ctx.transform.rotation,
        &((&ctx.cfg.move_and_slide).into()),
        &ctx.cfg.filter,
    );
    ctx.transform.translation += offset;
}

fn ground_move(wish_velocity: Vec3, time: &Time, move_and_slide: &MoveAndSlide, ctx: &mut CtxItem) {
    ctx.velocity.y = 0.0;
    ground_accelerate(wish_velocity, ctx.cfg.acceleration_hz, time, ctx);
    ctx.velocity.y = 0.0;

    ctx.velocity.0 += ctx.state.base_velocity;
    let speed = ctx.velocity.length();

    if speed < 0.01 {
        // zero velocity out and remove base
        ctx.velocity.0 = -ctx.state.base_velocity;
        return;
    }

    let mut movement = ctx.velocity.0 * time.delta_secs();
    movement.y = 0.0;

    let hit = move_and_slide.cast_move(
        ctx.state.collider(),
        ctx.transform.translation,
        ctx.transform.rotation,
        movement,
        ctx.cfg.move_and_slide.skin_width,
        &ctx.cfg.filter,
    );

    if hit.is_none() {
        ctx.transform.translation += movement;
        ctx.velocity.0 -= ctx.state.base_velocity;
        depenetrate_character(move_and_slide, ctx);
        snap_to_ground(move_and_slide, ctx);
        return;
    };

    let step_height = if ctx
        .input
        .craned
        .as_ref()
        .is_some_and(|c| c.elapsed() < ctx.cfg.jump_input_buffer)
    {
        ctx.input.craned = None;
        ctx.cfg.crane_height
    } else {
        ctx.cfg.step_size
    };

    step_move(step_height, time, move_and_slide, ctx);
    ctx.velocity.0 -= ctx.state.base_velocity;
    snap_to_ground(move_and_slide, ctx);
}

fn ground_accelerate(wish_velocity: Vec3, acceleration_hz: f32, time: &Time, ctx: &mut CtxItem) {
    let Ok((wish_dir, wish_speed)) = Dir3::new_and_length(wish_velocity) else {
        return;
    };
    let current_speed = ctx.velocity.dot(*wish_dir);
    let add_speed = wish_speed - current_speed;

    if add_speed <= 0.0 {
        return;
    }

    // TODO: read this from ground
    let surface_friction = 1.0;
    let accel_speed = wish_speed * acceleration_hz * time.delta_secs() * surface_friction;
    let accel_speed = f32::min(accel_speed, add_speed);

    ctx.velocity.0 += accel_speed * wish_dir;
}

fn air_move(wish_velocity: Vec3, time: &Time, move_and_slide: &MoveAndSlide, ctx: &mut CtxItem) {
    air_accelerate(wish_velocity, ctx.cfg.air_acceleration_hz, time, ctx);

    ctx.velocity.0 += ctx.state.base_velocity;
    if ctx
        .input
        .craned
        .as_ref()
        .is_some_and(|c| c.elapsed() < ctx.cfg.jump_input_buffer)
    {
        ctx.input.craned = None;
        step_move(ctx.cfg.crane_height, time, move_and_slide, ctx);
    } else {
        step_move(ctx.cfg.step_size, time, move_and_slide, ctx);
    }

    ctx.velocity.0 -= ctx.state.base_velocity;
}

fn air_accelerate(wish_velocity: Vec3, acceleration_hz: f32, time: &Time, ctx: &mut CtxItem) {
    let Ok((wish_dir, wish_speed)) = Dir3::new_and_length(wish_velocity) else {
        return;
    };
    let wishspd = f32::min(wish_speed, ctx.cfg.max_air_wish_speed);
    let current_speed = ctx.velocity.dot(*wish_dir);

    let add_speed = wishspd - current_speed;

    if add_speed <= 0.0 {
        return;
    }

    // TODO: read this from ground
    let surface_friction = 1.0;
    let accel_speed = wish_speed * acceleration_hz * time.delta_secs() * surface_friction;
    let accel_speed = f32::min(accel_speed, add_speed);

    ctx.velocity.0 += accel_speed * wish_dir;
}

fn step_move(step_height: f32, time: &Time, move_and_slide: &MoveAndSlide, ctx: &mut CtxItem) {
    let original_position = ctx.transform.translation;
    let original_velocity = ctx.velocity.0;
    let original_touching_entities = ctx.state.touching_entities.clone();

    // Slide the direct path
    move_character(time, move_and_slide, ctx);

    let down_touching_entities = ctx.state.touching_entities.clone();
    let down_position = ctx.transform.translation;
    let down_velocity = ctx.velocity.0;

    ctx.transform.translation = original_position;
    ctx.velocity.0 = original_velocity;
    ctx.state.touching_entities = original_touching_entities;

    // step up
    let cast_dir = Dir3::Y;
    let cast_len = step_height;

    let hit = move_and_slide.cast_move(
        ctx.state.collider(),
        ctx.transform.translation,
        ctx.transform.rotation,
        cast_dir * cast_len,
        ctx.cfg.move_and_slide.skin_width,
        &ctx.cfg.filter,
    );

    let dist = hit.map(|hit| hit.distance).unwrap_or(cast_len);
    ctx.transform.translation += cast_dir * dist;

    // Verify we have enough space to stand
    let hit = move_and_slide.cast_move(
        ctx.state.collider(),
        ctx.transform.translation,
        ctx.transform.rotation,
        ctx.velocity.normalize_or_zero() * ctx.cfg.min_step_ledge_space,
        ctx.cfg.move_and_slide.skin_width,
        &ctx.cfg.filter,
    );
    if hit.is_some() {
        ctx.transform.translation = down_position;
        ctx.velocity.0 = down_velocity;
        ctx.state.touching_entities = down_touching_entities;
        return;
    }

    // try to slide from upstairs
    move_character(time, move_and_slide, ctx);

    let cast_dir = Dir3::NEG_Y;
    let hit = move_and_slide.cast_move(
        ctx.state.collider(),
        ctx.transform.translation,
        ctx.transform.rotation,
        cast_dir * cast_len,
        ctx.cfg.move_and_slide.skin_width,
        &ctx.cfg.filter,
    );

    // If we either fall or slide, use the direct slide instead
    if !hit.is_some_and(|h| h.normal1.y >= ctx.cfg.min_walk_cos || ctx.cfg.step_into_air) {
        ctx.transform.translation = down_position;
        ctx.velocity.0 = down_velocity;
        ctx.state.touching_entities = down_touching_entities;
        return;
    };
    let hit = hit.unwrap();
    ctx.transform.translation += cast_dir * hit.distance;
    depenetrate_character(move_and_slide, ctx);

    let vec_up_pos = ctx.transform.translation;

    // use the one that went further
    let down_dist = down_position.xz().distance_squared(original_position.xz());
    let up_dist = vec_up_pos.xz().distance_squared(original_position.xz());
    if down_dist >= up_dist {
        ctx.transform.translation = down_position;
        ctx.velocity.0 = down_velocity;
        ctx.state.touching_entities = down_touching_entities;
    } else {
        ctx.velocity.y = down_velocity.y;
        ctx.state.last_step_up.reset();
    }
}

fn move_character(time: &Time, move_and_slide: &MoveAndSlide, ctx: &mut CtxItem) {
    let mut config = ctx.cfg.move_and_slide.clone();
    if let Some(grounded) = ctx.state.grounded {
        config.planes.push(Dir3::new_unchecked(grounded.normal1));
    }

    let mut touching_entities = std::mem::take(&mut ctx.state.touching_entities);
    let out = move_and_slide.move_and_slide(
        ctx.state.collider(),
        ctx.transform.translation,
        ctx.transform.rotation,
        ctx.velocity.0,
        time.delta(),
        &config,
        &ctx.cfg.filter,
        |hit| {
            touching_entities.push(hit.into());
            true
        },
    );
    ctx.transform.translation = out.position;
    ctx.velocity.0 = out.projected_velocity;
    std::mem::swap(&mut ctx.state.touching_entities, &mut touching_entities);
}

fn snap_to_ground(move_and_slide: &MoveAndSlide, ctx: &mut CtxItem) {
    let cast_dir = Vec3::Y;
    let cast_len = ctx.cfg.ground_distance;

    let hit = move_and_slide.cast_move(
        ctx.state.collider(),
        ctx.transform.translation,
        ctx.transform.rotation,
        cast_dir * cast_len,
        ctx.cfg.move_and_slide.skin_width,
        &ctx.cfg.filter,
    );
    let up_dist = hit.map(|h| h.distance).unwrap_or(cast_len);
    let start = ctx.transform.translation + cast_dir * up_dist;
    let cast_dir = Vec3::NEG_Y;
    let cast_len = up_dist + ctx.cfg.step_size;

    let hit = move_and_slide.cast_move(
        ctx.state.collider(),
        start,
        ctx.transform.rotation,
        cast_dir * cast_len,
        ctx.cfg.move_and_slide.skin_width,
        &ctx.cfg.filter,
    );
    let Some(hit) = hit else {
        return;
    };
    if hit.intersects()
        || hit.normal1.y < ctx.cfg.min_walk_cos
        || hit.distance <= ctx.cfg.ground_distance
    {
        return;
    }
    let original_position = ctx.transform.translation;
    ctx.transform.translation = start + cast_dir * hit.distance;
    if original_position.y - ctx.transform.translation.y > ctx.cfg.step_down_detection_distance {
        ctx.state.last_step_down.reset();
    }
    depenetrate_character(move_and_slide, ctx);
}

fn update_grounded(
    move_and_slide: &MoveAndSlide,
    colliders: &Query<ColliderComponents>,
    time: &Time,
    ctx: &mut CtxItem,
) {
    // TODO: reset surface friction here for some reason? something something water

    let y_vel = ctx.velocity.y;
    let moving_up = y_vel > 0.0;
    let mut moving_up_rapidly = y_vel > ctx.cfg.unground_speed;
    if moving_up_rapidly && ctx.state.grounded.is_some() {
        let ground_entity_y_vel = ctx.state.base_velocity.y;
        moving_up_rapidly = (y_vel - ground_entity_y_vel) > ctx.cfg.unground_speed;
    }

    let is_on_ladder = false;
    if moving_up_rapidly || (moving_up && is_on_ladder) {
        set_grounded(None, colliders, time, ctx);
    } else {
        let cast_dir = Dir3::NEG_Y;
        let cast_dist = if ctx.state.base_velocity.y < 0.0 {
            ctx.cfg.ground_distance - ctx.state.base_velocity.y * time.delta_secs()
        } else {
            ctx.cfg.ground_distance
        };
        let hit = move_and_slide.cast_move(
            ctx.state.collider(),
            ctx.transform.translation,
            ctx.transform.rotation,
            cast_dir * cast_dist,
            ctx.cfg.move_and_slide.skin_width,
            &ctx.cfg.filter,
        );
        if let Some(hit) = hit
            && hit.normal1.y >= ctx.cfg.min_walk_cos
        {
            set_grounded(hit, colliders, time, ctx);
        } else {
            set_grounded(None, colliders, time, ctx);
            // TODO: set surface friction to 0.25 for some reason
        }
    }
    // TODO: fire ground changed event
}

fn set_grounded(
    new_ground: impl Into<Option<MoveHitData>>,
    colliders: &Query<ColliderComponents>,
    time: &Time,
    ctx: &mut CtxItem,
) {
    let new_ground = new_ground.into();
    let old_ground = ctx.state.grounded;

    if new_ground.is_none()
        && let Some(old_ground) = old_ground
        && let Ok(platform) = colliders.get(old_ground.entity)
    {
        let platform_movement = calculate_platform_movement(old_ground, &platform, time, ctx);
        ctx.state.base_velocity.y = platform_movement.y / time.delta_secs();
    } else if let Some(new_ground) = new_ground
        && let Ok(platform) = colliders.get(new_ground.entity)
    {
        let platform_movement = calculate_platform_movement(new_ground, &platform, time, ctx);
        ctx.state.base_velocity = platform_movement / time.delta_secs();
    }

    ctx.state.grounded = new_ground;

    if ctx.state.grounded.is_some() {
        ctx.velocity.y = 0.0;
    }
}

fn calculate_platform_movement(
    ground: MoveHitData,
    platform: &ColliderComponentsReadOnlyItem,
    time: &Time,
    ctx: &CtxItem,
) -> Vec3 {
    let ground_com = (platform.rot.0 * platform.com.0) + platform.pos.0;
    let platform_transform = Transform::IDENTITY
        .with_translation(ground_com)
        .with_rotation(platform.rot.0);
    let next_platform_transform = Transform::IDENTITY
        .with_translation(ground_com + platform.lin_vel.0 * time.delta_secs())
        .with_rotation(
            Quat::from_scaled_axis(platform.ang_vel.0 * time.delta_secs()) * platform.rot.0,
        );
    let mut touch_point = ctx.transform.translation;
    touch_point.y = ground.point1.y;

    next_platform_transform.transform_point(
        platform_transform
            .compute_affine()
            .inverse()
            .transform_point3(touch_point),
    ) - touch_point
}

fn friction(time: &Time, ctx: &mut CtxItem) {
    let speed = ctx.velocity.length();
    if speed < 0.001 {
        return;
    }

    let mut drop = 0.0;
    // apply ground friction
    if ctx.state.grounded.is_some() {
        // TODO: read ground's friction
        let surface_friction = 1.0;
        let friction = ctx.cfg.friction_hz * surface_friction;
        let control = f32::max(speed, ctx.cfg.stop_speed);
        drop += control * friction * time.delta_secs();
    }

    let mut new_speed = (speed - drop).max(0.0);
    if new_speed != speed {
        new_speed /= speed;
        ctx.velocity.0 *= new_speed;
    }
}

fn handle_jump(time: &Time, colliders: &Query<ColliderComponents>, ctx: &mut CtxItem) {
    let Some(jump_time) = ctx.input.jumped.clone() else {
        return;
    };
    if (ctx.state.grounded.is_none() && ctx.state.last_ground.elapsed() > ctx.cfg.coyote_time)
        || jump_time.elapsed() > ctx.cfg.jump_input_buffer
    {
        return;
    }
    ctx.input.jumped = None;
    set_grounded(None, colliders, time, ctx);
    ctx.state.last_ground.set_elapsed(ctx.cfg.coyote_time);

    // TODO: read ground's jump factor
    let ground_factor = 1.0;
    // d = 0.5 * g * t^2		- distance traveled with linear accel
    // t = sqrt(2.0 * 45 / g)	- how long to fall 45 units
    // v = g * t				- velocity at the end (just invert it to jump up that high)
    // v = g * sqrt(2.0 * 45 / g )
    // v^2 = g * g * 2.0 * 45 / g
    // v = sqrt( g * 2.0 * 45 )
    let fl_mul = (2.0 * ctx.cfg.gravity * ctx.cfg.jump_height).sqrt();
    ctx.velocity.y = ground_factor * fl_mul;

    // TODO: Trigger jump event
}

#[expect(unused_variables, unreachable_code)]
fn handle_mantle(
    time: &Time,
    colliders: &Query<ColliderComponents>,
    move_and_slide: &MoveAndSlide,
    ctx: &mut CtxItem,
) {
    let Some(mantle_time) = ctx.input.mantled.clone() else {
        return;
    };
    if mantle_time.elapsed() > ctx.cfg.mantle_input_buffer {
        return;
    }
    // High level overview:
    // - move cast up
    // - translate a bit with horizontal movement
    // - move cast down
    // - move cast a bit back
    return;
}

fn start_gravity(time: &Time, ctx: &mut CtxItem) {
    ctx.velocity.y += (ctx.state.base_velocity.y - ctx.cfg.gravity * 0.5) * time.delta_secs();
    ctx.state.base_velocity.y = 0.0;

    validate_velocity(ctx);
}

fn finish_gravity(time: &Time, ctx: &mut CtxItem) {
    ctx.velocity.y -= ctx.cfg.gravity * 0.5 * time.delta_secs();
    validate_velocity(ctx);
}

fn validate_velocity(ctx: &mut CtxItem) {
    for i in 0..3 {
        if !ctx.velocity[i].is_finite() {
            warn!(
                "velocity[{i}] is not finite: {}, setting to 0",
                ctx.velocity[i]
            );
            ctx.velocity[i] = 0.0;
        }
    }
    ctx.velocity.0 = ctx.velocity.clamp_length(0.0, ctx.cfg.max_speed);
}

fn calculate_wish_velocity(orientation: Transform, ctx: &CtxItem) -> Vec3 {
    let movement = ctx.input.last_movement.unwrap_or_default();
    let mut forward = Vec3::from(orientation.forward());
    forward.y = 0.0;
    forward = forward.normalize_or_zero();
    let mut right = Vec3::from(orientation.right());
    right.y = 0.0;
    right = right.normalize_or_zero();

    let wish_vel = movement.y * forward + movement.x * right;
    let wish_dir = wish_vel.normalize_or_zero();

    // clamp the speed lower if ducking
    let speed = if ctx.state.crouching {
        ctx.cfg.speed * ctx.cfg.crouch_speed_scale
    } else {
        ctx.cfg.speed
    };
    wish_dir * speed
}

fn handle_crouching(move_and_slide: &MoveAndSlide, ctx: &mut CtxItem) {
    if ctx.input.crouched {
        ctx.state.crouching = true;
    } else if ctx.state.crouching {
        // try to stand up
        ctx.state.crouching = false;
        let is_intersecting = is_intersecting(move_and_slide, ctx);
        ctx.state.crouching = is_intersecting;
    }
}

#[must_use]
fn is_intersecting(move_and_slide: &MoveAndSlide, ctx: &CtxItem) -> bool {
    let mut intersecting = false;
    // No need to worry about skin width, depenetration will take care of it.
    // If we used skin width, we could not stand up if we are closer than skin width to the ground,
    // which happens when going under a slope.
    move_and_slide.query_pipeline.shape_intersections_callback(
        ctx.state.collider(),
        ctx.transform.translation,
        ctx.transform.rotation,
        &ctx.cfg.filter,
        |_| {
            intersecting = true;
            false
        },
    );
    intersecting
}
