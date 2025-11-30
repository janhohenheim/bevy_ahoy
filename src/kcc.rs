use bevy_ecs::{intern::Interned, schedule::ScheduleLabel};
use core::time::Duration;
use tracing::warn;

use crate::{CharacterControllerState, input::AccumulatedInput, prelude::*};

pub(super) fn plugin(schedule: Interned<dyn ScheduleLabel>) -> impl Fn(&mut App) {
    move |app: &mut App| {
        app.add_systems(schedule, run_kcc.in_set(AhoySystems::MoveCharacters));
    }
}

#[derive(Debug)]
struct Ctx {
    orientation: Transform,
    cfg: CharacterController,
    input: AccumulatedInput,
    dt: f32,
    dt_duration: Duration,
}

fn run_kcc(
    mut kccs: Query<(
        &CharacterController,
        &mut CharacterControllerState,
        &mut AccumulatedInput,
        &mut Transform,
        &mut LinearVelocity,
        Option<&CharacterControllerCamera>,
    )>,
    cams: Query<&Transform, Without<CharacterController>>,
    time: Res<Time>,
    move_and_slide: MoveAndSlide,
) {
    for (cfg, mut state, mut input, mut transform, mut velocity, cam) in &mut kccs {
        state.touching_entities.clear();
        state.last_ground.tick(time.delta());
        state.last_step_up.tick(time.delta());
        state.last_step_down.tick(time.delta());

        let ctx = Ctx {
            orientation: cam
                .and_then(|e| cams.get(e.get()).copied().ok())
                .unwrap_or(*transform),
            cfg: cfg.clone(),
            input: input.clone(),
            dt: time.delta_secs(),
            dt_duration: time.delta(),
        };
        depenetrate_character(&mut transform, &move_and_slide, &state, &ctx);

        update_grounded(&transform, &velocity, &move_and_slide, &mut state, &ctx);

        handle_crouching(*transform, &move_and_slide, &mut state, &ctx);

        // here we'd handle things like spectator, dead, noclip, etc.
        start_gravity(&mut velocity, &mut state, &ctx);

        handle_jump(&mut velocity, &mut input, &mut state, &ctx);

        // Fricion is handled before we add in any base velocity. That way, if we are on a conveyor,
        //  we don't slow when standing still, relative to the conveyor.
        if state.grounded.is_some() {
            velocity.y = 0.0;
            friction(&mut velocity, &state, &ctx);
        }

        validate_velocity(&mut velocity, &ctx);

        let wish_velocity = calculate_wish_velocity(&state, &ctx);
        if state.grounded.is_some() {
            ground_move(
                &mut transform,
                &mut velocity,
                wish_velocity,
                &move_and_slide,
                &mut state,
                &ctx,
            );
        } else {
            air_move(
                &mut transform,
                &mut velocity,
                wish_velocity,
                &move_and_slide,
                &mut state,
                &ctx,
            );
        }

        update_grounded(&transform, &velocity, &move_and_slide, &mut state, &ctx);
        validate_velocity(&mut velocity, &ctx);

        finish_gravity(&mut velocity, &ctx);

        if state.grounded.is_some() {
            velocity.y = 0.0;
            state.last_ground.reset();
        }
        // TODO: check_falling();
    }
}

fn depenetrate_character(
    transform: &mut Transform,
    move_and_slide: &MoveAndSlide,
    state: &CharacterControllerState,
    ctx: &Ctx,
) {
    let offset = move_and_slide.depenetrate(
        state.collider(),
        transform.translation,
        transform.rotation,
        &((&ctx.cfg.move_and_slide).into()),
        &ctx.cfg.filter,
    );
    transform.translation += offset;
}

fn ground_move(
    transform: &mut Transform,
    velocity: &mut Vec3,
    wish_velocity: Vec3,
    move_and_slide: &MoveAndSlide,
    state: &mut CharacterControllerState,
    ctx: &Ctx,
) {
    let old_grounded = state.grounded;

    velocity.y = 0.0;
    ground_accelerate(velocity, wish_velocity, ctx.cfg.acceleration_hz, ctx);
    velocity.y = 0.0;

    *velocity += state.base_velocity;
    let speed = velocity.length();

    if speed < 0.01 {
        // zero velocity out and remove base
        *velocity = -state.base_velocity;
        return;
    }

    let mut movement = *velocity * ctx.dt;
    movement.y = 0.0;

    let hit = move_and_slide.cast_move(
        state.collider(),
        transform.translation,
        transform.rotation,
        movement,
        ctx.cfg.move_and_slide.skin_width,
        &ctx.cfg.filter,
    );

    if hit.is_none() {
        transform.translation += movement;
        *velocity -= state.base_velocity;
        depenetrate_character(transform, move_and_slide, state, ctx);
        snap_to_ground(transform, move_and_slide, state, ctx);
        return;
    };

    // Don't walk up stairs if not on ground.
    if old_grounded.is_none() && !ctx.cfg.step_from_air {
        *velocity -= state.base_velocity;
        return;
    }

    step_move(transform, velocity, move_and_slide, state, ctx);

    *velocity -= state.base_velocity;
    snap_to_ground(transform, move_and_slide, state, ctx);
}

