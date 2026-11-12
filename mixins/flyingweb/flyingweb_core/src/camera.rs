use glam::DVec3;

use crate::{
    config::FlyingWebConfig,
    raycast::{WorldQuery, horizontal_right, look_direction, raycast_blocks},
};

/// Active traversal mode driving Spider-Cam framing, FOV, roll, and obstacle offsets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CameraTraversalMode {
    #[default]
    Grounded,
    WebSwinging,
    WebZip,
    PointLaunch,
    SlingshotCharge,
    SlingshotLaunch,
    Freefall,
    WebWings,
    WallRunning,
    WallCrawling,
    CeilingCrawling,
}

impl CameraTraversalMode {
    pub fn is_spider_traversal(self) -> bool {
        !matches!(self, Self::Grounded)
    }
}

/// Frame-level inputs supplied to `SpiderCamera::evaluate_frame`.
#[derive(Clone, Copy, Debug)]
pub struct CameraFrameInput {
    /// Sub-tick interpolated player feet position (`prev_feet.lerp(curr_feet, partial)`).
    pub interp_feet: DVec3,
    /// Current 20 Hz simulation velocity (blocks/tick).
    pub velocity: DVec3,
    /// Player look yaw in degrees (`0` = +Z south, `90` = -X west).
    pub player_yaw: f32,
    /// Player look pitch in degrees (`+` = looking down).
    pub player_pitch: f32,
    pub on_ground: bool,
    pub mode: CameraTraversalMode,
    /// Surface normal when wall-running or wall/ceiling crawling.
    pub surface_normal: DVec3,
    /// Lateral swing bank hint (`-1.0..=1.0`).
    pub swing_bank: f32,
    /// Elapsed wall-clock frame delta in seconds (`native_frame`).
    pub dt_sec: f32,
}

/// Computed camera pose, roll, and FOV delta for the current render frame.
#[derive(Clone, Copy, Debug)]
pub struct CameraPoseOutput {
    pub position: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub roll_deg: f32,
    pub fov_bonus_deg: f32,
    pub effective_distance: f32,
}

/// Frame-rate independent exponential smoothing factor `1 - exp(-rate * dt)`.
#[inline]
fn exp_alpha(rate: f32, dt_sec: f32) -> f32 {
    1.0 - (-rate.max(0.0) * dt_sec.clamp(0.0002, 0.1)).exp()
}

/// Shortest signed angular difference `target - current` in `[-180, 180]` degrees.
#[inline]
pub fn wrap_angle_delta_deg(current: f32, target: f32) -> f32 {
    (target - current + 180.0).rem_euclid(360.0) - 180.0
}

/// Insomniac-inspired Third-Person Dynamic Spider-Cam.
#[derive(Clone, Debug)]
pub struct SpiderCamera {
    initialized: bool,
    /// Smoothed focus pivot in world space.
    focus_pos: DVec3,
    /// Lagged inertia offset in world space conveying acceleration & bursts.
    inertia_offset: DVec3,
    /// Smoothed collision-clamped boom distance in blocks.
    current_distance: f32,
    /// Smoothed pitch assist offset in degrees.
    pitch_assist_deg: f32,
    /// Smoothed roll / bank angle in degrees.
    current_roll_deg: f32,
    /// Smoothed dynamic FOV bonus in degrees.
    current_fov_bonus: f32,
    /// Smoothed wall/surface normal clearance offset.
    surface_push_offset: DVec3,

    /// Previous frame player yaw for smooth turn-rate roll calculation.
    prev_yaw: f32,
    /// Critically damped landing compression spring state.
    landing_offset_y: f32,
    landing_vel_y: f32,
    was_on_ground: bool,
    peak_downward_speed: f64,
    /// Burst impulse timer (Web Zip / Slingshot / Vault Launch).
    burst_lag_impulse: DVec3,
}

impl Default for SpiderCamera {
    fn default() -> Self {
        Self {
            initialized: false,
            focus_pos: DVec3::ZERO,
            inertia_offset: DVec3::ZERO,
            current_distance: 4.2,
            pitch_assist_deg: 0.0,
            current_roll_deg: 0.0,
            current_fov_bonus: 0.0,
            surface_push_offset: DVec3::ZERO,
            prev_yaw: 0.0,
            landing_offset_y: 0.0,
            landing_vel_y: 0.0,
            was_on_ground: true,
            peak_downward_speed: 0.0,
            burst_lag_impulse: DVec3::ZERO,
        }
    }
}

