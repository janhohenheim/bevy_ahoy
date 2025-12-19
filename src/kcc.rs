use avian3d::character_controller::move_and_slide::MoveHitData;
use bevy_ecs::{
    intern::Interned,
    query::QueryData,
    schedule::ScheduleLabel,
    system::lifetimeless::{Read, Write},
};
use core::fmt::Debug;
use core::time::Duration;
use tracing::warn;

use crate::{CharacterControllerState, MantleProgress, input::AccumulatedInput, prelude::*};

pub(super) fn plugin(schedule: Interned<dyn ScheduleLabel>) -> impl Fn(&mut App) {
    move |app: &mut App| {
        app.add_systems(schedule, run_kcc.in_set(AhoySystems::MoveCharacters))
            .add_systems(Update, spin_cams);
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
    water: Read<WaterState>,
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
    waters: Query<Entity, With<Water>>,
) {
    let mut colliders = colliders.transmute_lens_inner();
    let colliders = colliders.query();
    let mut waters = waters.transmute_lens_inner();
    let waters = waters.query();
    for mut ctx in &mut kccs {
        ctx.state.touching_entities.clear();
        ctx.state.last_ground.tick(time.delta());
        ctx.state.last_tac.tick(time.delta());
        ctx.state.last_step_up.tick(time.delta());
        ctx.state.last_step_down.tick(time.delta());

        depenetrate_character(&move_and_slide, &mut ctx);
        update_grounded(&move_and_slide, &colliders, &time, &mut ctx);

        handle_crouching(&move_and_slide, &waters, &mut ctx);

        if ctx.water.level <= WaterLevel::Feet {
            // here we'd handle things like spectator, dead, noclip, etc.
            start_gravity(&time, &mut ctx);
        }

        ctx.state.orientation = ctx
            .cam
            .and_then(|e| Option::<&Transform>::copied(cams.get(e.get()).ok()))
            .unwrap_or(*ctx.transform);

        let wish_velocity = calculate_wish_velocity(&ctx);
        let wish_velocity_3d = calculate_3d_wish_velocity(&ctx);
        update_crane_state(wish_velocity, &time, &move_and_slide, &mut ctx);
        update_mantle_state(wish_velocity, &time, &move_and_slide, &mut ctx);
        if ctx.state.crane_height_left.is_some() {
            handle_crane_movement(wish_velocity, &time, &move_and_slide, &mut ctx);
        } else if ctx.state.mantle_progress.is_some() {
            handle_jump(wish_velocity, &time, &colliders, &move_and_slide, &mut ctx);
            handle_mantle_movement(
                wish_velocity_3d,
                &time,
                &move_and_slide,
                &colliders,
                &mut ctx,
            );
        } else {
            handle_jump(wish_velocity, &time, &colliders, &move_and_slide, &mut ctx);

            // Friction is handled before we add in any base velocity. That way, if we are on a conveyor,
            //  we don't slow when standing still, relative to the conveyor.
            friction(&time, &mut ctx);

            validate_velocity(&mut ctx);

            if ctx.water.level > WaterLevel::Feet {
                water_move(wish_velocity_3d, &time, &move_and_slide, &mut ctx);
            } else if ctx.state.grounded.is_some() {
                ground_move(wish_velocity, &time, &move_and_slide, &mut ctx);
            } else {
                air_move(wish_velocity, &time, &move_and_slide, &mut ctx);
            }
        }

        update_grounded(&move_and_slide, &colliders, &time, &mut ctx);
        validate_velocity(&mut ctx);

        if ctx.water.level <= WaterLevel::Feet {
            finish_gravity(&time, &mut ctx);
        }

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

    let hit = cast_move(movement, move_and_slide, ctx);

    if hit.is_none() {
        ctx.transform.translation += movement;
        ctx.velocity.0 -= ctx.state.base_velocity;
        depenetrate_character(move_and_slide, ctx);
        snap_to_ground(move_and_slide, ctx);
        return;
    };

    step_move(time, move_and_slide, ctx);

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

    step_move(time, move_and_slide, ctx);

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

fn water_move(
    mut wish_velocity: Vec3,
    time: &Time,
    move_and_slide: &MoveAndSlide,
    ctx: &mut CtxItem,
) {
    if ctx.input.swim_up {
        ctx.input.swim_up = false;
        wish_velocity += Vec3::Y * ctx.cfg.speed;
    };
    // Avoid Space + W + Look up to go faster than either alone
    wish_velocity = wish_velocity.clamp_length_max(ctx.cfg.speed);
    if wish_velocity == Vec3::ZERO {
        wish_velocity -= Vec3::Y * ctx.cfg.water_gravity;
    };
    wish_velocity *= ctx.cfg.water_slowdown;

    water_accelerate(wish_velocity, ctx.cfg.water_acceleration_hz, time, ctx);
    ctx.velocity.0 += ctx.state.base_velocity;

    step_move(time, move_and_slide, ctx);

    ctx.velocity.0 -= ctx.state.base_velocity;
}

fn water_accelerate(wish_velocity: Vec3, acceleration_hz: f32, time: &Time, ctx: &mut CtxItem) {
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

fn step_move(time: &Time, move_and_slide: &MoveAndSlide, ctx: &mut CtxItem) {
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
    let cast_len = ctx.cfg.step_size;

    let hit = cast_move(cast_dir * cast_len, move_and_slide, ctx);

    let dist = hit.map(|hit| hit.distance).unwrap_or(cast_len);
    ctx.transform.translation += cast_dir * dist;

    // Verify we have enough space to stand
    let hit = cast_move(
        ctx.velocity.normalize_or_zero() * ctx.cfg.min_step_ledge_space,
        move_and_slide,
        ctx,
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
    let hit = cast_move(cast_dir * cast_len, move_and_slide, ctx);

    // If we either fall or slide down, use the direct move-and-slide instead
    if !hit.is_some_and(|h| h.normal1.y >= ctx.cfg.min_walk_cos) {
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

fn handle_crane_movement(
    wish_velocity: Vec3,
    time: &Time,
    move_and_slide: &MoveAndSlide,
    ctx: &mut CtxItem,
) {
    let Some(crane_height) = ctx.state.crane_height_left else {
        return;
    };
    ctx.velocity.y = 0.0;
    ground_accelerate(wish_velocity, ctx.cfg.acceleration_hz, time, ctx);
    ctx.velocity.y = 0.0;
    ctx.velocity.0 += ctx.state.base_velocity;

    let Ok((vel_dir, speed)) = Dir3::new_and_length(ctx.velocity.0) else {
        ctx.state.crane_height_left = None;
        ctx.velocity.0 -= ctx.state.base_velocity;
        return;
    };

    let wish_dir = if let Ok(wish_dir) = Dir3::new(wish_velocity) {
        wish_dir
    } else {
        vel_dir
    };
    ctx.velocity.0 -= ctx.state.base_velocity;
    // Check wall
    let cast_dir = wish_dir;
    let cast_len = ctx.cfg.min_crane_ledge_space;
    let Some(wall_hit) = cast_move(cast_dir * cast_len, move_and_slide, ctx) else {
        // nothing to move onto
        ctx.state.crane_height_left = None;
        return;
    };
    let wall_normal = vec3(wall_hit.normal1.x, 0.0, wall_hit.normal1.z).normalize_or_zero();

    if (-wall_normal).dot(*wish_dir) < ctx.cfg.min_crane_cos {
        ctx.state.crane_height_left = None;
        return;
    }

    let cast_dir = Vec3::Y;
    let cast_len = (ctx.cfg.crane_speed * time.delta_secs()).min(crane_height);
    let top_hit = cast_move(cast_dir * cast_len, move_and_slide, ctx);
    let travel_dist = top_hit.map(|hit| hit.distance).unwrap_or(cast_len);

    ctx.transform.translation += cast_dir * travel_dist;
    let velocity_stash = ctx.velocity.0;
    **ctx.velocity = ctx.state.base_velocity;
    move_character(time, move_and_slide, ctx);
    **ctx.velocity = velocity_stash;

    *ctx.state.crane_height_left.as_mut().unwrap() = if top_hit.is_some() {
        0.0
    } else {
        (crane_height - travel_dist).max(0.0)
    };
    ctx.state.last_step_up.reset();

    if ctx.state.crane_height_left.unwrap() != 0.0 {
        let cast_dir = vel_dir;
        let cast_len = ctx.cfg.min_crane_ledge_space;
        if cast_move(cast_dir * cast_len, move_and_slide, ctx).is_none() {
            ctx.transform.translation += cast_dir * speed * time.delta_secs();
            depenetrate_character(move_and_slide, ctx);
            ctx.state.crane_height_left = None;
        }
        return;
    }

    let cast_dir = vel_dir;
    let cast_len = ctx.cfg.min_crane_ledge_space;
    if cast_move(cast_dir * cast_len, move_and_slide, ctx).is_some() {
        ctx.state.crane_height_left = None;
        return;
    }
    ctx.transform.translation += cast_dir * speed * time.delta_secs();
    depenetrate_character(move_and_slide, ctx);
    ctx.state.crane_height_left = None;
}

fn handle_mantle_movement(
    wish_velocity: Vec3,
    time: &Time,
    move_and_slide: &MoveAndSlide,
    colliders: &Query<ColliderComponents>,
    ctx: &mut CtxItem,
) {
    let Some(mantle) = ctx.state.mantle_progress else {
        return;
    };

    ctx.velocity.0 = Vec3::ZERO;
    if wish_velocity.length_squared() < 0.001 {
        return;
    };
    let Some((_wall_point, wall_normal)) =
        closest_wall_normal(ctx.cfg.max_ledge_grab_distance, move_and_slide, ctx)
    else {
        ctx.state.mantle_progress = None;
        return;
    };
    let Some(hit) = cast_move(
        -wall_normal * ctx.cfg.max_ledge_grab_distance,
        move_and_slide,
        ctx,
    ) else {
        ctx.state.mantle_progress = None;
        return;
    };

    {
        let progress = ctx.state.mantle_progress.as_mut().unwrap();
        progress.wall_normal = wall_normal;
        progress.ledge_position = hit.point1;
        progress.wall_entity = hit.entity;
        if let Ok(platform) = colliders.get(progress.wall_entity) {
            calculate_platform_movement(mantle.ledge_position, &platform, time, ctx);
        }
    }

    let climb_dir = Vec3::Y;
    let wish_y = calc_climb_factor(ctx);

    let mut climb_dist =
        (ctx.cfg.mantle_speed * time.delta_secs() * wish_y).min(mantle.height_left);
    if mantle.height_left - climb_dist
        > ctx.cfg.mantle_height - ctx.cfg.min_ledge_grab_space.size().y
    {
        climb_dist =
            mantle.height_left - ctx.cfg.mantle_height + ctx.cfg.min_ledge_grab_space.size().y;
    }

    let top_hit = cast_move(climb_dir * climb_dist, move_and_slide, ctx);
    let travel_dist =
        top_hit.map(|hit| hit.distance).unwrap_or(climb_dist.abs()) * climb_dist.signum();

    ctx.velocity.0 = climb_dir * travel_dist / time.delta_secs() + ctx.state.base_velocity;
    move_character(time, move_and_slide, ctx);
    ctx.velocity.0 -= ctx.state.base_velocity;

    ctx.state.mantle_progress.as_mut().unwrap().height_left = mantle.height_left - travel_dist;
    if climb_dist > 0.0 {
        ctx.state.last_step_up.reset();
    } else {
        ctx.state.last_step_down.reset();
    }
}

fn calc_climb_factor(ctx: &CtxItem) -> f32 {
    // positive when looking at the wall or above it, negative when looking down
    let movement = ctx.input.last_movement.unwrap_or_default().y;
    let cos = (ctx.state.orientation.forward() * movement.abs()).y;
    let factor = ((cos + ctx.cfg.climb_reverse_sin) * ctx.cfg.climb_sensitivity).clamp(-1.0, 1.0);
    if movement < 0.0 { -factor } else { factor }
}

fn update_crane_state(
    wish_velocity: Vec3,
    time: &Time,
    move_and_slide: &MoveAndSlide,
    ctx: &mut CtxItem,
) {
    let Some(crane_time) = ctx.input.craned.clone() else {
        return;
    };
    if crane_time.elapsed() > ctx.cfg.crane_input_buffer {
        return;
    }

    let Some(crane_height) = available_crane_height(wish_velocity, time, move_and_slide, ctx)
    else {
        ctx.state.crane_height_left = None;
        return;
    };

    ctx.input.craned = None;
    // Ensure we don't immediately jump on the surface if crane and jump are bound to the same key
    ctx.input.jumped = None;
    ctx.input.mantled = None;
    ctx.input.tac = None;

    ctx.state.mantle_progress = None;
    ctx.state.crane_height_left = Some(crane_height);
}

fn available_crane_height(
    wish_velocity: Vec3,
    time: &Time,
    move_and_slide: &MoveAndSlide,
    ctx: &mut CtxItem,
) -> Option<f32> {
    available_ledge_height(
        wish_velocity,
        ctx.cfg.min_crane_ledge_space,
        ctx.cfg.min_crane_cos,
        ctx.cfg.crane_height,
        time,
        move_and_slide,
        ctx,
    )
}

fn available_ledge_height(
    wish_velocity: Vec3,
    min_depth: f32,
    min_cos: f32,
    max_height: f32,
    time: &Time,
    move_and_slide: &MoveAndSlide,
    ctx: &mut CtxItem,
) -> Option<f32> {
    let original_position = ctx.transform.translation;
    let original_velocity = ctx.velocity.0;

    let wish_dir = if let Ok(wish_dir) = Dir3::new(wish_velocity) {
        wish_dir
    } else if let Ok(vel_dir) = Dir3::new(vec3(ctx.velocity.0.x, 0.0, ctx.velocity.0.z)) {
        vel_dir
    } else {
        ctx.velocity.0 = original_velocity;
        return None;
    };

    ctx.velocity.y = 0.0;
    ground_accelerate(wish_velocity, ctx.cfg.acceleration_hz, time, ctx);
    ctx.velocity.y = 0.0;
    ctx.velocity.0 += ctx.state.base_velocity;

    // Check wall
    let cast_dir = wish_dir;
    let cast_len = min_depth;
    let Some(wall_hit) = cast_move(cast_dir * cast_len, move_and_slide, ctx) else {
        // nothing to move onto
        ctx.velocity.0 = original_velocity;
        return None;
    };
    let wall_normal = vec3(wall_hit.normal1.x, 0.0, wall_hit.normal1.z).normalize_or_zero();

    if (-wall_normal).dot(*wish_dir) < min_cos {
        ctx.velocity.0 = original_velocity;
        return None;
    }

    // step up
    let cast_dir = Dir3::Y;
    let cast_len = max_height;

    let hit = cast_move(cast_dir * cast_len, move_and_slide, ctx);

    let up_dist = hit.map(|hit| hit.distance).unwrap_or(cast_len);
    ctx.transform.translation += cast_dir * up_dist;

    // Move onto ledge (penetration explicitly allowed since the ledge can be below a wall)
    ctx.transform.translation += -wall_normal * min_depth;

    // Move down
    let cast_dir = Dir3::NEG_Y;
    let cast_len = up_dist;
    let Some(down_dist) =
        cast_move(cast_dir * cast_len, move_and_slide, ctx).map(|hit| hit.distance)
    else {
        ctx.transform.translation = original_position;
        ctx.velocity.0 = original_velocity;
        return None;
    };
    let ledge_height = up_dist - down_dist;

    // Okay, we found a potentially ledge!
    ctx.transform.translation = original_position;

    // step up
    ctx.transform.translation.y += ledge_height;

    // check the full climb

    // make sure we have enough space to land
    let cast_dir = -wall_normal;
    let cast_len = min_depth;
    if cast_move(cast_dir * cast_len, move_and_slide, ctx).is_some() {
        ctx.transform.translation = original_position;
        ctx.velocity.0 = original_velocity;
        return None;
    };
    ctx.transform.translation += cast_dir * cast_len;

    let cast_dir = Dir3::NEG_Y;
    let cast_len = ledge_height;
    let hit = cast_move(cast_dir * cast_len, move_and_slide, ctx);

    // If this doesn't hit, our climb was actually going through geometry. Bail.
    let Some(hit) = hit else {
        ctx.transform.translation = original_position;
        ctx.velocity.0 = original_velocity;
        return None;
    };
    if hit.normal1.y < ctx.cfg.min_walk_cos {
        ctx.transform.translation = original_position;
        ctx.velocity.0 = original_velocity;
        return None;
    }

    // Reset KCC from speculative climb to actual current state
    ctx.transform.translation = original_position;
    ctx.velocity.0 = original_velocity;

    Some(ledge_height)
}

fn update_mantle_state(
    wish_velocity: Vec3,
    time: &Time,
    move_and_slide: &MoveAndSlide,
    ctx: &mut CtxItem,
) {
    if ctx.state.crane_height_left.is_some() {
        ctx.state.mantle_progress = None;
        return;
    }
    if ctx.state.mantle_progress.is_some() {
        return;
    }

    let Some(mantle_time) = ctx.input.mantled.clone() else {
        return;
    };
    if mantle_time.elapsed() > ctx.cfg.mantle_input_buffer {
        return;
    }

    let Some(mantle_height) = available_mantle_height(wish_velocity, time, move_and_slide, ctx)
    else {
        return;
    };

    ctx.input.craned = None;
    ctx.input.mantled = None;
    // Ensure we don't immediately jump on the surface if mantle and jump are bound to the same key
    ctx.input.jumped = None;

    ctx.state.mantle_progress = Some(mantle_height);
}

fn available_mantle_height(
    wish_velocity: Vec3,
    time: &Time,
    move_and_slide: &MoveAndSlide,
    ctx: &mut CtxItem,
) -> Option<MantleProgress> {
    let original_position = ctx.transform.translation;
    let original_velocity = ctx.velocity.0;

    let wish_dir = if let Ok(wish_dir) = Dir3::new(wish_velocity) {
        wish_dir
    } else if let Ok(fwd) = Dir3::new(vec3(
        ctx.state.orientation.forward().x,
        0.0,
        ctx.state.orientation.forward().z,
    )) {
        fwd
    } else {
        return None;
    };

    ctx.velocity.y = 0.0;
    ground_accelerate(wish_velocity, ctx.cfg.acceleration_hz, time, ctx);
    ctx.velocity.y = 0.0;
    ctx.velocity.0 += ctx.state.base_velocity;

    // Check wall
    let cast_dir = wish_dir;
    let cast_len = ctx.cfg.max_ledge_grab_distance;
    let Some(wall_hit) = cast_move(cast_dir * cast_len, move_and_slide, ctx) else {
        // nothing to move onto
        ctx.velocity.0 = original_velocity;
        return None;
    };
    let wall_normal = Dir3::new_unchecked(wall_hit.normal1);

    if (-wall_normal).dot(*wish_dir) < ctx.cfg.min_mantle_cos {
        ctx.velocity.0 = original_velocity;
        return None;
    }

    ctx.transform.translation += cast_dir * wall_hit.distance;
    depenetrate_character(move_and_slide, ctx);
    let wall_pos = ctx.transform.translation;

    // step up
    let cast_dir = Dir3::Y;
    let cast_len = ctx.cfg.mantle_height;

    let up_dist = cast_move_hands(cast_dir * cast_len, move_and_slide, ctx)
        .map(|hit| hit.distance)
        .unwrap_or(cast_len);
    ctx.transform.translation += cast_dir * up_dist;

    let radius = ctx.state.radius();
    let hand_to_wall_dist =
        radius + ctx.cfg.move_and_slide.skin_width + ctx.cfg.min_ledge_grab_space.half_size.z;
    // Move onto ledge (penetration explicitly allowed since the ledge can be below a wall)
    ctx.transform.translation += -wall_normal * hand_to_wall_dist;

    // Move down
    let cast_dir = Dir3::NEG_Y;
    let cast_len = up_dist;
    let Some(down_dist) =
        cast_move_hands(cast_dir * cast_len, move_and_slide, ctx).map(|hit| hit.distance)
    else {
        ctx.transform.translation = original_position;
        ctx.velocity.0 = original_velocity;
        return None;
    };
    ctx.transform.translation += cast_dir * down_dist;

    let ledge_height = up_dist - down_dist;

    // Okay, we found a potential mantle!
    ctx.transform.translation = wall_pos;

    // step up
    ctx.transform.translation.y += ledge_height;

    // check the full mantle

    // make sure we have enough space to land
    let cast_dir = -wall_normal;
    let cast_len = hand_to_wall_dist;
    if cast_move_hands(cast_dir * cast_len, move_and_slide, ctx).is_some() {
        ctx.transform.translation = original_position;
        ctx.velocity.0 = original_velocity;
        return None;
    };
    ctx.transform.translation += cast_dir * cast_len;

    let cast_dir = Dir3::NEG_Y;
    let cast_len = ledge_height;
    let hit = cast_move_hands(cast_dir * cast_len, move_and_slide, ctx);

    // Reset KCC from speculative mantle to actual current state
    ctx.transform.translation = original_position;
    ctx.velocity.0 = original_velocity;

    // If this doesn't hit, our mantle was actually going through geometry. Bail.
    let hit = hit?;
    if hit.normal1.y < ctx.cfg.min_walk_cos {
        return None;
    }

    let kcc_height = ctx.state.pos_to_head_dist();
    let mantle_height = ledge_height - kcc_height + ctx.cfg.climb_pull_up_height;

    if mantle_height < 0.0 {
        return None;
    }

    Some(MantleProgress {
        wall_normal,
        ledge_position: hit.point1,
        height_left: mantle_height,
        wall_entity: hit.entity,
    })
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
    let lost_velocity = (ctx.velocity.0 - out.projected_velocity).length();
    ctx.state.tac_velocity = ctx.state.tac_velocity * 0.99 + lost_velocity;
    ctx.transform.translation = out.position;
    ctx.velocity.0 = out.projected_velocity;
    std::mem::swap(&mut ctx.state.touching_entities, &mut touching_entities);
}

fn snap_to_ground(move_and_slide: &MoveAndSlide, ctx: &mut CtxItem) {
    let cast_dir = Vec3::Y;
    let cast_len = ctx.cfg.ground_distance;

    let hit = cast_move(cast_dir * cast_len, move_and_slide, ctx);
    let up_dist = hit.map(|h| h.distance).unwrap_or(cast_len);
    let start = ctx.transform.translation + cast_dir * up_dist;
    let cast_dir = Vec3::NEG_Y;
    let cast_len = up_dist + ctx.cfg.step_size;

    let orig_pos = ctx.transform.translation;

    ctx.transform.translation = start;
    let hit = cast_move(cast_dir * cast_len, move_and_slide, ctx);
    ctx.transform.translation = orig_pos;

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

fn closest_wall_normal(
    dist: f32,
    move_and_slide: &MoveAndSlide,
    ctx: &CtxItem,
) -> Option<(Vec3, Dir3)> {
    let mut closest_wall: Option<(ContactPoint, Dir3)> = None;
    move_and_slide.intersections(
        ctx.state.collider(),
        ctx.transform.translation,
        ctx.transform.rotation,
        dist + ctx.cfg.move_and_slide.skin_width,
        &ctx.cfg.filter,
        |contact_point, normal| {
            if normal.y.abs() < ctx.cfg.min_walk_cos
                && !closest_wall.is_some_and(|(p, _)| p.penetration < contact_point.penetration)
            {
                closest_wall = Some((*contact_point, normal));
            }
            true
        },
    );
    closest_wall.map(|(p, normal)| (p.point, normal))
}

fn update_grounded(
    move_and_slide: &MoveAndSlide,
    colliders: &Query<ColliderComponents>,
    time: &Time,
    ctx: &mut CtxItem,
) {
    if ctx.water.level > WaterLevel::Feet {
        set_grounded(None, colliders, time, ctx);
        return;
    }
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
        let hit = cast_move(cast_dir * cast_dist, move_and_slide, ctx);
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

#[must_use]
fn cast_move(movement: Vec3, move_and_slide: &MoveAndSlide, ctx: &CtxItem) -> Option<MoveHitData> {
    move_and_slide.cast_move(
        ctx.state.collider(),
        ctx.transform.translation,
        ctx.transform.rotation,
        movement,
        ctx.cfg.move_and_slide.skin_width,
        &ctx.cfg.filter,
    )
}

#[must_use]
fn cast_move_hands(
    movement: Vec3,
    move_and_slide: &MoveAndSlide,
    ctx: &CtxItem,
) -> Option<MoveHitData> {
    move_and_slide.cast_move(
        &ctx.state.hand_collider,
        ctx.transform.translation,
        ctx.transform.rotation,
        movement,
        ctx.cfg.move_and_slide.skin_width,
        &ctx.cfg.filter,
    )
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
        calculate_platform_movement(old_ground.point1, &platform, time, ctx);
    } else if let Some(new_ground) = new_ground
        && let Ok(platform) = colliders.get(new_ground.entity)
    {
        calculate_platform_movement(new_ground.point1, &platform, time, ctx);
    }

    ctx.state.grounded = new_ground;
    if ctx.state.grounded.is_some() {
        ctx.state.mantle_progress = None;
    }

    if ctx.state.grounded.is_some() {
        ctx.velocity.y = 0.0;
    }
}

fn calculate_platform_movement(
    ground: Vec3,
    platform: &ColliderComponentsReadOnlyItem,
    time: &Time,
    ctx: &mut CtxItem,
) {
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
    touch_point.y = ground.y;

    let platform_movement = next_platform_transform.transform_point(
        platform_transform
            .compute_affine()
            .inverse()
            .transform_point3(touch_point),
    ) - touch_point;

    ctx.state.base_velocity = platform_movement / time.delta_secs();
    ctx.state.base_angular_velocity = platform.ang_vel.0;
}

fn friction(time: &Time, ctx: &mut CtxItem) {
    let speed = if ctx.state.grounded.is_some() {
        ctx.velocity.xz().length()
    } else if ctx.water.level > WaterLevel::Feet {
        ctx.velocity.length()
    } else {
        return;
    };
    if speed < 0.001 {
        return;
    }

    let mut drop = 0.0;
    // apply ground friction
    // TODO: read ground's friction
    let surface_friction = 1.0;
    let friction = ctx.cfg.friction_hz * surface_friction;
    let control = f32::max(speed, ctx.cfg.stop_speed);
    drop += control * friction * time.delta_secs();

    let mut new_speed = (speed - drop).max(0.0);
    if new_speed != speed {
        new_speed /= speed;
        ctx.velocity.0 *= new_speed;
    }
}

fn handle_tac(
    wish_velocity: Vec3,
    time: &Time,
    move_and_slide: &MoveAndSlide,
    ctx: &mut CtxItem,
) -> Option<Vec3> {
    if ctx.state.mantle_progress.is_some() {
        return None;
    }
    let tac_time = ctx.input.tac.clone()?;
    if tac_time.elapsed() > ctx.cfg.tac_input_buffer {
        return None;
    }
    if wish_velocity.length_squared() < 0.1 || ctx.state.last_tac.elapsed() < ctx.cfg.tac_cooldown {
        return None;
    }
    let normal = if let Some(hit) =
        cast_move(ctx.velocity.0 * time.delta_secs(), move_and_slide, ctx)
    {
        hit.normal1
    } else if let Some(hit) = cast_move(wish_velocity * time.delta_secs(), move_and_slide, ctx) {
        hit.normal1
    } else {
        // No wall to tic tac off of, we're in free-fall.
        return None;
    };
    // Don't tac off of ceilings/overhangs
    if normal.y < -0.01 {
        return None;
    }
    let wish_unit = wish_velocity.normalize();
    let wish_dot = wish_unit.dot(normal);
    if -wish_dot > ctx.cfg.max_tac_cos {
        return None;
    }
    // Cancel velocity that would be lost to move_and_slide if tac is buffered
    let vel_dot = ctx.velocity.0.dot(normal).min(0.0);
    ctx.velocity.0 -= vel_dot * normal;
    let groundedness = ctx.state.tac_velocity.max(vel_dot).min(1.0);
    ctx.state.tac_velocity = 0.0;
    let flat_normal = Vec3::new(normal.x, 0.0, normal.z);
    let tac_wish = wish_unit - (wish_dot.min(0.0) - 1.0) * flat_normal;
    let tac_dir = (Vec3::Y * ctx.cfg.tac_jump_factor + tac_wish).normalize();
    Some(tac_dir * groundedness * ctx.cfg.tac_power)
}

fn handle_ledge_jump_dir(ctx: &mut CtxItem) -> Option<Vec3> {
    if ctx.state.mantle_progress.is_none()
        || ctx
            .input
            .mantled
            .as_ref()
            .is_some_and(|m| m.elapsed() < ctx.cfg.mantle_input_buffer)
        || ctx.input.jumped.is_none()
    {
        return None;
    }
    let fwd = ctx.state.orientation.forward();
    let flat_fwd = Dir3::new(vec3(fwd.x, 0.0, fwd.z)).ok()?;
    let tac_dir = if ctx.input.last_movement.unwrap_or_default().y >= 0.0 {
        Dir3::new(Vec3::Y * ctx.cfg.ledge_jump_factor + *flat_fwd).ok()?
    } else {
        Dir3::NEG_Y
    };
    ctx.state.mantle_progress = None;
    Some(tac_dir * ctx.cfg.ledge_jump_power)
}

fn handle_jump(
    wish_velocity: Vec3,
    time: &Time,
    colliders: &Query<ColliderComponents>,
    move_and_slide: &MoveAndSlide,
    ctx: &mut CtxItem,
) {
    // Handle tic tacs when we're in the air beyond coyote-time.
    let jumpdir =
        if ctx.state.grounded.is_none() && ctx.state.last_ground.elapsed() > ctx.cfg.coyote_time {
            if let Some(tac_dir) = handle_tac(wish_velocity, time, move_and_slide, ctx) {
                tac_dir
            } else if let Some(ledge_jump_dir) = handle_ledge_jump_dir(ctx) {
                ledge_jump_dir
            } else {
                return;
            }
        } else {
            let Some(jump_time) = ctx.input.jumped.clone() else {
                return;
            };
            if jump_time.elapsed() > ctx.cfg.jump_input_buffer {
                return;
            }
            set_grounded(None, colliders, time, ctx);
            // set last_ground to coyote time to make it not jump again after jumping ungrounds us
            ctx.state.last_ground.set_elapsed(ctx.cfg.coyote_time);
            Vec3::Y
        };
    ctx.state.last_tac.reset();

    ctx.input.jumped = None;
    ctx.input.tac = None;

    // TODO: read ground's jump factor
    let ground_factor = 1.0;
    // d = 0.5 * g * t^2		- distance traveled with linear accel
    // t = sqrt(2.0 * 45 / g)	- how long to fall 45 units
    // v = g * t				- velocity at the end (just invert it to jump up that high)
    // v = g * sqrt(2.0 * 45 / g )
    // v^2 = g * g * 2.0 * 45 / g
    // v = sqrt( g * 2.0 * 45 )
    let fl_mul = (2.0 * ctx.cfg.gravity * ctx.cfg.jump_height).sqrt();
    ctx.velocity.0 += jumpdir * ground_factor * fl_mul + Vec3::Y * ctx.state.base_velocity.y;
    if let Some(crane_input) = ctx.input.craned.as_mut() {
        crane_input
            .tick((ctx.cfg.crane_input_buffer - ctx.cfg.jump_crane_chain_time).max(Duration::ZERO));
    }

    // TODO: Trigger jump event
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

#[must_use]
fn calculate_wish_velocity(ctx: &CtxItem) -> Vec3 {
    let movement = ctx.input.last_movement.unwrap_or_default();
    let mut forward = Vec3::from(ctx.state.orientation.forward());
    forward.y = 0.0;
    forward = forward.normalize_or_zero();
    let mut right = Vec3::from(ctx.state.orientation.right());
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

#[must_use]
fn calculate_3d_wish_velocity(ctx: &CtxItem) -> Vec3 {
    let movement = ctx.input.last_movement.unwrap_or_default();
    let forward = ctx.state.orientation.forward();
    let right = ctx.state.orientation.right();

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

fn handle_crouching(move_and_slide: &MoveAndSlide, waters: &Query<Entity>, ctx: &mut CtxItem) {
    if ctx.input.crouched {
        ctx.state.crouching = true;
    } else if ctx.state.crouching {
        // try to stand up
        ctx.state.crouching = false;
        let is_intersecting = is_intersecting(move_and_slide, waters, ctx);
        ctx.state.crouching = is_intersecting;
    }
}

#[must_use]
fn is_intersecting(move_and_slide: &MoveAndSlide, waters: &Query<Entity>, ctx: &CtxItem) -> bool {
    let mut intersecting = false;
    // No need to worry about skin width, depenetration will take care of it.
    // If we used skin width, we could not stand up if we are closer than skin width to the ground,
    // which happens when going under a slope.
    move_and_slide.query_pipeline.shape_intersections_callback(
        ctx.state.collider(),
        ctx.transform.translation,
        ctx.transform.rotation,
        &ctx.cfg.filter,
        |e| {
            if waters.contains(e) {
                return true;
            }
            intersecting = true;
            false
        },
    );
    intersecting
}

// TODO: this should rotate the KCC, not the cams. The cams can then inherit that rotation inside the camera controller module.
fn spin_cams(
    kccs: Query<Ctx>,
    mut cams: Query<&mut Transform, Without<CharacterController>>,
    time: Res<Time>,
) {
    for ctx in &kccs {
        if ctx.state.grounded.is_some()
            && let Some(mut cam) = ctx.cam.and_then(|cam| cams.get_mut(cam.get()).ok())
        {
            cam.rotate_axis(
                Dir3::Y,
                ctx.state.base_angular_velocity.y * time.delta_secs(),
            );
        }
    }
}
