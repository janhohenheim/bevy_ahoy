use std::time::Duration;

use crate::prelude::*;

/// Determines whether a jump is triggered on action [`Start`] or [`Complete`], and
/// optionally configures jump cancel behavior to enable variable jump heights.
///
/// [`Start`]: bevy_enhanced_input::action::events::Start
/// [`Complete`]: bevy_enhanced_input::action::events::Complete
#[derive(Clone, Debug, Reflect)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize", reflect(Serialize, Deserialize))]
pub enum JumpTrigger {
    /// Apply impulse on jump start, and optionally cancel on jump complete.
    OnPress(Option<JumpCancelMode>),
    /// Apply impulse on jump complete, and optionally cancel on jump start.
    OnRelease {
        actuation: JumpActuation,
        cancel: Option<JumpCancelMode>,
    },
}

/// Determines how the duration of a hold modifies the character's jump
/// impulse velocity when the jump is released.
///
/// [`OnRelease`]: JumpTrigger::OnRelease
#[derive(Clone, Debug, Reflect)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize", reflect(Serialize, Deserialize))]
#[non_exhaustive]
pub enum JumpActuation {
    /// Do not modify the impulse based on hold duration before release.
    None {
        /*
        TODO: Add an optional timeout behavior that disables buffered jumps after holding for too long while grounded.
        /// The maximum amount of time a jump can be held while grounded before releasing the action
        /// will no longer trigger a jump.
        max_hold_time: Option<Duration>,
        */
    },

    /// Enable variable jump heights based on the output of an easing curve and the duration of the hold.
    Curve(JumpActuationCurve),
}

#[derive(Clone, Debug, Reflect)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize", reflect(Serialize, Deserialize))]
#[non_exhaustive]
pub struct JumpActuationCurve {
    /// An easing curve that scales the jump impulse based on how long the jump button was held before release, where the
    /// curve input ranges from `0.0` (jump released immediately) to `1.0` (jump released at or after the maximum hold time).
    /// The curve output scales the magnitude of the jump impulse.
    pub curve: EasingCurve<f32>,
    /// The maximum amount of time a jump can be held to progress along the curve.
    pub charge_duration: Duration,
    /*
    TODO: Add an optional timeout behavior that disables buffered jumps after holding for too long while grounded.
    /// If `true`, holding jump beyond the `maximum_hold_time` while grounded will no longer
    /// trigger a jump. The timer for this is reset on landing, so the player can hold jump before
    /// landing and still trigger a jump using the buffer input, but if they hold for too long, the
    /// buffered jump will be lost.
    ///
    /// This is particularly useful when paired with [`JumpCancelMode`] to allow the controller to
    /// cancel the jump and keep holding to prevent jumping again.
    pub timeout: bool,
    */
}

impl Default for JumpActuationCurve {
    fn default() -> Self {
        Self {
            charge_duration: Duration::from_millis(500),
            curve: EasingCurve::new(0.3, 1.0, EaseFunction::ExponentialOut),
        }
    }
}

/// Determines how a character's vertical velocity is modified when a jump is canceled.
#[derive(Clone, Debug, Reflect)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize", reflect(Serialize, Deserialize))]
pub struct JumpCancelMode {
    /// Defines the impulse behavior when the jump is canceled **before** reaching the apex.
    pub pre_apex: Option<JumpCancelPreApex>,
    /// Defines the impulse behavior when the jump is canceled **after** reaching the apex.
    pub post_apex: Option<JumpCancelPostApex>,
    /// The maximum downward velocity that can accumulate before canceling the jump button will no
    /// longer apply an impulse.
    ///
    /// This prevents boosting the character if they have already been falling for
    /// a while when a held jump is finally canceled.
    ///
    /// Note that this threshold is directional, meaning if the sign of the threshold is positive, it
    /// will only prevent positive (upward) velocity boosts, and if it's negative, it will only
    /// prevent negative (downward) velocity boosts.
    pub threshold: Option<f32>,
}

/// Determines how canceling a jump **before** the jump apex modifies the character's vertical velocity.
#[derive(Clone, Debug, Reflect)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize", reflect(Serialize, Deserialize))]
pub enum JumpCancelPreApex {
    /// Replace the vertical velocity with a new impulse when the jump is canceled.
    Cancel(f32),
    /// Apply an inverse impulse proportional to the remaining jump velocity, scaled by the output
    /// of an easing curve that evaluates completion to the apex.
    ///
    /// The curve input ranges from `0.0` (jump canceled immediately) to `1.0` (jump canceled at
    /// the apex). The curve output scales the magnitude of the impulse.
    Hop(EasingCurve<f32>),
    /// Apply an impulse inverse to the original jump velocity, scaled by a flat
    /// multiplier, regardless of when the jump is canceled prior to the apex.
    Mul(f32),
}