impl SpiderCamera {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Injects a smooth backward camera lag impulse when launching a Web Zip,
    /// Point Launch, or Slingshot so acceleration feels punchy without snapping!
    pub fn trigger_launch_impulse(&mut self, launch_dir: DVec3, strength: f64) {
        let dir = launch_dir.normalize_or_zero();
        self.burst_lag_impulse =
            (self.burst_lag_impulse - dir * strength.clamp(0.0, 1.35)).clamp_length_max(1.45);
    }

    /// Evaluates the full third-person dynamic camera pose, wall collision avoidance,
    /// banking roll, landing compression, and speed-based FOV for one render frame.
    pub fn evaluate_frame(
        &mut self,
        world: &dyn WorldQuery,
        input: CameraFrameInput,
        config: &FlyingWebConfig,
    ) -> CameraPoseOutput {
        let dt = input.dt_sec.clamp(0.0005, 0.05);
        let speed = input.velocity.length();
        let speed_norm = (speed / config.max_swing_speed.max(0.5)).clamp(0.0, 1.35) as f32;

        // Track downward airspeed so touchdown triggers proportional landing compression!
        if !input.on_ground {
            if -input.velocity.y > self.peak_downward_speed {
                self.peak_downward_speed = -input.velocity.y;
            }
        } else if !self.was_on_ground {
            let impact = (self.peak_downward_speed as f32 - 0.24).clamp(0.0, 1.4);
            if impact > 0.02 {
                self.landing_vel_y -=
                    impact * 3.8 * config.camera_landing_settle * config.camera_effect_intensity;
            }
            self.peak_downward_speed = 0.0;
        }
        self.was_on_ground = input.on_ground;

        // Advance critically damped landing spring (`omega = 15 rad/s`)
        let omega = 15.0_f32;
        let spring_acc =
            -omega * omega * self.landing_offset_y - 2.0 * omega * self.landing_vel_y;
        self.landing_vel_y += spring_acc * dt;
        self.landing_offset_y = (self.landing_offset_y + self.landing_vel_y * dt).clamp(-0.48, 0.18);

        // Mode-specific framing parameters
        let (dist_mult, height_bonus, lookahead_gain, fov_mult, mode_roll_mult) = match input.mode {
            CameraTraversalMode::Grounded => (0.92, 0.0, 0.18, 0.35, 0.15),
            CameraTraversalMode::WebSwinging => (1.06, 0.12, 0.88, 1.00, 1.00),
            CameraTraversalMode::WebZip => (1.10, 0.05, 0.55, 1.15, 0.55),
            CameraTraversalMode::PointLaunch => (1.04, 0.08, 0.65, 1.05, 0.45),
            CameraTraversalMode::SlingshotCharge => (0.80, -0.08, 0.0, -0.18, 0.0),
            CameraTraversalMode::SlingshotLaunch => (1.22, 0.14, 0.95, 1.30, 0.65),
            CameraTraversalMode::Freefall => {
                // Tighter overhead framing when diving steeply downward
                let dive_t = ((input.player_pitch - 25.0) / 55.0).clamp(0.0, 1.0);
                (1.02 - dive_t * 0.16, 0.10 + dive_t * 0.18, 0.72, 1.08, 0.55)
            }
            CameraTraversalMode::WebWings => (1.16, 0.18, 0.95, 1.18, 1.25),
            CameraTraversalMode::WallRunning => (1.02, 0.08, 0.75, 0.92, 0.85),
            CameraTraversalMode::WallCrawling => (0.82, 0.02, 0.25, 0.25, 0.30),
            CameraTraversalMode::CeilingCrawling => (0.78, -0.22, 0.20, 0.20, 0.20),
        };

        // Surface clearance offset so walls and ceilings never crowd the frame!
        let target_surface_push = match input.mode {
            CameraTraversalMode::WallRunning => input.surface_normal * 0.92,
            CameraTraversalMode::WallCrawling => input.surface_normal * 0.72,
            CameraTraversalMode::CeilingCrawling => input.surface_normal * 0.85,
            _ => DVec3::ZERO,
        };
        let surf_alpha = exp_alpha(10.0, dt) as f64;
        self.surface_push_offset = self.surface_push_offset.lerp(target_surface_push, surf_alpha);

        // Chest/head anchor on player
        let base_anchor = input.interp_feet
            + DVec3::new(
                0.0,
                1.46 + (config.camera_height_offset + height_bonus + self.landing_offset_y) as f64,
                0.0,
            );

        if !self.initialized || base_anchor.distance(self.focus_pos) > 24.0 {
            self.focus_pos = base_anchor;
            self.inertia_offset = DVec3::ZERO;
            self.current_distance = config.camera_base_distance * dist_mult;
            self.prev_yaw = input.player_yaw;
            self.initialized = true;
        }

        // Smooth focus tracking (high rate so player never drifts off screen, while filtering micro-jitter)
        let follow_alpha = exp_alpha(config.camera_follow_speed * 1.65, dt) as f64;
        self.focus_pos = self.focus_pos.lerp(base_anchor, follow_alpha);

        // Decay burst lag impulse smoothly
        let burst_decay = exp_alpha(7.5, dt) as f64;
        self.burst_lag_impulse = self.burst_lag_impulse.lerp(DVec3::ZERO, burst_decay);

        // Velocity look-ahead + subtle elastic inertia lag
        let vel_lookahead = input.velocity * (lookahead_gain as f64 * 0.65);
        let desired_inertia = (vel_lookahead * (1.0 - config.camera_inertia as f64 * 0.55)
            + self.burst_lag_impulse * (config.camera_inertia as f64))
            .clamp_length_max(1.65);
        let inertia_alpha = exp_alpha(8.5, dt) as f64;
        self.inertia_offset = self.inertia_offset.lerp(desired_inertia, inertia_alpha);

        // Pendulum-arc & dive pitch tracking assist (gentle, never fights player mouse control!)
        let target_pitch_assist = if input.mode == CameraTraversalMode::WebSwinging && speed > 0.22 {
            // Negative velocity.y (diving) -> slight downward camera pitch (+deg);
            // Positive velocity.y (upswing) -> slight upward camera lift (-deg).
            let vert_angle_deg = (-input.velocity.y / speed.max(0.25))
                .clamp(-0.85, 0.85)
                .asin()
                .to_degrees() as f32;
            (vert_angle_deg * 0.28 * config.camera_arc_pitch_tracking * config.camera_effect_intensity)
                .clamp(-12.0, 14.0)
        } else if input.mode == CameraTraversalMode::WebWings && speed > 0.20 {
            let glide_angle = (-input.velocity.y / speed.max(0.25))
                .clamp(-0.7, 0.7)
                .asin()
                .to_degrees() as f32;
            (glide_angle * 0.22 * config.camera_arc_pitch_tracking).clamp(-9.0, 11.0)
        } else {
            0.0
        };
        self.pitch_assist_deg +=
            (target_pitch_assist - self.pitch_assist_deg) * exp_alpha(6.5, dt);

        let effective_yaw = input.player_yaw;
        let effective_pitch = (input.player_pitch + self.pitch_assist_deg).clamp(-88.5, 88.5);

        // Compute turn-rate + lateral swing bank roll
        let yaw_delta = wrap_angle_delta_deg(self.prev_yaw, effective_yaw);
        self.prev_yaw = effective_yaw;
        let yaw_rate = (yaw_delta / dt).clamp(-260.0, 260.0);

        let turn_roll = (-yaw_rate * 0.032) * speed_norm.clamp(0.15, 1.1);
        let swing_roll = input.swing_bank * 8.5;
        let wall_roll = if input.mode == CameraTraversalMode::WallRunning {
            let right = horizontal_right(effective_yaw);
            // Tilt slightly away from the wall (~7 deg) for authentic Insomniac wall-run framing!
            -(input.surface_normal.dot(right) as f32) * 7.5
        } else {
            0.0
        };
        let raw_roll = (turn_roll + swing_roll + wall_roll)
            * mode_roll_mult
            * config.camera_tilt_strength
            * config.camera_effect_intensity;
        let target_roll = raw_roll.clamp(-15.0, 15.0);
        self.current_roll_deg += (target_roll - self.current_roll_deg) * exp_alpha(8.0, dt);

        // Speed-adaptive desired boom distance
        let desired_dist = (config.camera_base_distance * dist_mult
            + speed_norm * config.camera_speed_distance_gain * config.camera_effect_intensity)
            .clamp(1.6, 12.5);

        // Compute ideal boom direction & lateral shoulder framing
        let look_vec = look_direction(effective_yaw, effective_pitch);
        let right_vec = horizontal_right(effective_yaw);
        let shoulder_shift = right_vec * (config.camera_shoulder_offset as f64);

        let safe_origin = input.interp_feet + DVec3::new(0.0, 1.42, 0.0);
        let framed_pivot = self.focus_pos + self.inertia_offset + shoulder_shift + self.surface_push_offset;
        let ideal_cam = framed_pivot - look_vec * (desired_dist as f64);

        // 5-ray collision avoidance probe from `safe_origin` (inside player's upper torso, guaranteed clear)
        // to `ideal_cam`, preventing ANY wall clipping or X-ray block peeking!
        let max_unobstructed_dist = if config.camera_wall_collision {
            Self::probe_safe_boom_distance(world, safe_origin, ideal_cam, right_vec, look_vec)
        } else {
            safe_origin.distance(ideal_cam) as f32
        };

        let target_boom_len = safe_origin.distance(ideal_cam) as f32;
        let allowed_dist = target_boom_len.min(max_unobstructed_dist).max(0.45);

        // Asymmetric filter: snap inward immediately when a wall blocks view; ease outward smoothly!
        let dist_rate = if allowed_dist < self.current_distance {
            44.0
        } else {
            7.5
        };
        self.current_distance += (allowed_dist - self.current_distance) * exp_alpha(dist_rate, dt);

        let boom_dir = (ideal_cam - safe_origin).normalize_or(-look_vec);
        let final_cam_pos = safe_origin + boom_dir * (self.current_distance as f64);

        // Speed-reactive smooth Dynamic FOV
        let raw_fov_bonus = if fov_mult < 0.0 {
            // Subtle Slingshot charge zoom-in tension (-4.5 deg)
            fov_mult * 22.0 * config.dynamic_fov_strength * config.camera_effect_intensity
        } else {
            let curved_speed = speed_norm.powf(1.25);
            (curved_speed * 23.0 * fov_mult * config.dynamic_fov_strength * config.camera_effect_intensity)
                .clamp(0.0, 28.0)
        };
        self.current_fov_bonus += (raw_fov_bonus - self.current_fov_bonus) * exp_alpha(6.8, dt);

        CameraPoseOutput {
            position: final_cam_pos.to_array(),
            yaw: effective_yaw,
            pitch: effective_pitch,
            roll_deg: self.current_roll_deg,
            fov_bonus_deg: self.current_fov_bonus,
            effective_distance: self.current_distance,
        }
    }

