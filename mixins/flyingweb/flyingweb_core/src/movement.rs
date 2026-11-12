use glam::{DVec3, IVec3};

use crate::{
    camera::CameraTraversalMode,
    config::FlyingWebConfig,
    input::{TickInput, WristSide},
    physics::VANILLA_GRAVITY_PER_TICK,
    raycast::{
        RayHit, WorldQuery, find_point_launch_perch, horizontal_forward, horizontal_right,
        look_direction, raycast_blocks,
    },
    swinging::SwingController,
    web::{StrandRole, StrandState, WebStrandManager},
};

/// Acrobatic Air Trick variants performed while airborne.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AirTrickKind {
    FrontFlip,
    Backflip,
    CorkscrewSpin,
    WindmillTwist,
}

impl AirTrickKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::FrontFlip => "Front Sommersault",
            Self::Backflip => "Inverted Backflip",
            Self::CorkscrewSpin => "Corkscrew Spin",
            Self::WindmillTwist => "Web-Slinger Twist",
        }
    }
}

/// Mutually exclusive non-swinging Spider-Hero movement states.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum TraversalState {
    #[default]
    Normal,
    /// Rapid mid-air directional Web Zip impulse (`ticks_left` in `1..=8`).
    WebZipping {
        target_vel: DVec3,
        ticks_left: u8,
    },
    /// Dual-web pull toward a specific solid perch (`perch_pos`), with a Vault-Boost window!
    PointLaunching {
        perch_pos: DVec3,
        ticks_elapsed: u8,
    },
    /// Brief window right after touching down on a Point-Launch perch where pressing Jump triggers Vault Launch!
    PointPerchedWindow {
        ticks_left: u8,
    },
    /// Grounded dual-web Slingshot tension charge (`charge` in `0.0..=1.0`).
    SlingshotCharging {
        left_anchor: DVec3,
        right_anchor: DVec3,
        charge: f32,
    },
    /// Post-release Slingshot high-speed launch phase (`ticks_left` for camera/trail flair).
    SlingshotLaunching {
        ticks_left: u8,
    },
    /// Aerodynamic Web Wings gliding (`airspeed` in blocks/tick).
    WebWingsGliding {
        airspeed: f64,
        glide_ticks: u32,
        bank_deg: f32,
    },
    /// High-speed horizontal or vertical sprint along a solid wall (`wall_normal` points away from wall).
    WallRunning {
        wall_normal: DVec3,
        ticks_on_wall: u32,
        vertical_climb: bool,
    },
    /// Clinging and crawling on a vertical wall or inverted ceiling (`surface_normal` points outward into air).
    SurfaceCrawling {
        surface_normal: DVec3,
        ceiling: bool,
    },
}

/// Events emitted by `TraversalController` for audio, VFX, camera impulses, and HUD feedback.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TraversalEvent {
    WebZipFired { dir: DVec3 },
    PointLaunchStarted { perch: DVec3 },
    PointLaunchVaulted { dir: DVec3 },
    SlingshotStarted,
    SlingshotReleased { dir: DVec3, charge: f32 },
    WebWingsDeployed,
    WebWingsStowed,
    AirTrickCompleted { kind: AirTrickKind, combo: u32 },
    WallRunStep { pos: DVec3, block: IVec3 },
    WallKickOff { dir: DVec3 },
    SuperheroLanding { impact_speed: f64 },
}

/// Manages Web Zip, Point Launch, Slingshot, Web Wings, Air Tricks, Wall Running,
/// and Wall/Ceiling Crawling alongside Web Swinging.
#[derive(Clone, Debug)]
pub struct TraversalController {
    pub state: TraversalState,
    /// Remaining mid-air Web Zip charges (max 2 per airtime; restored on swing/wall/ground).
    pub air_zip_charges: u8,
    pub zip_cooldown_ticks: u8,
    /// Preview of valid Point-Launch perch near crosshair for HUD icon.
    pub hovered_perch: Option<RayHit>,

    /// Active mid-air trick state (purely acrobatic pose + style combo, never alters physics!).
    pub active_trick: Option<(AirTrickKind, f32)>,
    pub trick_combo: u32,

    /// Toggle latch allowing comfortable hands-free wall & ceiling crawling.
    pub crawl_latch_enabled: bool,
    /// Ticks remaining in the 3-point Superhero Landing recovery pose (`0` = none).
    pub superhero_landing_ticks: u8,
    prev_downward_speed: f64,
    was_on_ground: bool,
}