fn ground_accelerate(velocity: &mut Vec3, wish_velocity: Vec3, acceleration_hz: f32, ctx: &Ctx) {
    let Ok((wish_dir, wish_speed)) = Dir3::new_and_length(wish_velocity) else {
        return;
    };
    let current_speed = velocity.dot(*wish_dir);
    let add_speed = wish_speed - current_speed;

    if add_speed <= 0.0 {
        return;
    }

    // TODO: read this from ground
    let surface_friction = 1.0;
    let accel_speed = wish_speed * acceleration_hz * ctx.dt * surface_friction;
    let accel_speed = f32::min(accel_speed, add_speed);

    *velocity += accel_speed * wish_dir;
}

fn air_move(
    transform: &mut Transform,
    velocity: &mut Vec3,
    wish_velocity: Vec3,
    move_and_slide: &MoveAndSlide,
    state: &mut CharacterControllerState,
    ctx: &Ctx,
) {
    air_accelerate(velocity, wish_velocity, ctx.cfg.air_acceleration_hz, ctx);

    *velocity += state.base_velocity;

    if ctx.cfg.step_from_air {
        step_move(transform, velocity, move_and_slide, state, ctx);
    } else {
        move_character(transform, velocity, move_and_slide, state, ctx);
    }

    *velocity -= state.base_velocity;
}

fn air_accelerate(velocity: &mut Vec3, wish_velocity: Vec3, acceleration_hz: f32, ctx: &Ctx) {
    let Ok((wish_dir, wish_speed)) = Dir3::new_and_length(wish_velocity) else {
        return;
    };
    let wishspd = f32::min(wish_speed, ctx.cfg.max_air_speed);
    let current_speed = velocity.dot(*wish_dir);

    let add_speed = wishspd - current_speed;

    if add_speed <= 0.0 {
        return;
    }

    // TODO: read this from ground
    let surface_friction = 1.0;
    let accel_speed = wish_speed * acceleration_hz * ctx.dt * surface_friction;
    let accel_speed = f32::min(accel_speed, add_speed);

    *velocity += accel_speed * wish_dir;
}

fn step_move(
    transform: &mut Transform,
    velocity: &mut Vec3,
    move_and_slide: &MoveAndSlide,
    state: &mut CharacterControllerState,
    ctx: &Ctx,
) {
    let original_position = transform.translation;
    let original_velocity = *velocity;
    let original_touching_entities = state.touching_entities.clone();

    // Slide the direct path
    move_character(transform, velocity, move_and_slide, state, ctx);

    let down_touching_entities = state.touching_entities.clone();
    let down_position = transform.translation;
    let down_velocity = *velocity;

    transform.translation = original_position;
    *velocity = original_velocity;
    state.touching_entities = original_touching_entities;

    // step up
    let cast_dir = Dir3::Y;
    let cast_len = ctx.cfg.step_size;

    let hit = move_and_slide.cast_move(
        state.collider(),
        transform.translation,
        transform.rotation,
        cast_dir * cast_len,
        ctx.cfg.move_and_slide.skin_width,
        &ctx.cfg.filter,
    );

    let dist = hit.map(|hit| hit.distance).unwrap_or(cast_len);
    transform.translation += cast_dir * dist;

    // Verify we have enough space to stand
    let hit = move_and_slide.cast_move(
        state.collider(),
        transform.translation,
        transform.rotation,
        velocity.normalize_or_zero() * ctx.cfg.min_step_ledge_space,
        ctx.cfg.move_and_slide.skin_width,
        &ctx.cfg.filter,
    );
    if hit.is_some() {
        transform.translation = down_position;
        *velocity = down_velocity;
        state.touching_entities = down_touching_entities;
        return;
    }

    // try to slide from upstairs
    move_character(transform, velocity, move_and_slide, state, ctx);

    let cast_dir = Dir3::NEG_Y;
    let hit = move_and_slide.cast_move(
        state.collider(),
        transform.translation,
        transform.rotation,
        cast_dir * cast_len,
        ctx.cfg.move_and_slide.skin_width,
        &ctx.cfg.filter,
    );

    // If we either fall or slide, use the direct slide instead
    if !hit.is_some_and(|h| h.normal1.y >= ctx.cfg.min_walk_cos || ctx.cfg.step_into_air) {
        transform.translation = down_position;
        *velocity = down_velocity;
        state.touching_entities = down_touching_entities;
        return;
    };
    let hit = hit.unwrap();
    transform.translation += cast_dir * hit.distance;
    depenetrate_character(transform, move_and_slide, state, ctx);

    let vec_up_pos = transform.translation;

    // use the one that went further
    let down_dist = down_position.xz().distance_squared(original_position.xz());
    let up_dist = vec_up_pos.xz().distance_squared(original_position.xz());
    if down_dist >= up_dist {
        transform.translation = down_position;
        *velocity = down_velocity;
        state.touching_entities = down_touching_entities;
    } else {
        velocity.y = down_velocity.y;
        state.last_step_up.reset();
    }
}

