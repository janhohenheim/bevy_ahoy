use std::{f32::consts::TAU, time::Duration};

use avian_pickup::actor::AvianPickupActor;
use bevy_ecs::{lifecycle::HookContext, relationship::Relationship, world::DeferredWorld};

use crate::{CharacterControllerState, input::RotateCamera, prelude::*};

pub(super) fn plugin(app: &mut App) {
    app.add_systems(
        RunFixedMainLoop,
        sync_camera_transform.after(TransformEasingSystems::UpdateEasingTick),
    )
    .add_observer(rotate_camera);
}

#[derive(Component, Clone, Copy, Debug)]
#[relationship(relationship_target = CharacterControllerCamera)]
#[require(AvianPickupActor, Transform)]
#[component(on_add = Self::on_add)]
pub struct CharacterControllerCameraOf {
    #[relationship]
    pub character_controller: Entity,
    pub enable_smoothing: bool,
    pub step_smooth_time: Duration,
    pub teleport_detection_distance: f32,
}

impl CharacterControllerCameraOf {
    pub fn new(character_controller: Entity) -> Self {
        Self {
            character_controller,
            enable_smoothing: true,
            step_smooth_time: Duration::from_millis(200),
            teleport_detection_distance: 10.0,
        }
    }
}

impl CharacterControllerCameraOf {
    fn on_add(mut world: DeferredWorld, ctx: HookContext) {
        let Some(kcc) = world.get::<Self>(ctx.entity).copied() else {
            return;
        };
        let Some(kcc_transform) = world.get::<Transform>(kcc.get()).copied() else {
            return;
        };
        let Some(mut camera_transform) = world.get_mut::<Transform>(ctx.entity) else {
            return;
        };
        *camera_transform = kcc_transform;
    }
}

#[derive(Component, Clone, Copy, Debug)]
#[relationship_target(relationship = CharacterControllerCameraOf)]
pub struct CharacterControllerCamera(Entity);

impl CharacterControllerCamera {
    pub fn get(self) -> Entity {
        self.0
    }
}

pub(crate) fn sync_camera_transform(
    mut cameras: Query<
        (&mut Transform, &CharacterControllerCameraOf),
        (Without<CharacterControllerState>,),
    >,
    kccs: Query<(&Transform, &CharacterController, &CharacterControllerState)>,
    time: Res<Time>,
) {
    // TODO: DIY TransformHelper to use current global transform.
    // Can't use GlobalTransform directly: outdated -> jitter
    // Can't use TransformHelper directly: access conflict with &mut Transform
    for (mut camera_transform, camera) in cameras.iter_mut() {
        if let Ok((kcc_transform, cfg, state)) = kccs.get(camera.character_controller) {
            let height = state
                // changing the collider does not change the transform, so to get the correct position for the feet,
                // we need to use the collider we spawned with.
                .standing_collider
                .aabb(Vec3::default(), Rotation::default())
                .size()
                .y;
            let view_height = if state.crouching {
                cfg.crouch_view_height
            } else {
                cfg.standing_view_height
            };
            let new_translation =
                kcc_transform.translation + Vec3::Y * (-height / 2.0 + view_height);
            camera_transform.translation.x = new_translation.x;
            camera_transform.translation.z = new_translation.z;
            if !camera.enable_smoothing {
                camera_transform.translation.y = new_translation.y;
                return;
            }
            if state.last_step_up.elapsed() < camera.step_smooth_time
                || state.last_step_down.elapsed() < camera.step_smooth_time
            {
                let decay_rate = f32::ln(100000.0);
                camera_transform.translation.y.smooth_nudge(
                    &new_translation.y,
                    decay_rate,
                    time.delta_secs(),
                );
            } else if new_translation.y - camera_transform.translation.y
                < camera.teleport_detection_distance
            {
                let decay_rate = f32::ln(100_000_000.0);
                camera_transform.translation.y.smooth_nudge(
                    &new_translation.y,
                    decay_rate,
                    time.delta_secs(),
                );
            } else {
                camera_transform.translation.y = new_translation.y;
            }
        }
    }
}

fn rotate_camera(
    rotate: On<Fire<RotateCamera>>,
    cameras: Query<&CharacterControllerCamera>,
    mut transforms: Query<&mut Transform>,
) {
    let Ok(camera) = cameras.get(rotate.context) else {
        return;
    };
    let Ok(mut transform) = transforms.get_mut(camera.get()) else {
        return;
    };
    let (mut yaw, mut pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);

    let delta = -rotate.value;
    yaw += delta.x.to_radians();
    pitch += delta.y.to_radians();
    pitch = pitch.clamp(-TAU / 4.0 + 0.01, TAU / 4.0 - 0.01);

    transform.rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
}
