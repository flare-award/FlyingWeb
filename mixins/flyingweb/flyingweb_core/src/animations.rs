use crate::{
    input::WristSide,
    movement::{AirTrickKind, TraversalState},
    swinging::SwingPhase,
};

/// 3D limb Euler angles in degrees (`[pitch_x, yaw_y, roll_z]`) plus body root transforms
/// for rendering the articulated Spider-Hero skin-layer model.
#[derive(Clone, Copy, Debug, Default)]
pub struct SpiderRigPose {
    /// Vertical root translation in blocks (e.g. crouch on Superhero Landing or ceiling offset).
    pub root_offset_y: f32,
    /// Whole-body pitch/yaw/roll in degrees around chest center (`y = 0.95` blocks).
    pub body_rot: [f32; 3],
    pub head_rot: [f32; 3],
    pub right_arm_rot: [f32; 3],
    pub left_arm_rot: [f32; 3],
    pub right_leg_rot: [f32; 3],
    pub left_leg_rot: [f32; 3],
    /// Underarm Web-Wings membrane deployment `0.0..=1.0`.
    pub web_wings_deploy: f32,
    /// True when the pose differs meaningfully from vanilla idle/walk.
    pub custom_pose_active: bool,
    /// First-person right/left arm pitch lift (`0.0..=1.0`) for `native_render_hand_pose`.
    pub fp_right_shot: f32,
    pub fp_left_shot: f32,
}

fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_rot(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        lerp_f32(a[0], b[0], t),
        lerp_f32(a[1], b[1], t),
        lerp_f32(a[2], b[2], t),
    ]
}

/// Inputs used to synthesize the target Spider-Hero pose for the current frame.
#[derive(Clone, Copy, Debug)]
pub struct PoseEvalInput {
    pub swing_phase: SwingPhase,
    pub traversal: TraversalState,
    pub active_trick: Option<(AirTrickKind, f32)>,
    pub superhero_landing_ticks: u8,
    pub on_ground: bool,
    pub speed_blocks_per_tick: f64,
    pub vertical_vel: f64,
    pub lateral_bank: f32,
    pub head_yaw_rel_deg: f32,
    pub head_pitch_deg: f32,
    pub walk_pos: f32,
    pub walk_speed: f32,
    pub age_ticks: f32,
    pub arm_shoot_anim_enabled: bool,
    pub dt_sec: f32,
}

/// Smoothly blends between Spider-Hero traversal, trick, crawl, glide, and landing animations.
#[derive(Clone, Debug, Default)]
pub struct AnimationController {
    current: SpiderRigPose,
    right_shot_flash: f32,
    left_shot_flash: f32,
}

impl AnimationController {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn trigger_wrist_shot(&mut self, wrist: WristSide, dual: bool) {
        if dual {
            self.right_shot_flash = 1.0;
            self.left_shot_flash = 1.0;
        } else {
            match wrist {
                WristSide::Right => self.right_shot_flash = 1.0,
                WristSide::Left => self.left_shot_flash = 1.0,
            }
        }
    }

    pub fn current_pose(&self) -> SpiderRigPose {
        self.current
    }

