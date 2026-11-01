use crate::{CharacterControllerState, prelude::*};

pub struct AhoyWaterPlugin;

impl Plugin for AhoyWaterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            update_water.before(AhoySystems::MoveCharacters),
        );
    }
}

#[derive(Component, Default, Copy, Reflect, PartialEq, Clone, Debug)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize", reflect(Serialize, Deserialize))]
#[reflect(Component)]
pub struct WaterState {
    pub level: WaterLevel,
    pub speed: f32,
}

#[derive(Default, Copy, Reflect, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize", reflect(Serialize, Deserialize))]
pub enum WaterLevel {
    #[default]
    None,
    Feet,
    Waist,
    Head,
}

#[derive(Reflect, Component, Default)]
#[require(Sensor, Transform, GlobalTransform)]
#[reflect(Component)]
pub struct Water {
    pub speed: f32,
}

fn update_water(
    mut kccs: Query<(
        &Position,
        &CharacterController,
        &CharacterControllerState,
        &mut WaterState,
        &CollidingEntities,
    )>,
    waters: Query<(&Collider, &Position, &Rotation, &Water)>,
) {
    for (kcc_center, cfg, state, mut water_state, colliding_entities) in &mut kccs {
        water_state.level = WaterLevel::None;
        water_state.speed = f32::MAX;
        let kcc_center = kcc_center.0;
        let eye_pos = kcc_center
            + Vec3::Y
                * if state.crouching {
                    cfg.crouch_view_height
                } else {
                    cfg.standing_view_height
                };
        for (collider, position, rotation, water) in waters.iter_many(colliding_entities.iter()).flatten() {
            let level = if collider.contains_point(position.0, *rotation, eye_pos) {
                WaterLevel::Head
            } else if collider.contains_point(position.0, *rotation, kcc_center) {
                WaterLevel::Waist
            } else {
                WaterLevel::Feet
            };

            water_state.level = level.max(water_state.level);
            water_state.speed = water_state.speed.min(water.speed);
        }
    }
}
