use crate::prelude::*;

use crate::fixed_update_utils::did_fixed_timestep_run_this_frame;

pub(super) fn plugin(app: &mut App) {
    app.add_observer(apply_movement)
        .add_observer(apply_jump)
        .add_observer(apply_crouch)
        .add_systems(
            RunFixedMainLoop,
            clear_accumulated_input
                .run_if(did_fixed_timestep_run_this_frame)
                .in_set(RunFixedMainLoopSystems::AfterFixedMainLoop),
        );
}

#[derive(Debug, InputAction)]
#[action_output(Vec2)]
pub struct Movement;

#[derive(Debug, InputAction)]
#[action_output(bool)]
pub struct Jump;

#[derive(Debug, InputAction)]
#[action_output(bool)]
pub struct Crouch;

#[derive(Debug, InputAction)]
#[action_output(Vec2)]
pub struct RotateCamera;

/// Input accumulated since the last fixed update loop. Is cleared after every fixed update loop.
#[derive(Component, Clone, Copy, Reflect, Default, Debug)]
#[reflect(Component)]
pub struct AccumulatedInput {
    // The last non-zero move that was input since the last fixed update loop
    pub last_movement: Option<Vec2>,
    // Whether any frame since the last fixed update loop input a jump
    pub jumped: bool,
    // Whether any frame since the last fixed update loop input a crouch
    pub crouched: bool,
}

fn apply_movement(
    movement: On<Fire<Movement>>,
    mut accumulated_inputs: Query<&mut AccumulatedInput>,
) {
    if let Ok(mut accumulated_inputs) = accumulated_inputs.get_mut(movement.context) {
        accumulated_inputs.last_movement = Some(movement.value);
    }
}

fn apply_jump(jump: On<Fire<Jump>>, mut accumulated_inputs: Query<&mut AccumulatedInput>) {
    if let Ok(mut accumulated_inputs) = accumulated_inputs.get_mut(jump.context) {
        accumulated_inputs.jumped = true;
    }
}

fn apply_crouch(crouch: On<Fire<Crouch>>, mut accumulated_inputs: Query<&mut AccumulatedInput>) {
    if let Ok(mut accumulated_inputs) = accumulated_inputs.get_mut(crouch.context) {
        accumulated_inputs.crouched = true;
    }
}

fn clear_accumulated_input(mut accumulated_inputs: Query<&mut AccumulatedInput>) {
    for mut accumulated_input in &mut accumulated_inputs {
        *accumulated_input = default();
    }
}