fn move_character(
    transform: &mut Transform,
    velocity: &mut Vec3,
    move_and_slide: &MoveAndSlide,
    state: &mut CharacterControllerState,
    ctx: &Ctx,
) {
    let mut config = ctx.cfg.move_and_slide.clone();
    if let Some(grounded) = state.grounded {
        config.planes.push(Dir3::new_unchecked(grounded.normal1));
    }

    let mut touching_entities = std::mem::take(&mut state.touching_entities);
    let out = move_and_slide.move_and_slide(
        state.collider(),
        transform.translation,
        transform.rotation,
        *velocity,
        ctx.dt_duration,
        &config,
        &ctx.cfg.filter,
        |hit| {
            touching_entities.push(hit.into());
            true
        },
    );
    transform.translation = out.position;
    *velocity = out.projected_velocity;
    std::mem::swap(&mut state.touching_entities, &mut touching_entities);
}

fn snap_to_ground(
    transform: &mut Transform,
    move_and_slide: &MoveAndSlide,
    state: &mut CharacterControllerState,
    ctx: &Ctx,
) {
    let cast_dir = Vec3::Y;
    let cast_len = ctx.cfg.ground_distance;

    let hit = move_and_slide.cast_move(
        state.collider(),
        transform.translation,
        transform.rotation,
        cast_dir * cast_len,
        ctx.cfg.move_and_slide.skin_width,
        &ctx.cfg.filter,
    );
    let up_dist = hit.map(|h| h.distance).unwrap_or(cast_len);
    let start = transform.translation + cast_dir * up_dist;
    let cast_dir = Vec3::NEG_Y;
    let cast_len = up_dist + ctx.cfg.step_size;

    let hit = move_and_slide.cast_move(
        state.collider(),
        start,
        transform.rotation,
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
    let original_position = transform.translation;
    transform.translation = start + cast_dir * hit.distance;
    if original_position.y - transform.translation.y > ctx.cfg.step_down_detection_distance {
        state.last_step_down.reset();
    }
    depenetrate_character(transform, move_and_slide, state, ctx);
}

fn update_grounded(
    transform: &Transform,
    velocity: &Vec3,
    move_and_slide: &MoveAndSlide,
    state: &mut CharacterControllerState,
    ctx: &Ctx,
) {
    // TODO: reset surface friction here for some reason? something something water

    let y_vel = velocity.y;
    let moving_up = y_vel > 0.0;
    let mut moving_up_rapidly = y_vel > ctx.cfg.unground_speed;
    if moving_up_rapidly && let Some(_ground) = state.grounded {
        // TODO: get ground abs velocity here
        let ground_entity_y_vel = 0.0;
        moving_up_rapidly = (y_vel - ground_entity_y_vel) > ctx.cfg.unground_speed;
    }

    let is_on_ladder = false;
    if moving_up_rapidly || (moving_up && is_on_ladder) {
        state.grounded = None;
    } else {
        let cast_dir = Dir3::NEG_Y;
        let cast_dist = ctx.cfg.ground_distance;
        let hit = move_and_slide.cast_move(
            state.collider(),
            transform.translation,
            transform.rotation,
            cast_dir * cast_dist,
            ctx.cfg.move_and_slide.skin_width,
            &ctx.cfg.filter,
        );
        if let Some(hit) = hit
            && hit.normal1.y >= ctx.cfg.min_walk_cos
        {
            state.grounded = Some(hit);
        } else {
            state.grounded = None;
            // TODO: set surface friction to 0.25 for some reason
        }
    }
    // TODO: fire ground changed event
}

fn friction(velocity: &mut Vec3, state: &CharacterControllerState, ctx: &Ctx) {
    let speed = velocity.length();
    if speed < 0.001 {
        return;
    }

    let mut drop = 0.0;
    // apply ground friction
    if state.grounded.is_some() {
        // TODO: read ground's friction
        let surface_friction = 1.0;
        let friction = ctx.cfg.friction_hz * surface_friction;
        let control = f32::max(speed, ctx.cfg.stop_speed);
        drop += control * friction * ctx.dt;
    }

    let mut new_speed = (speed - drop).max(0.0);
    if new_speed != speed {
        new_speed /= speed;
        *velocity *= new_speed;
    }
}

fn handle_jump(
    velocity: &mut Vec3,
    input: &mut AccumulatedInput,
    state: &mut CharacterControllerState,
    ctx: &Ctx,
) {
    let Some(jump_time) = input.jumped.clone() else {
        return;
    };
    if (state.grounded.is_none() && state.last_ground.elapsed() > ctx.cfg.coyote_time)
        || jump_time.elapsed() > ctx.cfg.jump_input_buffer
    {
        return;
    }
    input.jumped = None;
    state.grounded = None;
    state.last_ground.set_elapsed(ctx.cfg.coyote_time);

    // TODO: read ground's jump factor
    let ground_factor = 1.0;
    // d = 0.5 * g * t^2		- distance traveled with linear accel
    // t = sqrt(2.0 * 45 / g)	- how long to fall 45 units
    // v = g * t				- velocity at the end (just invert it to jump up that high)
    // v = g * sqrt(2.0 * 45 / g )
    // v^2 = g * g * 2.0 * 45 / g
    // v = sqrt( g * 2.0 * 45 )
    let fl_mul = (2.0 * ctx.cfg.gravity * ctx.cfg.jump_height).sqrt();
    velocity.y = ground_factor * fl_mul;

    // TODO: Trigger jump event
}

fn start_gravity(velocity: &mut Vec3, state: &mut CharacterControllerState, ctx: &Ctx) {
    velocity.y += (state.base_velocity.y - ctx.cfg.gravity * 0.5) * ctx.dt;
    state.base_velocity.y = 0.0;

    validate_velocity(velocity, ctx);
}

fn finish_gravity(velocity: &mut Vec3, ctx: &Ctx) {
    velocity.y -= ctx.cfg.gravity * 0.5 * ctx.dt;
    validate_velocity(velocity, ctx);
}

fn validate_velocity(velocity: &mut Vec3, ctx: &Ctx) {
    for i in 0..3 {
        if !velocity[i].is_finite() {
            warn!("velocity[{i}] is not finite: {}, setting to 0", velocity[i]);
            velocity[i] = 0.0;
        }
    }
    *velocity = velocity.clamp_length(0.0, ctx.cfg.max_speed);
}

fn calculate_wish_velocity(state: &CharacterControllerState, ctx: &Ctx) -> Vec3 {
    let movement = ctx.input.last_movement.unwrap_or_default();
    let mut forward = Vec3::from(ctx.orientation.forward());
    forward.y = 0.0;
    forward = forward.normalize_or_zero();
    let mut right = Vec3::from(ctx.orientation.right());
    right.y = 0.0;
    right = right.normalize_or_zero();

    let wish_vel = movement.y * forward + movement.x * right;
    let wish_dir = wish_vel.normalize_or_zero();

    // clamp the speed lower if ducking
    let speed = if state.crouching {
        ctx.cfg.speed * ctx.cfg.crouch_speed_scale
    } else {
        ctx.cfg.speed
    };
    wish_dir * speed
}

fn handle_crouching(
    transform: Transform,
    move_and_slide: &MoveAndSlide,
    state: &mut CharacterControllerState,
    ctx: &Ctx,
) {
    if ctx.input.crouched {
        state.crouching = true;
    } else if state.crouching {
        // try to stand up
        state.crouching = false;
        let is_intersecting = is_intersecting(transform, state, move_and_slide, ctx);
        state.crouching = is_intersecting;
    }
}

#[must_use]
fn is_intersecting(
    transform: Transform,
    state: &CharacterControllerState,
    move_and_slide: &MoveAndSlide,
    ctx: &Ctx,
) -> bool {
    let mut intersecting = false;
    // No need to worry about skin width, depenetration will take care of it.
    // If we used skin width, we could not stand up if we are closer than skin width to the ground,
    // which happens when going under a slope.
    move_and_slide.query_pipeline.shape_intersections_callback(
        state.collider(),
        transform.translation,
        transform.rotation,
        &ctx.cfg.filter,
        |_| {
            intersecting = true;
            false
        },
    );
    intersecting
}