    pub fn evaluate(&mut self, inp: PoseEvalInput) -> SpiderRigPose {
        let dt = inp.dt_sec.clamp(0.0005, 0.05);
        let blend = (1.0 - (-15.0 * dt).exp()).clamp(0.05, 1.0);

        self.right_shot_flash = (self.right_shot_flash - dt * 3.4).max(0.0);
        self.left_shot_flash = (self.left_shot_flash - dt * 3.4).max(0.0);

        let mut target = SpiderRigPose::default();
        target.head_rot = [
            inp.head_pitch_deg.clamp(-65.0, 65.0),
            inp.head_yaw_rel_deg.clamp(-70.0, 70.0),
            0.0,
        ];

        // Baseline vanilla walk swing so grounded movement looks completely natural in SkinSuit mode
        let leg_swing = (inp.walk_pos * 0.6662).cos() * 48.0 * inp.walk_speed.clamp(0.0, 1.1);
        let arm_swing = -leg_swing * 0.78;
        let idle_breathe = (inp.age_ticks * 0.09).sin() * 2.2;
        target.right_arm_rot = [arm_swing, 0.0, 3.0 + idle_breathe];
        target.left_arm_rot = [-arm_swing, 0.0, -3.0 - idle_breathe];
        target.right_leg_rot = [leg_swing, 0.0, 0.0];
        target.left_leg_rot = [-leg_swing, 0.0, 0.0];

        // 1. Superhero 3-Point Landing Pose (highest ground priority!)
        if inp.superhero_landing_ticks > 0 && inp.on_ground {
            target.custom_pose_active = true;
            target.root_offset_y = -0.34;
            target.body_rot = [36.0, 12.0, 0.0];
            target.head_rot = [-22.0, -10.0, 0.0];
            // Right fist planted to pavement, left arm swept dramatically behind!
            target.right_arm_rot = [-58.0, -8.0, 10.0];
            target.left_arm_rot = [48.0, 14.0, -24.0];
            target.right_leg_rot = [-68.0, -10.0, 0.0];
            target.left_leg_rot = [44.0, 8.0, 0.0];
        }
        // 2. Active Web Swinging Pose
        else if inp.swing_phase.is_active() {
            target.custom_pose_active = true;
            let wrist = inp.swing_phase.active_wrist().unwrap_or(WristSide::Right);
            let arc_vert = (inp.vertical_vel as f32).clamp(-1.2, 1.2);
            // Lean forward into pendulum arc + bank into turn
            let forward_lean = (24.0 - arc_vert * 22.0).clamp(4.0, 52.0);
            let roll_bank = (inp.lateral_bank * 26.0).clamp(-28.0, 28.0);
            target.body_rot = [forward_lean, 0.0, roll_bank];

            // Tuck knees tightly through bottom of arc, extend gracefully on upswing!
            let knee_tuck = (1.0 - arc_vert.abs()).clamp(0.15, 1.0);
            target.right_leg_rot = [-46.0 * knee_tuck - 10.0, -5.0, 4.0];
            target.left_leg_rot = [-26.0 * knee_tuck + 14.0, 5.0, -4.0];

            match wrist {
                WristSide::Right => {
                    target.right_arm_rot = [-154.0, -8.0, 8.0];
                    target.left_arm_rot = [36.0, 12.0, -22.0];
                }
                WristSide::Left => {
                    target.left_arm_rot = [-154.0, 8.0, -8.0];
                    target.right_arm_rot = [36.0, -12.0, 22.0];
                }
            }
        }
        // 3. Non-swinging Spider-Traversal poses
        else {
            match inp.traversal {
                TraversalState::PointLaunching { .. } | TraversalState::WebZipping { .. } => {
                    target.custom_pose_active = true;
                    target.body_rot = [42.0, 0.0, 0.0];
                    // Dual or single forward web pull!
                    target.right_arm_rot = [-128.0, -6.0, 6.0];
                    target.left_arm_rot = [-128.0, 6.0, -6.0];
                    target.right_leg_rot = [-42.0, 0.0, 3.0];
                    target.left_leg_rot = [18.0, 0.0, -3.0];
                }
                TraversalState::SlingshotCharging { charge, .. } => {
                    target.custom_pose_active = true;
                    target.root_offset_y = -0.22 * charge;
                    target.body_rot = [26.0 + 16.0 * charge, 0.0, 0.0];
                    target.right_arm_rot = [-102.0, -18.0, 12.0];
                    target.left_arm_rot = [-102.0, 18.0, -12.0];
                    target.right_leg_rot = [-48.0 * charge, -8.0, 0.0];
                    target.left_leg_rot = [34.0 * charge, 8.0, 0.0];
                }
                TraversalState::SlingshotLaunching { .. } => {
                    target.custom_pose_active = true;
                    target.body_rot = [64.0, 0.0, 0.0];
                    target.right_arm_rot = [54.0, 0.0, 18.0];
                    target.left_arm_rot = [54.0, 0.0, -18.0];
                    target.right_leg_rot = [14.0, 0.0, 4.0];
                    target.left_leg_rot = [14.0, 0.0, -4.0];
                }
                TraversalState::WebWingsGliding { bank_deg, .. } => {
                    target.custom_pose_active = true;
                    target.web_wings_deploy = 1.0;
                    let pitch_lean = (76.0 + inp.head_pitch_deg * 0.25).clamp(52.0, 96.0);
                    let flutter = (inp.age_ticks * 0.85).sin() * 2.4;
                    target.body_rot = [pitch_lean, 0.0, bank_deg];
                    target.head_rot = [-48.0, inp.head_yaw_rel_deg * 0.4, 0.0];
                    // Arms swept out horizontally into aerodynamic Web-Wings airfoil!
                    target.right_arm_rot = [-18.0, 0.0, 78.0 + flutter];
                    target.left_arm_rot = [-18.0, 0.0, -78.0 - flutter];
                    target.right_leg_rot = [8.0, 0.0, 7.0];
                    target.left_leg_rot = [8.0, 0.0, -7.0];
                }
                TraversalState::WallRunning {
                    ticks_on_wall,
                    vertical_climb,
                    ..
                } => {
                    target.custom_pose_active = true;
                    let run_cycle = (ticks_on_wall as f32 * 0.72).sin();
                    if vertical_climb {
                        target.body_rot = [-58.0, 0.0, 0.0];
                    } else {
                        target.body_rot = [24.0, 0.0, inp.lateral_bank * 28.0];
                    }
                    target.right_arm_rot = [-68.0 * run_cycle, 0.0, 14.0];
                    target.left_arm_rot = [68.0 * run_cycle, 0.0, -14.0];
                    target.right_leg_rot = [62.0 * run_cycle, 0.0, 0.0];
                    target.left_leg_rot = [-62.0 * run_cycle, 0.0, 0.0];
                }
                TraversalState::SurfaceCrawling { ceiling, .. } => {
                    target.custom_pose_active = true;
                    let crawl = (inp.age_ticks * 0.42).sin()
                        * (inp.speed_blocks_per_tick as f32 * 3.8).clamp(0.0, 1.0);
                    if ceiling {
                        // Inverted upside-down ceiling spider-crawl!
                        target.root_offset_y = 0.14;
                        target.body_rot = [0.0, 0.0, 180.0];
                        target.right_arm_rot = [-115.0 + crawl * 32.0, -24.0, 34.0];
                        target.left_arm_rot = [-115.0 - crawl * 32.0, 24.0, -34.0];
                        target.right_leg_rot = [-42.0 - crawl * 28.0, -20.0, 26.0];
                        target.left_leg_rot = [-42.0 + crawl * 28.0, 20.0, -26.0];
                    } else {
                        // Vertical wall spider-crawl!
                        target.body_rot = [-82.0, 0.0, 0.0];
                        target.head_rot = [48.0, inp.head_yaw_rel_deg * 0.5, 0.0];
                        target.right_arm_rot = [-124.0 + crawl * 36.0, -22.0, 28.0];
                        target.left_arm_rot = [-124.0 - crawl * 36.0, 22.0, -28.0];
                        target.right_leg_rot = [-38.0 - crawl * 32.0, -18.0, 22.0];
                        target.left_leg_rot = [-38.0 + crawl * 32.0, 18.0, -22.0];
                    }
                }
                TraversalState::Normal | TraversalState::PointPerchedWindow { .. } => {
                    if let Some((trick, phase)) = inp.active_trick {
                        target.custom_pose_active = true;
                        let deg = phase * 360.0;
                        match trick {
                            AirTrickKind::FrontFlip => {
                                target.body_rot = [deg, 0.0, 0.0];
                                target.right_arm_rot = [-55.0, 0.0, 18.0];
                                target.left_arm_rot = [-55.0, 0.0, -18.0];
                                target.right_leg_rot = [-68.0, 0.0, 0.0];
                                target.left_leg_rot = [-68.0, 0.0, 0.0];
                            }
                            AirTrickKind::Backflip => {
                                target.body_rot = [-deg, 0.0, 0.0];
                                target.right_arm_rot = [-135.0, 0.0, 26.0];
                                target.left_arm_rot = [-135.0, 0.0, -26.0];
                                target.right_leg_rot = [24.0, 0.0, 8.0];
                                target.left_leg_rot = [-38.0, 0.0, -8.0];
                            }
                            AirTrickKind::CorkscrewSpin => {
                                target.body_rot = [58.0, 0.0, deg];
                                target.right_arm_rot = [-165.0, 0.0, 6.0];
                                target.left_arm_rot = [24.0, 0.0, -12.0];
                                target.right_leg_rot = [12.0, 0.0, 4.0];
                                target.left_leg_rot = [12.0, 0.0, -4.0];
                            }
                            AirTrickKind::WindmillTwist => {
                                target.body_rot = [32.0, deg, 24.0];
                                target.right_arm_rot = [-90.0, -25.0, 52.0];
                                target.left_arm_rot = [-90.0, 25.0, -52.0];
                                target.right_leg_rot = [-54.0, -18.0, 16.0];
                                target.left_leg_rot = [48.0, 18.0, -16.0];
                            }
                        }
                    } else if !inp.on_ground && inp.vertical_vel < -0.18 {
                        // Stylish Freefall / Head-first Skydiving pose!
                        target.custom_pose_active = true;
                        let dive_factor = ((-inp.vertical_vel as f32 - 0.18) / 0.85).clamp(0.0, 1.0);
                        let look_dive = ((inp.head_pitch_deg - 15.0) / 55.0).clamp(0.0, 1.0);
                        let streamline = (dive_factor * 0.65 + look_dive * 0.55).clamp(0.0, 1.0);
                        target.body_rot = [
                            18.0 + streamline * 56.0,
                            0.0,
                            inp.lateral_bank * 18.0,
                        ];
                        target.right_arm_rot = [
                            -32.0 + streamline * 78.0,
                            0.0,
                            46.0 - streamline * 28.0,
                        ];
                        target.left_arm_rot = [
                            -32.0 + streamline * 78.0,
                            0.0,
                            -46.0 + streamline * 28.0,
                        ];
                        target.right_leg_rot = [-28.0 + streamline * 38.0, 0.0, 8.0];
                        target.left_leg_rot = [14.0, 0.0, -8.0];
                    }
                }
            }
        }

        // Overlay crisp wrist-extension shot impulse if `arm_shoot_anim_enabled`
        if inp.arm_shoot_anim_enabled {
            if self.right_shot_flash > 0.05 {
                let w = self.right_shot_flash;
                target.right_arm_rot = lerp_rot(target.right_arm_rot, [-118.0, -6.0, 6.0], w);
                target.custom_pose_active = true;
            }
            if self.left_shot_flash > 0.05 {
                let w = self.left_shot_flash;
                target.left_arm_rot = lerp_rot(target.left_arm_rot, [-118.0, 6.0, -6.0], w);
                target.custom_pose_active = true;
            }
        }

        target.fp_right_shot = self.right_shot_flash;
        target.fp_left_shot = self.left_shot_flash;

        // Avoid wrap pop during continuous 360° air tricks by snapping directly to trick angle while tricking
        let body_blend = if inp.active_trick.is_some() {
            1.0
        } else {
            blend
        };

        self.current = SpiderRigPose {
            root_offset_y: lerp_f32(self.current.root_offset_y, target.root_offset_y, blend),
            body_rot: lerp_rot(self.current.body_rot, target.body_rot, body_blend),
            head_rot: lerp_rot(self.current.head_rot, target.head_rot, blend),
            right_arm_rot: lerp_rot(self.current.right_arm_rot, target.right_arm_rot, blend),
            left_arm_rot: lerp_rot(self.current.left_arm_rot, target.left_arm_rot, blend),
            right_leg_rot: lerp_rot(self.current.right_leg_rot, target.right_leg_rot, blend),
            left_leg_rot: lerp_rot(self.current.left_leg_rot, target.left_leg_rot, blend),
            web_wings_deploy: lerp_f32(
                self.current.web_wings_deploy,
                target.web_wings_deploy,
                blend,
            ),
            custom_pose_active: target.custom_pose_active,
            fp_right_shot: target.fp_right_shot,
            fp_left_shot: target.fp_left_shot,
        };
        self.current
    }
}