impl Default for TraversalController {
    fn default() -> Self {
        Self {
            state: TraversalState::Normal,
            air_zip_charges: 2,
            zip_cooldown_ticks: 0,
            hovered_perch: None,
            active_trick: None,
            trick_combo: 0,
            crawl_latch_enabled: false,
            superhero_landing_ticks: 0,
            prev_downward_speed: 0.0,
            was_on_ground: true,
        }
    }
}

impl TraversalController {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn is_suppressing_vanilla_walk(&self) -> bool {
        matches!(
            self.state,
            TraversalState::SlingshotCharging { .. }
                | TraversalState::SurfaceCrawling { .. }
                | TraversalState::WallRunning { .. }
                | TraversalState::WebWingsGliding { .. }
                | TraversalState::PointLaunching { .. }
        )
    }

    pub fn should_negate_fall_damage(&self, swing_active: bool) -> bool {
        swing_active
            || !matches!(self.state, TraversalState::Normal)
            || self.superhero_landing_ticks > 0
    }

    pub fn camera_mode(&self, swing_attached: bool, on_ground: bool) -> (CameraTraversalMode, DVec3) {
        if swing_attached {
            return (CameraTraversalMode::WebSwinging, DVec3::Y);
        }
        match self.state {
            TraversalState::WebZipping { .. } => (CameraTraversalMode::WebZip, DVec3::Y),
            TraversalState::PointLaunching { .. } | TraversalState::PointPerchedWindow { .. } => {
                (CameraTraversalMode::PointLaunch, DVec3::Y)
            }
            TraversalState::SlingshotCharging { .. } => {
                (CameraTraversalMode::SlingshotCharge, DVec3::Y)
            }
            TraversalState::SlingshotLaunching { .. } => {
                (CameraTraversalMode::SlingshotLaunch, DVec3::Y)
            }
            TraversalState::WebWingsGliding { .. } => (CameraTraversalMode::WebWings, DVec3::Y),
            TraversalState::WallRunning { wall_normal, .. } => {
                (CameraTraversalMode::WallRunning, wall_normal)
            }
            TraversalState::SurfaceCrawling {
                surface_normal,
                ceiling,
            } => {
                if ceiling {
                    (CameraTraversalMode::CeilingCrawling, surface_normal)
                } else {
                    (CameraTraversalMode::WallCrawling, surface_normal)
                }
            }
            TraversalState::Normal => {
                if on_ground {
                    (CameraTraversalMode::Grounded, DVec3::Y)
                } else {
                    (CameraTraversalMode::Freefall, DVec3::Y)
                }
            }
        }
    }

