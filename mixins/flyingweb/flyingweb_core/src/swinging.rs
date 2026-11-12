use glam::DVec3;

use crate::{
    config::FlyingWebConfig,
    input::{TickInput, WristSide},
    physics::{
        PendulumConstraint, PendulumStepInput, compute_release_velocity, step_pendulum,
    },
    raycast::{
        AnchorCandidate, WorldQuery, find_best_swing_anchor, horizontal_forward,
        horizontal_right, look_direction, raycast_blocks,
    },
    web::{StrandId, WebStrandManager},
};

/// Travel speed of a freshly shot web tip in blocks/tick (`7.8` blocks/tick = `156 m/s`):
/// fast enough to feel snappy and responsive while visibly shooting out from the wrist!
pub const WEB_PROJECTILE_SPEED_PER_TICK: f64 = 7.8;

/// Active phase of the Web Swinging state machine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SwingPhase {
    /// Not currently shooting or attached to a swing web.
    Idle,
    /// Web silk bolt has left the wrist and is flying toward `anchor` (`0..=6` ticks).
    Shooting {
        strand_id: StrandId,
        wrist: WristSide,
        anchor: DVec3,
        anchor_normal: DVec3,
        tip_progress: f64,
        total_distance: f64,
    },
    /// Web tip has anchored into solid geometry; pendulum constraint is active!
    Attached {
        strand_id: StrandId,
        wrist: WristSide,
        constraint: PendulumConstraint,
        ticks_attached: u32,
    },
}

impl SwingPhase {
    pub fn is_active(&self) -> bool {
        !matches!(self, Self::Idle)
    }

    pub fn is_attached(&self) -> bool {
        matches!(self, Self::Attached { .. })
    }

    pub fn anchor_point(&self) -> Option<DVec3> {
        match *self {
            Self::Idle => None,
            Self::Shooting { anchor, .. } => Some(anchor),
            Self::Attached { constraint, .. } => Some(constraint.anchor),
        }
    }

    pub fn active_wrist(&self) -> Option<WristSide> {
        match *self {
            Self::Idle => None,
            Self::Shooting { wrist, .. } | Self::Attached { wrist, .. } => Some(wrist),
        }
    }

    pub fn tension(&self) -> f32 {
        match *self {
            Self::Attached { constraint, .. } => constraint.tension,
            Self::Shooting { .. } => 0.35,
            Self::Idle => 0.0,
        }
    }
}

/// High-level events emitted by `SwingController` for audio, VFX, camera impulses, and animations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SwingEvent {
    /// Player pressed shoot with no valid solid anchor in range.
    MissedShot { wrist: WristSide },
    /// Web shot launched from `wrist` toward `anchor`.
    ShotLaunched {
        wrist: WristSide,
        origin: DVec3,
        anchor: DVec3,
    },
    /// Flying web tip impacted `anchor` and latched taut.
    AnchorAttached {
        wrist: WristSide,
        anchor: DVec3,
        normal: DVec3,
    },
    /// Web released cleanly (`vault_jump` is true when vaulted off with Jump).
    Released {
        vault_jump: bool,
        release_speed: f64,
    },
}

/// Manages Spider-Hero Web Swinging lifecycle, rapid hand-to-hand anchor switching,
/// obstacle line-break safety, and wrist kinematics.
#[derive(Clone, Debug)]
pub struct SwingController {
    pub phase: SwingPhase,
    pub next_wrist: WristSide,
    /// Brief cooldown in ticks preventing accidental same-tick double fire (`2` ticks = 100 ms).
    pub shot_cooldown_ticks: u8,
    /// Preview of best available swing anchor this tick for HUD reticle feedback.
    pub hovered_anchor: Option<AnchorCandidate>,
    /// Last horizontal swing angular bank (-1..1) for smooth camera & body roll.
    pub lateral_bank: f32,
}

impl Default for SwingController {
    fn default() -> Self {
        Self {
            phase: SwingPhase::Idle,
            next_wrist: WristSide::Right,
            shot_cooldown_ticks: 0,
            hovered_anchor: None,
            lateral_bank: 0.0,
        }
    }
}

impl SwingController {
    pub fn reset(&mut self) {
        self.phase = SwingPhase::Idle;
        self.shot_cooldown_ticks = 0;
        self.hovered_anchor = None;
        self.lateral_bank = 0.0;
    }

