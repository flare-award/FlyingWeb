use glam::DVec3;

use crate::config::FlyingWebConfig;

/// Vanilla Horizon `PlayerMotion` base gravitational acceleration per tick (`motion.rs`).
pub const VANILLA_GRAVITY_PER_TICK: f64 = 0.08;

/// Sub-steps per 50 ms game tick for velocity-Verlet + XPBD tether constraint integration.
const SUBSTEPS: usize = 4;

/// Physical state of a single active pendulum tether constraint.
#[derive(Clone, Copy, Debug)]
pub struct PendulumConstraint {
    pub anchor: DVec3,
    pub anchor_normal: DVec3,
    pub max_length: f64,
    pub min_length: f64,
    /// Instantaneous normalized tension `0.0..=1.0` computed by the solver.
    pub tension: f32,
    /// True when distance >= 98.5% of `max_length`.
    pub is_taut: bool,
}

/// Inputs to one 20 Hz pendulum physics step.
#[derive(Clone, Copy, Debug)]
pub struct PendulumStepInput {
    /// Player center-of-mass / chest pivot (`feet + (0, 1.05, 0)`).
    pub pivot_pos: DVec3,
    /// Authoritative pre-step velocity in blocks/tick.
    pub velocity: DVec3,
    /// Unit wish direction on XZ / aim space from WASD + camera yaw.
    pub wish_dir: DVec3,
    /// Forward input (-1..1).
    pub forward_input: f32,
    /// Reel delta in blocks this tick (`+` shorten, `-` lengthen).
    pub reel_delta: f64,
    /// Distance from player feet to nearest solid ground directly below (`None` if > 6 blocks).
    pub ground_clearance: Option<f64>,
    pub on_ground: bool,
    pub sneak_held: bool,
}

/// Result of one 20 Hz pendulum physics step.
#[derive(Clone, Copy, Debug)]
pub struct PendulumStepOutput {
    pub new_velocity: DVec3,
    pub updated_constraint: PendulumConstraint,
    /// Peak speed reached during the step (blocks/tick).
    pub speed: f64,
}

