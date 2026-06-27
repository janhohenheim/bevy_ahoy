use avian_pickup::{Holding, actor::AvianPickupActorState};
use bevy_ecs::relationship::Relationship as _;

use crate::prelude::*;

pub struct AhoyPickupGluePlugin;

impl Plugin for AhoyPickupGluePlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(filter_out_picked_up_prop)
            .add_observer(filter_in_unpicked_prop);
    }
}

fn filter_out_picked_up_prop(
    insert: On<Insert, Holding>,
    pickup_actor: Query<(&Holding, &CharacterControllerCameraOf), Changed<AvianPickupActorState>>,
    mut kcc: Query<&mut CharacterController>,
    prop: Query<&RigidBodyColliders>,
) {
    let Ok((holding, camera_of)) = pickup_actor.get(insert.entity) else {
        return;
    };
    let Ok(mut controller) = kcc.get_mut(camera_of.get()) else {
        return;
    };
    let Ok(prop_colliders) = prop.get(holding.0) else {
        return;
    };
    controller.filter.excluded_entities.extend(prop_colliders);
}

fn filter_in_unpicked_prop(
    discard: On<Discard, Holding>,
    pickup_actor: Query<(&Holding, &CharacterControllerCameraOf), Changed<AvianPickupActorState>>,
    mut kcc: Query<&mut CharacterController>,
    prop: Query<&RigidBodyColliders>,
) {
    let Ok((holding, camera_of)) = pickup_actor.get(discard.entity) else {
        return;
    };
    let Ok(mut controller) = kcc.get_mut(camera_of.get()) else {
        return;
    };
    let Ok(prop_colliders) = prop.get(holding.0) else {
        return;
    };
    for entity in prop_colliders {
        controller.filter.excluded_entities.remove(&entity);
    }
}