    /// Computes the world-space 3D wrist nozzle position for `wrist` given player feet, yaw, and pitch.
    pub fn compute_wrist_origin(
        player_feet: DVec3,
        yaw_deg: f32,
        pitch_deg: f32,
        wrist: WristSide,
        is_reaching_up: bool,
    ) -> DVec3 {
        let right = horizontal_right(yaw_deg);
        let fwd = look_direction(yaw_deg, pitch_deg);
        let shoulder_height = if is_reaching_up { 1.48 } else { 1.24 };
        player_feet
            + DVec3::new(0.0, shoulder_height, 0.0)
            + right * (0.34 * wrist.sign())
            + fwd * 0.28
    }

    /// Updates HUD anchor preview and advances the web-swinging state machine by one 20 Hz tick.
    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        world: &dyn WorldQuery,
        player_feet: DVec3,
        mut velocity: DVec3,
        yaw_deg: f32,
        pitch_deg: f32,
        on_ground: bool,
        input: &TickInput,
        config: &FlyingWebConfig,
        strands: &mut WebStrandManager,
        events: &mut Vec<SwingEvent>,
    ) -> DVec3 {
        if self.shot_cooldown_ticks > 0 {
            self.shot_cooldown_ticks -= 1;
        }

        let eye_pos = player_feet + DVec3::new(0.0, 1.62, 0.0);
        let pivot_pos = player_feet + DVec3::new(0.0, 1.05, 0.0);
        let aim_dir = look_direction(yaw_deg, pitch_deg);
        let preview_wrist = self.next_wrist;
        let preview_wrist_pos =
            Self::compute_wrist_origin(player_feet, yaw_deg, pitch_deg, preview_wrist, false);

        // Always update smart cone anchor preview when abilities are active so the HUD reticle
        // gives crisp Insomniac-style target lock feedback!
        self.hovered_anchor = if config.abilities_active() {
            find_best_swing_anchor(
                world,
                eye_pos,
                preview_wrist_pos,
                aim_dir,
                player_feet.y,
                velocity,
                config.min_web_range,
                config.max_web_range,
                config.cone_search_degrees,
            )
        } else {
            None
        };

        if !config.abilities_active() {
            if self.phase.is_active() {
                self.detach_current(strands);
            }
            self.lateral_bank *= 0.80;
            return velocity;
        }

        // Slingshot chord (`trick_held` while grounded) reserves `shoot` for Slingshot in `movement.rs`.
        let slingshot_chord = config.enable_slingshot && on_ground && input.trick_held;

        // 1. Check explicit Release conditions:
        //    - Dedicated `release_web` key pressed
        //    - In `hold_to_swing` mode: `shoot_web` key released
        //    - Mid-swing `Jump` pressed -> signature Apex Vault Launch!
        if self.phase.is_active() {
            let hold_released = config.hold_to_swing && !input.shoot_held;
            let vault_jump = self.phase.is_attached() && input.jump_just_pressed && !on_ground;
            if input.release_just_pressed || hold_released || vault_jump {
                let look_flat = horizontal_forward(yaw_deg);
                if self.phase.is_attached() {
                    velocity = compute_release_velocity(velocity, look_flat, vault_jump, config);
                }
                self.detach_current(strands);
                events.push(SwingEvent::Released {
                    vault_jump,
                    release_speed: velocity.length(),
                });
                if vault_jump || input.release_just_pressed {
                    self.lateral_bank *= 0.85;
                    return velocity;
                }
            }
        }

        // 2. Check Shoot / Rapid Anchor Switching:
        //    - In `hold_to_swing`: pressing `shoot_just_pressed` fires a fresh web (if already swinging,
        //      switches seamlessly from Left -> Right wrist to a new anchor ahead!).
        //    - In toggle mode (`!hold_to_swing`): pressing `shoot_just_pressed` while idle fires; pressing
        //      `shoot_just_pressed` while swinging switches to a new forward anchor if available, or releases!
        if input.shoot_just_pressed && !slingshot_chord && self.shot_cooldown_ticks == 0 {
            if let Some(candidate) = self.hovered_anchor {
                // Detach previous swing strand into a smooth fading trail (rapid hand-to-hand switch!)
                if self.phase.is_active() {
                    self.detach_current(strands);
                }
                let wrist = self.next_wrist;
                self.next_wrist = wrist.flip();
                let wrist_origin =
                    Self::compute_wrist_origin(player_feet, yaw_deg, pitch_deg, wrist, true);
                let dist = wrist_origin.distance(candidate.hit.position).max(1.0);
                let strand_id = strands.spawn_swing_strand(
                    wrist,
                    wrist_origin,
                    candidate.hit.position,
                    candidate.hit.normal,
                    config.suit_variant.web_tint_argb(),
                );
                self.shot_cooldown_ticks = 2;

                // Very close anchors (< 9 blocks) attach immediately on the same tick;
                // farther anchors fly visibly across 1..5 ticks at 156 m/s!
                if dist <= WEB_PROJECTILE_SPEED_PER_TICK * 1.15 {
                    strands.mark_attached(strand_id, 0.55);
                    let max_len = pivot_pos
                        .distance(candidate.hit.position)
                        .clamp(config.min_web_range, config.max_web_range);
                    self.phase = SwingPhase::Attached {
                        strand_id,
                        wrist,
                        constraint: PendulumConstraint {
                            anchor: candidate.hit.position,
                            anchor_normal: candidate.hit.normal,
                            max_length: max_len,
                            min_length: config.min_web_range,
                            tension: 0.55,
                            is_taut: true,
                        },
                        ticks_attached: 0,
                    };
                    events.push(SwingEvent::ShotLaunched {
                        wrist,
                        origin: wrist_origin,
                        anchor: candidate.hit.position,
                    });
                    events.push(SwingEvent::AnchorAttached {
                        wrist,
                        anchor: candidate.hit.position,
                        normal: candidate.hit.normal,
                    });
                } else {
                    self.phase = SwingPhase::Shooting {
                        strand_id,
                        wrist,
                        anchor: candidate.hit.position,
                        anchor_normal: candidate.hit.normal,
                        tip_progress: WEB_PROJECTILE_SPEED_PER_TICK,
                        total_distance: dist,
                    };
                    events.push(SwingEvent::ShotLaunched {
                        wrist,
                        origin: wrist_origin,
                        anchor: candidate.hit.position,
                    });
                }
            } else if !config.hold_to_swing && self.phase.is_active() {
                self.detach_current(strands);
                events.push(SwingEvent::Released {
                    vault_jump: false,
                    release_speed: velocity.length(),
                });
            } else {
                let wrist = self.next_wrist;
                self.next_wrist = wrist.flip();
                events.push(SwingEvent::MissedShot { wrist });
            }
        }

        // 3. Advance active Shooting or Attached phase
        match self.phase {
            SwingPhase::Idle => {
                self.lateral_bank *= 0.82;
            }
            SwingPhase::Shooting {
                strand_id,
                wrist,
                anchor,
                anchor_normal,
                mut tip_progress,
                total_distance,
            } => {
                // Verify target block hasn't been broken or unloaded mid-flight.
                let anchor_voxel = glam::IVec3::new(
                    (anchor.x - anchor_normal.x * 0.15).floor() as i32,
                    (anchor.y - anchor_normal.y * 0.15).floor() as i32,
                    (anchor.z - anchor_normal.z * 0.15).floor() as i32,
                );
                if !world.is_solid_block(anchor_voxel) {
                    self.detach_current(strands);
                    return velocity;
                }

                let wrist_origin =
                    Self::compute_wrist_origin(player_feet, yaw_deg, pitch_deg, wrist, true);
                tip_progress += WEB_PROJECTILE_SPEED_PER_TICK;
                let frac = (tip_progress / total_distance.max(0.5)).clamp(0.0, 1.0);
                strands.update_shooting_strand(strand_id, wrist_origin, frac as f32);

                if frac >= 1.0 {
                    strands.mark_attached(strand_id, 0.50);
                    let max_len = pivot_pos
                        .distance(anchor)
                        .clamp(config.min_web_range, config.max_web_range);
                    // Small upward liftoff nudge if shooting from flat ground so pendulum swing begins cleanly
                    if on_ground && anchor.y > pivot_pos.y + 2.0 {
                        velocity.y = velocity.y.max(0.26);
                    }
                    self.phase = SwingPhase::Attached {
                        strand_id,
                        wrist,
                        constraint: PendulumConstraint {
                            anchor,
                            anchor_normal,
                            max_length: max_len,
                            min_length: config.min_web_range,
                            tension: 0.50,
                            is_taut: true,
                        },
                        ticks_attached: 0,
                    };
                    events.push(SwingEvent::AnchorAttached {
                        wrist,
                        anchor,
                        normal: anchor_normal,
                    });
                } else {
                    self.phase = SwingPhase::Shooting {
                        strand_id,
                        wrist,
                        anchor,
                        anchor_normal,
                        tip_progress,
                        total_distance,
                    };
                }
            }
            SwingPhase::Attached {
                strand_id,
                wrist,
                constraint,
                ticks_attached,
            } => {
                // Verify the anchor block still exists and the tether hasn't exceeded crazy range
                let anchor_voxel = glam::IVec3::new(
                    (constraint.anchor.x - constraint.anchor_normal.x * 0.15).floor() as i32,
                    (constraint.anchor.y - constraint.anchor_normal.y * 0.15).floor() as i32,
                    (constraint.anchor.z - constraint.anchor_normal.z * 0.15).floor() as i32,
                );
                let dist_now = pivot_pos.distance(constraint.anchor);
                if !world.is_solid_block(anchor_voxel) || dist_now > config.max_web_range + 12.0 {
                    self.detach_current(strands);
                    events.push(SwingEvent::Released {
                        vault_jump: false,
                        release_speed: velocity.length(),
                    });
                    return velocity;
                }

                // Compute WASD wish direction in camera horizontal frame
                let fwd = horizontal_forward(yaw_deg);
                let right = horizontal_right(yaw_deg);
                // Note: in Horizon's `native_movement_input`, `+strafe` is left (`keys.left`), `-strafe` is right!
                let wish_dir =
                    (fwd * (input.forward as f64) - right * (input.strafe as f64)).normalize_or_zero();

                let reel_delta = (input.wheel_delta as f64) * config.reel_speed
                    + if input.jump_held && input.sneak_held {
                        config.reel_speed * 0.65
                    } else {
                        0.0
                    };

                let ground_clearance = raycast_blocks(world, player_feet + DVec3::Y * 0.1, DVec3::NEG_Y, 4.0, false)
                    .map(|h| (h.distance - 0.1).max(0.0));

                let step_out = step_pendulum(
                    constraint,
                    PendulumStepInput {
                        pivot_pos,
                        velocity,
                        wish_dir,
                        forward_input: input.forward,
                        reel_delta,
                        ground_clearance,
                        on_ground,
                        sneak_held: input.sneak_held,
                    },
                    config,
                );

                velocity = step_out.new_velocity;

                // Compute smooth lateral bank angle from angular cross-velocity & strafe input
                let horiz_vel = DVec3::new(velocity.x, 0.0, velocity.z);
                let side_vel = horiz_vel.dot(right) as f32;
                let target_bank = (-input.strafe * 0.55 + side_vel * 0.45).clamp(-1.0, 1.0);
                self.lateral_bank += (target_bank - self.lateral_bank) * 0.22;

                let wrist_origin =
                    Self::compute_wrist_origin(player_feet, yaw_deg, pitch_deg, wrist, true);
                strands.update_attached_strand(
                    strand_id,
                    wrist_origin,
                    step_out.updated_constraint.tension,
                    step_out.updated_constraint.max_length,
                );

                self.phase = SwingPhase::Attached {
                    strand_id,
                    wrist,
                    constraint: step_out.updated_constraint,
                    ticks_attached: ticks_attached.saturating_add(1),
                };
            }
        }

        velocity
    }

    pub fn force_cancel(&mut self, strands: &mut WebStrandManager) {
        if self.phase.is_active() {
            self.detach_current(strands);
        }
        self.lateral_bank = 0.0;
    }

    fn detach_current(&mut self, strands: &mut WebStrandManager) {
        match self.phase {
            SwingPhase::Shooting { strand_id, .. } | SwingPhase::Attached { strand_id, .. } => {
                strands.detach_strand(strand_id);
            }
            SwingPhase::Idle => {}
        }
        self.phase = SwingPhase::Idle;
    }
}