/// Determines how canceling a jump **after** the jump apex modifies the player's velocity.
#[derive(Clone, Debug, Reflect)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize", reflect(Serialize, Deserialize))]
pub enum JumpCancelPostApex {
    /// Replace the vertical velocity with a new impulse when the jump is released.
    Cancel(f32),
    /// Apply a flat impulse regardless of when the jump is released after the apex.
    Flat(f32),
}

impl Default for JumpCancelPreApex {
    fn default() -> Self {
        Self::Hop(EasingCurve::new(0.0, 1.0, EaseFunction::QuadraticIn))
    }
}

impl Default for JumpCancelPostApex {
    fn default() -> Self {
        Self::Flat(0.0)
    }
}

impl Default for JumpCancelMode {
    fn default() -> Self {
        Self {
            pre_apex: Some(JumpCancelPreApex::default()),
            post_apex: Some(JumpCancelPostApex::default()),
            threshold: None,
        }
    }
}

impl Default for JumpTrigger {
    fn default() -> Self {
        Self::default_on_press()
    }
}

impl JumpTrigger {
    #[inline]
    pub fn default_on_release() -> Self {
        Self::OnRelease {
            actuation: JumpActuation::Curve(JumpActuationCurve::default()),
            cancel: Some(JumpCancelMode::default()),
        }
    }

    #[inline]
    pub fn default_on_press() -> Self {
        Self::OnPress(Some(JumpCancelMode::default()))
    }
}

impl JumpCancelPreApex {
    fn apply_impulse(
        &self,
        y_velocity: f32,
        jump_power: f32,
        time_since_grounded: Duration,
        time_to_apex: Duration,
    ) -> f32 {
        let apex_completion = time_since_grounded.as_secs_f32() / time_to_apex.as_secs_f32();
        let inverse_completion = 1.0 - apex_completion.clamp(0.0, 1.0);

        match self {
            &JumpCancelPreApex::Cancel(new_impulse) => new_impulse,
            JumpCancelPreApex::Hop(curve) => {
                if let Some(hop_multiplier) = curve.sample(inverse_completion) {
                    y_velocity - jump_power * hop_multiplier
                } else {
                    y_velocity
                }
            }
            JumpCancelPreApex::Mul(factor) => y_velocity - jump_power * inverse_completion * factor,
        }
    }
}

impl JumpCancelPostApex {
    fn apply_impulse(&self, velocity_y: f32) -> f32 {
        match *self {
            JumpCancelPostApex::Cancel(new_impulse) => new_impulse,
            JumpCancelPostApex::Flat(flat) => velocity_y + flat,
        }
    }
}

impl JumpCancelMode {
    pub(crate) fn handle_cancel(
        &self,
        mut lin_velocity_y: f32,
        jump_power: f32,
        gravity: f32,
        is_grounded: bool,
        time_since_grounded: Duration,
    ) -> Option<f32> {
        if is_grounded {
            return None;
        }

        if self.pre_apex.is_none() && self.post_apex.is_none() {
            return None;
        }

        // Directional threshold check. Supports clamped boosting in either direction.
        if let Some(threshold) = self.threshold {
            let signum = threshold.signum();
            if lin_velocity_y * signum > threshold * signum {
                return None;
            }
        }

        let time_to_apex = Duration::from_secs_f32((jump_power / gravity).abs());

        // Pre-apex release
        if time_since_grounded < time_to_apex
            && let Some(ref pre_apex) = self.pre_apex
        {
            lin_velocity_y = pre_apex.apply_impulse(
                lin_velocity_y,
                jump_power,
                time_since_grounded,
                time_to_apex,
            );
        } else
        // Post-apex release
        if let Some(ref post_apex) = self.post_apex {
            lin_velocity_y = post_apex.apply_impulse(lin_velocity_y);
        };

        // Clamp to threshold after applying impulse
        if let Some(threshold) = self.threshold {
            let signum = threshold.signum();
            lin_velocity_y = if signum > 0.0 {
                lin_velocity_y.min(threshold)
            } else {
                lin_velocity_y.max(threshold)
            };
        }

        Some(lin_velocity_y)
    }
}