/// Advances a 3D pendulum web-swinging state by one 20 Hz game tick using
/// `SUBSTEPS` velocity-Verlet + unilateral XPBD rope constraint iterations.
///
/// Key physical guarantees:
/// - **True pendulum energy exchange**: downward descent under gravity accelerates
///   tangential speed; upward arc climb decelerates speed back into height.
/// - **Unilateral flexible web**: slack when `distance < max_length`, taut when
///   `distance >= max_length` (never pushes outward like a rigid pole).
/// - **Bounded pumping & quadratic drag**: mid-swing steering/pumping tapers to zero
///   at `max_swing_speed` and quadratic air drag prevents infinite-speed exploits.
pub fn step_pendulum(
    mut constraint: PendulumConstraint,
    input: PendulumStepInput,
    config: &FlyingWebConfig,
) -> PendulumStepOutput {
    let dt = 1.0 / (SUBSTEPS as f64);

    // Apply user reel-in / pay-out smoothly.
    if input.reel_delta.abs() > 1e-5 {
        constraint.max_length = (constraint.max_length - input.reel_delta)
            .clamp(constraint.min_length, config.max_web_range);
    }

    // Auto slack take-up when diving forward beneath an overhead anchor so entering
    // a swing from freefall catches smoothly at the bottom of the arc.
    let initial_vec = input.pivot_pos - constraint.anchor;
    let initial_dist = initial_vec.length();
    if initial_dist < constraint.max_length - 0.35 && input.velocity.y < -0.05 {
        let target_len = initial_dist.max(constraint.min_length);
        constraint.max_length =
            constraint.max_length + (target_len - constraint.max_length) * 0.32;
    }

    // Optional Insomniac street-skim ground clearance assist: subtly shortens a very low
    // overhead tether so Spider-Man skims 0.55 blocks above flat streets instead of dragging.
    if config.auto_ground_clearance
        && !input.sneak_held
        && !input.on_ground
        && let Some(clearance) = input.ground_clearance
        && clearance < 1.35
        && constraint.anchor.y > input.pivot_pos.y + 3.0
    {
        let needed_shorten = (1.35 - clearance) * 0.28;
        constraint.max_length =
            (constraint.max_length - needed_shorten).max(constraint.min_length);
    }

    let mut pos = input.pivot_pos;
    let mut vel = input.velocity;
    if !vel.is_finite() {
        vel = DVec3::ZERO;
    }

    let gravity = DVec3::new(
        0.0,
        -VANILLA_GRAVITY_PER_TICK * config.swing_gravity_scale,
        0.0,
    );
    let mut accum_tension = 0.0_f64;
    let mut any_taut = false;

    for _ in 0..SUBSTEPS {
        // 1. Gravitational acceleration (converts potential <-> kinetic energy along the arc)
        vel += gravity * dt;

        // 2. Trial position advance
        let trial_pos = pos + vel * dt;
        let rel = trial_pos - constraint.anchor;
        let dist = rel.length();

        if dist > 1e-6 {
            let radial_out = rel / dist; // unit vector pointing from anchor -> player

            // 3. Athletic mid-swing tangential pumping & lateral bank steering
            if input.wish_dir.length_squared() > 1e-4 {
                let tangent_wish =
                    (input.wish_dir - radial_out * input.wish_dir.dot(radial_out)).normalize_or_zero();
                let speed_ratio = (vel.length() / config.max_swing_speed).clamp(0.0, 1.25);
                // Smoothly taper pump thrust to 0 as speed approaches max_swing_speed!
                let pump_headroom = (1.0 - speed_ratio * speed_ratio).max(0.0);

                // Pumping is strongest through the middle/downswing of the pendulum arc
                // (`radial_out.y < -0.15` means player is below the anchor).
                let below_anchor_factor = (-radial_out.y).clamp(0.15, 1.0);
                let forward_pump = if input.forward_input > 0.05 {
                    config.swing_pump_accel * pump_headroom * below_anchor_factor
                } else {
                    config.air_steer_accel * pump_headroom * 0.75
                };
                // Cross-arc lateral steering remains responsive even at high speed without adding energy!
                let current_dir = vel.normalize_or_zero();
                let lateral_wish =
                    tangent_wish - current_dir * tangent_wish.dot(current_dir).max(0.0);

                vel += tangent_wish * (forward_pump * dt)
                    + lateral_wish * (config.air_steer_accel * 0.65 * dt);
            }

            // 4. Unilateral Rope / Web Constraint (taut when dist >= max_length)
            if dist >= constraint.max_length {
                any_taut = true;
                let stretch = dist - constraint.max_length;
                let v_radial = vel.dot(radial_out);

                // Cancel outward radial velocity so momentum redirects cleanly along the arc tangent.
                if v_radial > 0.0 {
                    vel -= radial_out * v_radial;
                }

                // Soft-compliant XPBD position restore pull inward along `-radial_out`
                // (clamped to prevent explosive snap if an obstacle temporarily displaced the player).
                let pull_speed = ((stretch * config.web_stiffness) / dt).clamp(0.0, 0.85);
                vel -= radial_out * pull_speed;

                // Centripetal + gravitational rope tension metric: T = v_t^2 / L - g . r_hat
                let v_tan_sq = (vel - radial_out * vel.dot(radial_out)).length_squared();
                let centripetal = v_tan_sq / constraint.max_length.max(1.5);
                let grav_proj = (-gravity.dot(radial_out)).max(0.0);
                let raw_t = (centripetal + grav_proj) / 0.22;
                accum_tension += raw_t.clamp(0.05, 1.0);
            } else if dist >= constraint.max_length * 0.975 {
                any_taut = true;
                accum_tension += 0.25;
            }
        }

        // 5. Quadratic aerodynamic drag + smooth asymptotic speed cap
        let speed = vel.length();
        if speed > 1e-6 {
            let drag_loss = (config.aerodynamic_drag * speed * dt).clamp(0.0, 0.18);
            vel *= 1.0 - drag_loss;

            let post_speed = vel.length();
            if post_speed > config.max_swing_speed {
                // Smooth exponential bleed toward max_swing_speed rather than harsh brick-wall clip.
                let excess = post_speed - config.max_swing_speed;
                let target_speed = config.max_swing_speed + excess * (1.0 - 0.35 * dt);
                vel = (vel / post_speed) * target_speed;
            }
        }

        pos += vel * dt;
    }

    constraint.is_taut = any_taut;
    constraint.tension = ((accum_tension / (SUBSTEPS as f64)) as f32).clamp(0.0, 1.0);

    let final_speed = vel.length();
    PendulumStepOutput {
        new_velocity: vel,
        updated_constraint: constraint,
        speed: final_speed,
    }
}

/// Computes the velocity vector upon releasing a web swing:
/// - Always preserves 100% of current tangential swing momentum.
/// - When `jump_release` is true (Spider-Man's signature **Swing Jump** at the upswing!),
///   adds an athletic forward-and-upward launch impulse scaled by current arc speed so
///   releasing near the 30°–60° upswing vaults cleanly over rooftops without free energy from standstill.
pub fn compute_release_velocity(
    current_velocity: DVec3,
    look_forward_flat: DVec3,
    jump_release: bool,
    config: &FlyingWebConfig,
) -> DVec3 {
    if !jump_release || config.release_jump_boost <= 0.0 {
        return current_velocity;
    }

    let speed = current_velocity.length();
    // Scale boost by actual swing momentum (minimum small hop if slow, full vault at speed).
    let momentum_factor = (speed / 1.15).clamp(0.30, 1.15);
    let boost_mag = config.release_jump_boost * momentum_factor;

    let horiz_vel = DVec3::new(current_velocity.x, 0.0, current_velocity.z);
    let forward_dir = if horiz_vel.length_squared() > 0.04 {
        horiz_vel.normalize()
    } else if look_forward_flat.length_squared() > 0.04 {
        look_forward_flat.normalize()
    } else {
        DVec3::Z
    };

    let mut boosted = current_velocity
        + forward_dir * (boost_mag * 0.62)
        + DVec3::Y * (boost_mag * 0.78);

    // Respect soft speed ceiling so chain-jumping cannot diverge to infinite speed.
    let hard_cap = config.max_swing_speed * 1.12;
    let new_speed = boosted.length();
    if new_speed > hard_cap {
        boosted = (boosted / new_speed) * hard_cap;
    }
    boosted
}