    /// Casts a 5-ray camera frustum feeler from `safe_origin` toward `ideal_cam`
    /// and returns the maximum collision-free boom length with a `0.26` block wall cushion.
    fn probe_safe_boom_distance(
        world: &dyn WorldQuery,
        safe_origin: DVec3,
        ideal_cam: DVec3,
        right_vec: DVec3,
        look_vec: DVec3,
    ) -> f32 {
        let span = ideal_cam - safe_origin;
        let full_len = span.length();
        if full_len < 1e-4 {
            return 0.45;
        }
        let boom_dir = span / full_len;
        let cam_up = right_vec.cross(look_vec).normalize_or(DVec3::Y);

        let offsets = [
            DVec3::ZERO,
            right_vec * 0.19 + cam_up * 0.14,
            -right_vec * 0.19 + cam_up * 0.14,
            right_vec * 0.19 - cam_up * 0.14,
            -right_vec * 0.19 - cam_up * 0.14,
        ];

        let mut min_safe = full_len;
        for off in offsets {
            let target = ideal_cam + off;
            let ray_vec = target - safe_origin;
            let ray_len = ray_vec.length();
            if ray_len < 1e-4 {
                continue;
            }
            if let Some(hit) = raycast_blocks(world, safe_origin, ray_vec / ray_len, ray_len + 0.15, true) {
                // Project hit back onto main boom and subtract near-plane wall cushion (0.26 blocks)
                let along_boom = (hit.position - safe_origin).dot(boom_dir) - 0.26;
                min_safe = min_safe.min(along_boom.max(0.35));
            }
        }
        min_safe as f32
    }
}