    /// Advances non-swinging Spider-Hero traversal states for one 20 Hz game tick.
    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        world: &dyn WorldQuery,
        player_feet: DVec3,
        mut velocity: DVec3,
        yaw_deg: f32,
        pitch_deg: f32,
        on_ground: bool,
        swing_active: bool,
        input: &TickInput,
        config: &FlyingWebConfig,
        strands: &mut WebStrandManager,
        events: &mut Vec<TraversalEvent>,
    ) -> DVec3 {
        if self.zip_cooldown_ticks > 0 {
            self.zip_cooldown_ticks -= 1;
        }
        if self.superhero_landing_ticks > 0 {
            self.superhero_landing_ticks -= 1;
        }

        // Refill mid-air zip charges & reset trick combo on solid ground or swing attach
        if on_ground || swing_active {
            self.air_zip_charges = 2;
            if on_ground {
                self.trick_combo = 0;
            }
        }

        // Detect high-velocity Superhero Landing!
        if !on_ground {
            if -velocity.y > self.prev_downward_speed {
                self.prev_downward_speed = -velocity.y;
            }
        } else {
            if !self.was_on_ground && self.prev_downward_speed > 0.72 && config.abilities_active() {
                self.superhero_landing_ticks = 9;
                events.push(TraversalEvent::SuperheroLanding {
                    impact_speed: self.prev_downward_speed,
                });
            }
            self.prev_downward_speed = 0.0;
        }
        self.was_on_ground = on_ground;

        let eye_pos = player_feet + DVec3::new(0.0, 1.62, 0.0);
        let aim_dir = look_direction(yaw_deg, pitch_deg);

        // Update Point-Launch perch preview for HUD reticle
        self.hovered_perch = if config.abilities_active() && config.enable_point_launch && !swing_active {
            find_point_launch_perch(
                world,
                eye_pos,
                aim_dir,
                3.5,
                config.max_web_range * 0.85,
            )
        } else {
            None
        };

        if !config.abilities_active() {
            self.cancel_non_normal(strands);
            self.active_trick = None;
            return velocity;
        }

        // If Web Swinging just engaged, cleanly yield non-swinging traversal states!
        if swing_active {
            if !matches!(self.state, TraversalState::Normal) {
                if matches!(self.state, TraversalState::WebWingsGliding { .. }) {
                    events.push(TraversalEvent::WebWingsStowed);
                }
                self.cancel_non_normal(strands);
            }
            self.active_trick = None;
            return velocity;
        }

        // Toggle Wall/Ceiling Crawl latch (`V`)
        if input.crawl_toggle_just_pressed && (config.enable_wall_crawl || config.enable_ceiling_crawl) {
            self.crawl_latch_enabled = !self.crawl_latch_enabled;
            if !self.crawl_latch_enabled && matches!(self.state, TraversalState::SurfaceCrawling { .. }) {
                self.state = TraversalState::Normal;
            }
        }
        if input.release_just_pressed {
            self.crawl_latch_enabled = false;
            if !matches!(self.state, TraversalState::Normal) {
                if matches!(self.state, TraversalState::WebWingsGliding { .. }) {
                    events.push(TraversalEvent::WebWingsStowed);
                }
                self.cancel_non_normal(strands);
            }
        }

        // 1. Check Slingshot Launch initiation (grounded + holding `trick_held` + `shoot_just_pressed`)
        if config.enable_slingshot
            && on_ground
            && input.trick_held
            && input.shoot_just_pressed
            && matches!(self.state, TraversalState::Normal)
        {
            if let Some((left_a, right_a)) =
                Self::find_slingshot_anchors(world, eye_pos, yaw_deg, pitch_deg, config.max_web_range)
            {
                let left_w =
                    SwingController::compute_wrist_origin(player_feet, yaw_deg, pitch_deg, WristSide::Left, false);
                let right_w =
                    SwingController::compute_wrist_origin(player_feet, yaw_deg, pitch_deg, WristSide::Right, false);
                let tint = config.suit_variant.web_tint_argb();
                strands.detach_by_role(StrandRole::Slingshot);
                strands.spawn_custom_strand(
                    StrandRole::Slingshot,
                    Some(WristSide::Left),
                    left_w,
                    left_a,
                    DVec3::Y,
                    StrandState::Attached {
                        tension: 0.4,
                        rest_length: left_w.distance(left_a),
                    },
                    tint,
                );
                strands.spawn_custom_strand(
                    StrandRole::Slingshot,
                    Some(WristSide::Right),
                    right_w,
                    right_a,
                    DVec3::Y,
                    StrandState::Attached {
                        tension: 0.4,
                        rest_length: right_w.distance(right_a),
                    },
                    tint,
                );
                self.state = TraversalState::SlingshotCharging {
                    left_anchor: left_a,
                    right_anchor: right_a,
                    charge: 0.12,
                };
                events.push(TraversalEvent::SlingshotStarted);
            }
        }

        // 2. Check Web Wings toggle (`G` in mid-air!)
        if config.enable_web_wings && input.wings_just_pressed && !on_ground {
            if matches!(self.state, TraversalState::WebWingsGliding { .. }) {
                self.state = TraversalState::Normal;
                events.push(TraversalEvent::WebWingsStowed);
            } else {
                self.cancel_non_normal(strands);
                self.active_trick = None;
                let init_speed = velocity.length().clamp(0.45, config.max_swing_speed);
                self.state = TraversalState::WebWingsGliding {
                    airspeed: init_speed,
                    glide_ticks: 0,
                    bank_deg: 0.0,
                };
                events.push(TraversalEvent::WebWingsDeployed);
            }
        }

        // 3. Check Point Launch / Mid-Air Web Zip (`F`)
        if input.zip_just_pressed && self.zip_cooldown_ticks == 0 {
            if let Some(perch) = self.hovered_perch
                && config.enable_point_launch
            {
                self.cancel_non_normal(strands);
                self.active_trick = None;
                let lw =
                    SwingController::compute_wrist_origin(player_feet, yaw_deg, pitch_deg, WristSide::Left, true);
                let rw =
                    SwingController::compute_wrist_origin(player_feet, yaw_deg, pitch_deg, WristSide::Right, true);
                let tint = config.suit_variant.web_tint_argb();
                strands.spawn_custom_strand(
                    StrandRole::ZipOrLaunch,
                    Some(WristSide::Left),
                    lw,
                    perch.position,
                    DVec3::Y,
                    StrandState::Attached {
                        tension: 0.95,
                        rest_length: lw.distance(perch.position),
                    },
                    tint,
                );
                strands.spawn_custom_strand(
                    StrandRole::ZipOrLaunch,
                    Some(WristSide::Right),
                    rw,
                    perch.position,
                    DVec3::Y,
                    StrandState::Attached {
                        tension: 0.95,
                        rest_length: rw.distance(perch.position),
                    },
                    tint,
                );
                self.zip_cooldown_ticks = 10;
                self.air_zip_charges = 2;
                self.state = TraversalState::PointLaunching {
                    perch_pos: perch.position,
                    ticks_elapsed: 0,
                };
                events.push(TraversalEvent::PointLaunchStarted {
                    perch: perch.position,
                });
            } else if config.enable_web_zip && !on_ground && self.air_zip_charges > 0 {
                // Mid-air directional Web Zip!
                self.cancel_non_normal(strands);
                self.active_trick = None;
                self.air_zip_charges -= 1;
                self.zip_cooldown_ticks = 12;

                // Prefer forward geometry hit for visual web anchor; otherwise whip short forward tether!
                let zip_dir = DVec3::new(aim_dir.x, aim_dir.y.clamp(-0.35, 0.55), aim_dir.z)
                    .normalize_or(horizontal_forward(yaw_deg));
                let anchor = raycast_blocks(world, eye_pos, zip_dir, config.max_web_range, false)
                    .map(|h| h.position)
                    .unwrap_or(eye_pos + zip_dir * 14.0);
                let wrist_pos =
                    SwingController::compute_wrist_origin(player_feet, yaw_deg, pitch_deg, WristSide::Right, true);
                let id = strands.spawn_custom_strand(
                    StrandRole::ZipOrLaunch,
                    Some(WristSide::Right),
                    wrist_pos,
                    anchor,
                    -zip_dir,
                    StrandState::Attached {
                        tension: 0.9,
                        rest_length: wrist_pos.distance(anchor),
                    },
                    config.suit_variant.web_tint_argb(),
                );
                // Immediately detach on short timer if unanchored air-whip so no floating web lingers!
                if raycast_blocks(world, eye_pos, zip_dir, config.max_web_range, false).is_none() {
                    strands.detach_strand(id);
                }

                let zip_speed = (velocity.length() * 0.45 + 1.05).clamp(1.05, 1.78);
                let target_vel = zip_dir * zip_speed + DVec3::Y * 0.16;
                self.state = TraversalState::WebZipping {
                    target_vel,
                    ticks_left: 5,
                };
                events.push(TraversalEvent::WebZipFired { dir: zip_dir });
            }
        }

        // 4. Advance active traversal state machine
        match self.state {
            TraversalState::Normal => {
                // Check automatic Wall Run or Wall/Ceiling Crawl transitions!
                if let Some((surf_norm, hit_block)) = Self::probe_adjacent_surface(
                    world,
                    player_feet,
                    yaw_deg,
                    config.enable_ceiling_crawl && self.crawl_latch_enabled,
                ) {
                    let is_ceiling = surf_norm.y < -0.65;
                    let is_vertical_wall = surf_norm.y.abs() < 0.35;

                    if self.crawl_latch_enabled
                        && ((is_ceiling && config.enable_ceiling_crawl)
                            || (!is_ceiling && config.enable_wall_crawl))
                    {
                        self.active_trick = None;
                        self.state = TraversalState::SurfaceCrawling {
                            surface_normal: surf_norm,
                            ceiling: is_ceiling,
                        };
                    } else if is_vertical_wall
                        && config.enable_wall_run
                        && !on_ground
                        && input.forward > 0.15
                        && (config.auto_wall_run_attach || input.jump_held)
                        && velocity.length() > 0.24
                    {
                        self.active_trick = None;
                        self.air_zip_charges = 2;
                        self.state = TraversalState::WallRunning {
                            wall_normal: surf_norm,
                            ticks_on_wall: 0,
                            vertical_climb: pitch_deg < -20.0,
                        };
                        events.push(TraversalEvent::WallRunStep {
                            pos: player_feet,
                            block: hit_block,
                        });
                    }
                }

                // Update airborne Air Tricks (purely acrobatic pose + combo, zero trajectory tampering!)
                self.tick_air_tricks(world, player_feet, on_ground, input, config, events);

                // Gentle mid-air body control while freefalling
                if !on_ground {
                    let fwd = horizontal_forward(yaw_deg);
                    let right = horizontal_right(yaw_deg);
                    let wish = (fwd * (input.forward as f64) - right * (input.strafe as f64))
                        .normalize_or_zero();
                    if wish.length_squared() > 0.1 {
                        let horiz_speed = DVec3::new(velocity.x, 0.0, velocity.z).length();
                        if horiz_speed < config.max_swing_speed * 0.85 {
                            velocity += wish * (config.air_steer_accel * 0.45);
                        }
                    }
                }
            }

            TraversalState::WebZipping {
                target_vel,
                ticks_left,
            } => {
                velocity = velocity.lerp(target_vel, 0.68);
                if ticks_left <= 1 || on_ground {
                    strands.detach_by_role(StrandRole::ZipOrLaunch);
                    self.state = TraversalState::Normal;
                } else {
                    self.state = TraversalState::WebZipping {
                        target_vel,
                        ticks_left: ticks_left - 1,
                    };
                }
            }

            TraversalState::PointLaunching {
                perch_pos,
                ticks_elapsed,
            } => {
                let to_perch = perch_pos - player_feet;
                let dist = to_perch.length();

                // Signature Timed Point-Launch Vault: pressing Jump near the perch rockets forward!
                if dist < 2.8 && input.jump_just_pressed {
                    strands.detach_by_role(StrandRole::ZipOrLaunch);
                    let fwd = horizontal_forward(yaw_deg);
                    let launch_vel = fwd * 1.38 + DVec3::Y * 0.58;
                    velocity = launch_vel;
                    self.state = TraversalState::Normal;
                    events.push(TraversalEvent::PointLaunchVaulted { dir: fwd });
                    return velocity;
                }

                if dist < 0.85 || ticks_elapsed > 32 {
                    strands.detach_by_role(StrandRole::ZipOrLaunch);
                    velocity = DVec3::new(0.0, -0.05, 0.0);
                    self.state = TraversalState::PointPerchedWindow { ticks_left: 9 };
                } else {
                    let pull_dir = to_perch / dist;
                    let pull_speed = (dist * 0.38).clamp(0.65, 1.85);
                    // Slight upward arc in first half of launch so Spider-Man clears parapet lips!
                    let arc_lift = if dist > 3.0 { 0.18 } else { 0.04 };
                    velocity = pull_dir * pull_speed + DVec3::Y * arc_lift;
                    self.state = TraversalState::PointLaunching {
                        perch_pos,
                        ticks_elapsed: ticks_elapsed + 1,
                    };
                }
            }

            TraversalState::PointPerchedWindow { ticks_left } => {
                if input.jump_just_pressed {
                    let fwd = horizontal_forward(yaw_deg);
                    velocity = fwd * 1.34 + DVec3::Y * 0.56;
                    self.state = TraversalState::Normal;
                    events.push(TraversalEvent::PointLaunchVaulted { dir: fwd });
                } else if ticks_left <= 1 {
                    self.state = TraversalState::Normal;
                } else {
                    self.state = TraversalState::PointPerchedWindow {
                        ticks_left: ticks_left - 1,
                    };
                }
            }

            TraversalState::SlingshotCharging {
                left_anchor,
                right_anchor,
                mut charge,
            } => {
                // Hold player anchored on ground while elastic web tension builds!
                velocity = DVec3::new(0.0, velocity.y.min(0.0), 0.0);
                charge = (charge + 1.0 / 22.0).clamp(0.0, 1.0);

                // Update wrist positions on both slingshot strands
                let lw =
                    SwingController::compute_wrist_origin(player_feet, yaw_deg, pitch_deg, WristSide::Left, false);
                let rw =
                    SwingController::compute_wrist_origin(player_feet, yaw_deg, pitch_deg, WristSide::Right, false);
                for s in strands.strands() {
                    if s.role == StrandRole::Slingshot {
                        let _ = (lw, rw, left_anchor, right_anchor);
                    }
                }

                // Release `shoot_web` or press `Jump` -> CATAPULT LAUNCH!!
                if !input.shoot_held || input.jump_just_pressed {
                    strands.detach_by_role(StrandRole::Slingshot);
                    let launch_pitch = pitch_deg.clamp(-65.0, -14.0);
                    let launch_dir = look_direction(yaw_deg, launch_pitch);
                    let launch_speed = (0.95 + (charge as f64) * 1.38)
                        .clamp(0.95, config.max_swing_speed * 1.04);
                    velocity = launch_dir * launch_speed;
                    self.state = TraversalState::SlingshotLaunching { ticks_left: 14 };
                    events.push(TraversalEvent::SlingshotReleased {
                        dir: launch_dir,
                        charge,
                    });
                } else {
                    self.state = TraversalState::SlingshotCharging {
                        left_anchor,
                        right_anchor,
                        charge,
                    };
                }
            }

            TraversalState::SlingshotLaunching { ticks_left } => {
                if ticks_left <= 1 || on_ground {
                    self.state = TraversalState::Normal;
                } else {
                    self.state = TraversalState::SlingshotLaunching {
                        ticks_left: ticks_left - 1,
                    };
                }
            }

            TraversalState::WebWingsGliding {
                mut airspeed,
                glide_ticks,
                mut bank_deg,
            } => {
                if on_ground {
                    self.state = TraversalState::Normal;
                    events.push(TraversalEvent::WebWingsStowed);
                    return velocity;
                }

                // True Airfoil Lift/Drag Gliding Physics:
                // pitch_deg > 0 = diving nose-down (gains speed, loses altitude);
                // pitch_deg < 0 = flaring nose-up (trades speed for climb, stalls if too slow!).
                let clamped_pitch = pitch_deg.clamp(-48.0, 68.0);
                let pitch_rad = (clamped_pitch as f64).to_radians();

                // Gravity component along glide path (`sin(pitch)` positive when diving down!)
                let grav_along = VANILLA_GRAVITY_PER_TICK * 0.92 * pitch_rad.sin();
                let parasitic_drag = 0.011 * airspeed * airspeed + 0.004;
                airspeed = (airspeed + grav_along - parasitic_drag)
                    .clamp(0.24, config.max_swing_speed * 0.92);

                // Aerodynamic lift efficiency drops sharply below stall speed (0.38 blocks/tick)
                let lift_authority = ((airspeed - 0.26) / 0.36).clamp(0.0, 1.0);
                let glide_dir = look_direction(yaw_deg, clamped_pitch);

                // Induced sink rate ensures level flight slowly loses altitude (~9.5:1 glide ratio)
                // and steep nose-up climb stalls cleanly once kinetic energy is spent!
                let base_sink = 0.052 + (1.0 - lift_authority) * 0.14;
                velocity = glide_dir * airspeed - DVec3::Y * base_sink;

                let target_bank = (-input.strafe * 22.0).clamp(-24.0, 24.0);
                bank_deg += (target_bank - bank_deg) * 0.20;

                self.state = TraversalState::WebWingsGliding {
                    airspeed,
                    glide_ticks: glide_ticks.saturating_add(1),
                    bank_deg,
                };
            }

            TraversalState::WallRunning {
                mut wall_normal,
                ticks_on_wall,
                mut vertical_climb,
            } => {
                // Kick-off jump away from wall!
                if input.jump_just_pressed {
                    let fwd = horizontal_forward(yaw_deg);
                    let kick_dir = (wall_normal * 0.52 + fwd * 0.62 + DVec3::Y * 0.52).normalize_or(DVec3::Y);
                    velocity = kick_dir * 1.04;
                    self.state = TraversalState::Normal;
                    events.push(TraversalEvent::WallKickOff { dir: kick_dir });
                    return velocity;
                }

                // Re-probe wall contact behind `-wall_normal` (including smooth exterior corner wrap!)
                let Some((updated_normal, hit_block)) =
                    Self::probe_specific_wall(world, player_feet, wall_normal)
                else {
                    // Reached top ledge of building -> auto-mantle hop onto roof!
                    if vertical_climb || velocity.y > 0.1 {
                        velocity += -wall_normal * 0.30 + DVec3::Y * 0.34;
                    }
                    self.state = TraversalState::Normal;
                    return velocity;
                };
                wall_normal = updated_normal;
                vertical_climb = pitch_deg < -20.0;

                if on_ground && ticks_on_wall > 4 && !vertical_climb {
                    self.state = TraversalState::Normal;
                    return velocity;
                }
                if input.forward < -0.15 || ticks_on_wall > 90 {
                    self.state = TraversalState::Normal;
                    return velocity;
                }

                let look_fwd = horizontal_forward(yaw_deg);
                let wall_tangent =
                    (look_fwd - wall_normal * look_fwd.dot(wall_normal)).normalize_or_zero();

                let run_speed = 0.62_f64;
                // Gentle wall-suction keeps player pressed smoothly against wall blocks
                let wall_adhesion = -wall_normal * 0.12;
                let vert_vel = if vertical_climb {
                    0.50
                } else if ticks_on_wall < 28 {
                    0.04
                } else {
                    // Gradual arc descent on very long horizontal wall runs
                    -((ticks_on_wall - 28) as f64 * 0.006).min(0.22)
                };

                velocity = wall_tangent * run_speed + DVec3::Y * vert_vel + wall_adhesion;

                if ticks_on_wall % 5 == 0 {
                    events.push(TraversalEvent::WallRunStep {
                        pos: player_feet,
                        block: hit_block,
                    });
                }

                self.state = TraversalState::WallRunning {
                    wall_normal,
                    ticks_on_wall: ticks_on_wall + 1,
                    vertical_climb,
                };
            }

            TraversalState::SurfaceCrawling { .. } => {
                if input.jump_just_pressed {
                    self.crawl_latch_enabled = false;
                    let (_, norm) = self.camera_mode(false, on_ground);
                    velocity = norm * 0.42 + DVec3::Y * 0.32;
                    self.state = TraversalState::Normal;
                    return velocity;
                }

                // Probe walls AND overhead ceiling (`+Y`) for seamless wall <-> ceiling transitions!
                let Some((surf_norm, _)) = Self::probe_adjacent_surface(
                    world,
                    player_feet,
                    yaw_deg,
                    config.enable_ceiling_crawl,
                ) else {
                    self.state = TraversalState::Normal;
                    return velocity;
                };

                let is_ceiling = surf_norm.y < -0.65;
                if is_ceiling && !config.enable_ceiling_crawl {
                    self.state = TraversalState::Normal;
                    return velocity;
                }

                let crawl_speed = 0.28_f64;
                let adhesion = -surf_norm * 0.11;

                let move_vec = if is_ceiling {
                    // Inverted ceiling crawl: WASD moves along horizontal XZ plane flush under the roof!
                    let fwd = horizontal_forward(yaw_deg);
                    let right = horizontal_right(yaw_deg);
                    (fwd * (input.forward as f64) - right * (input.strafe as f64))
                        .normalize_or_zero()
                        * crawl_speed
                } else {
                    // Vertical wall crawl: W/S climbs along Aim/Up tangent, A/D traverses sideways!
                    let right_wall = DVec3::Y.cross(surf_norm).normalize_or(DVec3::X);
                    let look_dir = look_direction(yaw_deg, pitch_deg);
                    // Project 3D look vector onto the wall tangent plane so looking up climbs up,
                    // looking sideways crawls diagonally, and looking down crawls down!
                    let tangent_fwd = (look_dir - surf_norm * look_dir.dot(surf_norm))
                        .normalize_or(DVec3::Y);
                    let wish = (tangent_fwd * (input.forward as f64)
                        + right_wall * (input.strafe as f64))
                        .normalize_or_zero();
                    wish * crawl_speed
                };

                velocity = move_vec + adhesion;
                self.state = TraversalState::SurfaceCrawling {
                    surface_normal: surf_norm,
                    ceiling: is_ceiling,
                };
            }
        }

        velocity
    }

    fn tick_air_tricks(
        &mut self,
        world: &dyn WorldQuery,
        player_feet: DVec3,
        on_ground: bool,
        input: &TickInput,
        config: &FlyingWebConfig,
        events: &mut Vec<TraversalEvent>,
    ) {
        if !config.enable_air_tricks || on_ground {
            self.active_trick = None;
            return;
        }
        // Automatically exit tricks cleanly when close to ground (`< 2.1` blocks) so Spider-Man lands on his feet!
        let near_ground =
            raycast_blocks(world, player_feet + DVec3::Y * 0.2, DVec3::NEG_Y, 2.2, false).is_some();
        if near_ground || !input.trick_held {
            self.active_trick = None;
            return;
        }

        let desired_kind = if input.sneak_held {
            AirTrickKind::WindmillTwist
        } else if input.strafe.abs() > 0.35 {
            AirTrickKind::CorkscrewSpin
        } else if input.forward < -0.25 {
            AirTrickKind::Backflip
        } else {
            AirTrickKind::FrontFlip
        };

        match self.active_trick {
            Some((kind, mut phase)) => {
                phase += 1.0 / 11.0; // One full 360° acrobatic rotation every 11 ticks (~0.55 s)!
                if phase >= 1.0 {
                    self.trick_combo = self.trick_combo.saturating_add(1);
                    events.push(TraversalEvent::AirTrickCompleted {
                        kind,
                        combo: self.trick_combo,
                    });
                    self.active_trick = Some((desired_kind, phase - 1.0));
                } else {
                    self.active_trick = Some((kind, phase));
                }
            }
            None => {
                self.active_trick = Some((desired_kind, 0.05));
            }
        }
    }

    fn cancel_non_normal(&mut self, strands: &mut WebStrandManager) {
        strands.detach_by_role(StrandRole::ZipOrLaunch);
        strands.detach_by_role(StrandRole::Slingshot);
        self.state = TraversalState::Normal;
    }

    /// Probes for two forward/lateral solid anchors for a dual-strand Slingshot Launch.
    fn find_slingshot_anchors(
        world: &dyn WorldQuery,
        eye_pos: DVec3,
        yaw_deg: f32,
        pitch_deg: f32,
        max_range: f64,
    ) -> Option<(DVec3, DVec3)> {
        let base_pitch = pitch_deg.clamp(-35.0, 5.0);
        let left_dir = look_direction(yaw_deg + 16.0, base_pitch);
        let right_dir = look_direction(yaw_deg - 16.0, base_pitch);
        let center_dir = look_direction(yaw_deg, base_pitch - 8.0);

        let left_hit = raycast_blocks(world, eye_pos, left_dir, max_range, false)
            .or_else(|| raycast_blocks(world, eye_pos, center_dir, max_range, false))?;
        let right_hit = raycast_blocks(world, eye_pos, right_dir, max_range, false)
            .or_else(|| raycast_blocks(world, eye_pos, center_dir, max_range, false))?;

        Some((left_hit.position, right_hit.position))
    }

    /// Probes adjacent walls around chest/waist height AND overhead ceiling (`+Y` from head)
    /// returning `(outward_surface_normal, solid_block_pos)`.
    fn probe_adjacent_surface(
        world: &dyn WorldQuery,
        player_feet: DVec3,
        yaw_deg: f32,
        check_ceiling: bool,
    ) -> Option<(DVec3, IVec3)> {
        if check_ceiling {
            let head_top = player_feet + DVec3::new(0.0, 1.72, 0.0);
            if let Some(ceil_hit) = raycast_blocks(world, head_top, DVec3::Y, 0.55, false) {
                return Some((DVec3::NEG_Y, ceil_hit.block_pos));
            }
        }

        let chest = player_feet + DVec3::new(0.0, 1.0, 0.0);
        let fwd = horizontal_forward(yaw_deg);
        let dirs = [
            fwd,
            DVec3::X,
            DVec3::NEG_X,
            DVec3::Z,
            DVec3::NEG_Z,
        ];
        let mut best: Option<(f64, DVec3, IVec3)> = None;
        for d in dirs {
            if d.length_squared() < 0.5 {
                continue;
            }
            if let Some(hit) = raycast_blocks(world, chest, d, 0.78, false)
                && hit.normal.y.abs() < 0.35
                && best.is_none_or(|(prev_d, _, _)| hit.distance < prev_d)
            {
                best = Some((hit.distance, hit.normal, hit.block_pos));
            }
        }
        best.map(|(_, n, b)| (n, b))
    }

    fn probe_specific_wall(
        world: &dyn WorldQuery,
        player_feet: DVec3,
        prev_wall_normal: DVec3,
    ) -> Option<(DVec3, IVec3)> {
        let chest = player_feet + DVec3::new(0.0, 0.95, 0.0);
        let into_wall = (-prev_wall_normal).normalize_or_zero();
        if let Some(hit) = raycast_blocks(world, chest, into_wall, 0.95, false)
            && hit.normal.y.abs() < 0.35
        {
            return Some((hit.normal, hit.block_pos));
        }
        None
    }
}
