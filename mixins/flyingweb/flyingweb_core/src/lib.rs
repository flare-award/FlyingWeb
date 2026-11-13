pub mod animations;
pub mod camera;
pub mod config;
pub mod effects;
pub mod gadgets;
pub mod input;
pub mod movement;
pub mod physics;
pub mod raycast;
pub mod suit;
pub mod swinging;
pub mod ui;
pub mod web;

use glam::DVec3;

use crate::{
    animations::{AnimationController, PoseEvalInput},
    camera::{CameraFrameInput, CameraPoseOutput, SpiderCamera},
    config::FlyingWebConfig,
    effects::{EffectsSynthesizer, ParticleCue, SoundCue},
    gadgets::{
        GadgetEvent, GadgetManager, GadgetNetRequest, MobImpulseCommand, TargetEntitySnapshot,
    },
    input::{InputRouter, SpiderAction},
    movement::{TraversalController, TraversalEvent, TraversalState},
    raycast::{WorldQuery, look_direction},
    suit::SuitAtlasManager,
    swinging::{SwingController, SwingEvent},
    ui::FlyingWebUiState,
    web::WebStrandManager,
};

/// Snapshot of the local player's physical state at the moment a 20 Hz client tick executes.
#[derive(Clone, Copy, Debug)]
pub struct LocalPlayerSnapshot {
    pub position: DVec3,
    pub previous_position: DVec3,
    pub vanilla_velocity: DVec3,
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub on_ground: bool,
    pub horizontal_collision: bool,
    pub vertical_collision: bool,
    pub in_fluid: bool,
    pub creative_flying: bool,
    pub spectator: bool,
}

/// Output produced by one 20 Hz client simulation tick for `glue.rs` to apply to Horizon's `Game`.
#[derive(Clone, Debug, Default)]
pub struct ClientTickResult {
    /// Additive velocity kick `[vx, vy, vz]` to pass to `game.native_push_player(kick)`
    /// when Spider-Man physics is actively driving motion.
    pub velocity_kick: Option<[f32; 3]>,
    /// True when `player.fall_distance` should be zeroed (swinging, gliding, crawling, superhero landing).
    pub reset_fall_distance: bool,
    /// True when camera should switch into `ThirdPersonBack` automatically.
    pub request_third_person: bool,
    /// True when traversal ended and camera can return to first-person if `auto_third_person && !keep_third_person_on_ground`.
    pub release_auto_third_person: bool,
    /// True when the player pressed the Open Settings keybind this tick.
    pub open_settings_screen: bool,
    /// Optional gadget network packet to send to the server (`native_net_send_to_server`).
    pub outbound_gadget_packet: Option<GadgetNetRequest>,
    /// Optional status notification for the client action bar.
    pub action_bar_notice: Option<String>,
    /// Original layered local sound cues to play this tick.
    pub sounds: Vec<SoundCue>,
    /// Particle cues to spawn this tick.
    pub particles: Vec<ParticleCue>,
}

/// Complete client-side FlyingWeb runtime session state, cleanly reset on world unload,
/// death, respawn, or dimension change.
#[derive(Clone, Debug)]
pub struct FlyingWebClientSession {
    pub config: FlyingWebConfig,
    pub input: InputRouter,
    pub swing: SwingController,
    pub traversal: TraversalController,
    pub camera: SpiderCamera,
    pub strands: WebStrandManager,
    pub gadgets: GadgetManager,
    pub animations: AnimationController,
    pub effects: EffectsSynthesizer,
    pub suit_atlas: SuitAtlasManager,
    pub ui: FlyingWebUiState,

    /// Authoritative pre-vanilla-drag Spider-Velocity tracked across 20 Hz ticks.
    sim_velocity: DVec3,
    prev_tick_pos: Option<DVec3>,
    auto_switched_camera: bool,
    pub last_frame_dt_sec: f32,
    pub latest_cam_output: Option<CameraPoseOutput>,
}

impl Default for FlyingWebClientSession {
    fn default() -> Self {
        Self {
            config: FlyingWebConfig::default(),
            input: InputRouter::default(),
            swing: SwingController::default(),
            traversal: TraversalController::default(),
            camera: SpiderCamera::default(),
            strands: WebStrandManager::default(),
            gadgets: GadgetManager::default(),
            animations: AnimationController::default(),
            effects: EffectsSynthesizer::default(),
            suit_atlas: SuitAtlasManager::default(),
            ui: FlyingWebUiState::default(),
            sim_velocity: DVec3::ZERO,
            prev_tick_pos: None,
            auto_switched_camera: false,
            last_frame_dt_sec: 1.0 / 60.0,
            latest_cam_output: None,
        }
    }
}

impl FlyingWebClientSession {
    /// Cleanly wipes all transient physics, active webs, gadgets, inputs, and camera springs
    /// on world exit, death, respawn, or dimension change while preserving user config & suit atlas!
    pub fn reset_transient_state(&mut self) {
        self.input.reset_all();
        self.swing.reset();
        self.traversal.reset();
        self.camera.reset();
        self.strands.clear();
        self.gadgets.reset_world_state();
        self.animations.reset();
        self.effects.reset();
        self.sim_velocity = DVec3::ZERO;
        self.prev_tick_pos = None;
        self.auto_switched_camera = false;
        self.latest_cam_output = None;
    }

    pub fn authoritative_velocity(&self) -> DVec3 {
        self.sim_velocity
    }

    /// Steps the entire client-side Spider-Hero simulation by one 20 Hz game tick.
    pub fn step_client_tick(
        &mut self,
        world: &dyn WorldQuery,
        player: LocalPlayerSnapshot,
        nearby_targets: &[TargetEntitySnapshot],
    ) -> ClientTickResult {
        let mut out = ClientTickResult::default();
        self.ui.tick();

        let mut tick_in = self.input.consume_tick();
        if tick_in.open_settings_just_pressed {
            out.open_settings_screen = true;
        }

        // Scroll-wheel while holding `CycleGadget` (or when not swinging) cycles gadgets!
        if tick_in.gadget_cycle_just_pressed {
            self.gadgets.cycle_selected(1, &self.config);
            out.action_bar_notice = Some(format!(
                "Spider-Gadget: {}",
                self.gadgets.selected.label()
            ));
        } else if tick_in.wheel_delta.abs() >= 0.9 && self.input.is_held(SpiderAction::CycleGadget) {
            let dir = if tick_in.wheel_delta > 0.0 { -1 } else { 1 };
            self.gadgets.cycle_selected(dir, &self.config);
            tick_in.wheel_delta = 0.0;
            out.action_bar_notice = Some(format!(
                "Spider-Gadget: {}",
                self.gadgets.selected.label()
            ));
        }

        // Yield cleanly to Creative Flight, Spectator mode, or Swimming in deep water/lava!
        if player.creative_flying || player.spectator || !self.config.abilities_active() {
            if self.swing.phase.is_active() {
                self.swing.force_cancel(&mut self.strands);
            }
            self.strands.tick();
            self.sim_velocity = player.vanilla_velocity;
            self.prev_tick_pos = Some(player.position);
            return out;
        }

        // Reconcile `sim_velocity` with real world displacement `(player.position - prev_pos)`
        // so colliding against a solid building wall or floor kills blocked velocity cleanly!
        let actual_disp = player.position - player.previous_position;
        let spider_driving_prev = self.swing.phase.is_attached()
            || !matches!(self.traversal.state, TraversalState::Normal);

        let mut working_vel = if spider_driving_prev && self.prev_tick_pos.is_some() {
            let mut v = self.sim_velocity;
            if player.horizontal_collision {
                v.x = actual_disp.x.clamp(-0.35, 0.35);
                v.z = actual_disp.z.clamp(-0.35, 0.35);
            }
            if player.vertical_collision {
                v.y = actual_disp.y.clamp(-0.25, 0.25);
            }
            v
        } else {
            // Inherit real displacement velocity (un-damped by end-of-tick 0.91 air drag!)
            actual_disp
        };

        if player.in_fluid {
            working_vel *= 0.78;
        }

        // 1. Step Web Swinging state machine & pendulum physics
        let mut swing_events = Vec::new();
        working_vel = self.swing.tick(
            world,
            player.position,
            working_vel,
            player.yaw_deg,
            player.pitch_deg,
            player.on_ground,
            &tick_in,
            &self.config,
            &mut self.strands,
            &mut swing_events,
        );

        // 2. Step Additional Traversal Mechanics (Web Zip, Point Launch, Slingshot, Web Wings, Tricks, Wall Run/Crawl)
        let mut trav_events = Vec::new();
        working_vel = self.traversal.tick(
            world,
            player.position,
            working_vel,
            player.yaw_deg,
            player.pitch_deg,
            player.on_ground,
            self.swing.phase.is_active(),
            &tick_in,
            &self.config,
            &mut self.strands,
            &mut trav_events,
        );

        // 3. Check Traversable Tightrope (`WebLine` gadget) support when walking/descending onto a line!
        let mut on_tightrope = false;
        if !self.swing.phase.is_active()
            && matches!(self.traversal.state, TraversalState::Normal)
            && let Some((_, rope_vel)) = self.gadgets.evaluate_tightrope_support(
                player.position,
                working_vel,
                tick_in.sneak_held,
            )
        {
            working_vel = rope_vel;
            on_tightrope = true;
        }

        // 4. Fire / Trigger Spider-Gadget (`B`)
        let mut gadget_events = Vec::new();
        if tick_in.gadget_use_just_pressed {
            let wrist = self.swing.next_wrist;
            self.swing.next_wrist = wrist.flip();
            let shot_orig = SwingController::compute_wrist_origin(
                player.position,
                player.yaw_deg,
                player.pitch_deg,
                wrist,
                false,
            );
            let aim = look_direction(player.yaw_deg, player.pitch_deg);
            if let Some(pkt) = self.gadgets.try_use_gadget(
                world,
                self.gadgets.selected,
                shot_orig,
                aim,
                tick_in.sneak_held,
                &self.config,
                &mut gadget_events,
            ) {
                self.animations.trigger_wrist_shot(wrist, false);
                out.outbound_gadget_packet = Some(pkt);
            }
        }

        // Step client-side gadget visuals & tightrope timers
<<<<<<< HEAD
        let mut client_unused_impulses = Vec::new();
        self.gadgets.tick(
            world,
            nearby_targets,
            &mut client_unused_impulses,
=======
        self.gadgets.tick(
            world,
            nearby_targets,
            &mut Vec::new(),
>>>>>>> e7c9626 (Fix Clippy warning: remove unused variable  in)
            &mut gadget_events,
        );

        // Step active & fading silk strands
        self.strands.tick();

        // 5. Translate events into animations, camera impulses, sounds, and particles
        for ev in swing_events {
            if let SwingEvent::ShotLaunched { wrist, .. } = ev {
                self.animations.trigger_wrist_shot(wrist, false);
            }
            if let SwingEvent::Released {
                vault_jump: true, ..
            } = ev
            {
                self.camera.trigger_launch_impulse(working_vel, 0.75);
            }
            self.effects.emit_swing_event(
                ev,
                player.position,
                &self.config,
                &mut out.sounds,
                &mut out.particles,
            );
        }

        for ev in trav_events {
            let mut step_hint = "";
            match ev {
                TraversalEvent::WebZipFired { dir } => {
                    self.animations
                        .trigger_wrist_shot(crate::input::WristSide::Right, false);
                    self.camera.trigger_launch_impulse(dir, 0.92);
                }
                TraversalEvent::PointLaunchStarted { .. } | TraversalEvent::SlingshotStarted => {
                    self.animations
                        .trigger_wrist_shot(crate::input::WristSide::Right, true);
                }
                TraversalEvent::PointLaunchVaulted { dir } => {
                    self.camera.trigger_launch_impulse(dir, 1.05);
                }
                TraversalEvent::SlingshotReleased { dir, charge } => {
                    self.camera
                        .trigger_launch_impulse(dir, 0.7 + (charge as f64) * 0.65);
                }
                TraversalEvent::WallRunStep { block, .. } => {
                    step_hint = world.sample_voxel(block).step_sound;
                }
                TraversalEvent::AirTrickCompleted { kind, combo } => {
                    out.action_bar_notice = Some(format!("Air Trick: {} (x{combo})", kind.label()));
                }
                _ => {}
            }
            self.effects.emit_traversal_event(
                ev,
                player.position,
                step_hint,
                &self.config,
                &mut out.sounds,
                &mut out.particles,
            );
        }

        for ev in gadget_events {
            self.effects.emit_gadget_event(
                ev,
                &self.config,
                &mut out.sounds,
                &mut out.particles,
            );
        }

        let is_gliding = matches!(self.traversal.state, TraversalState::WebWingsGliding { .. });
        self.effects.emit_ambient_motion_fx(
            player.position,
            working_vel,
            is_gliding,
            &self.config,
            &mut out.sounds,
            &mut out.particles,
        );

        // 6. Decide whether Spider-Physics is actively driving velocity this tick
        let spider_driving_now = self.swing.phase.is_attached()
            || !matches!(self.traversal.state, TraversalState::Normal)
            || on_tightrope;

        // Also preserve horizontal momentum smoothly in freefall right after releasing a swing
        // instead of letting vanilla's harsh `0.91` per-tick drag kill swing distance!
        let airborne_momentum_carry = !player.on_ground
            && !spider_driving_now
            && working_vel.length() > 0.36
            && self.prev_tick_pos.is_some();

        if spider_driving_now {
            let delta = working_vel - player.vanilla_velocity;
            if delta.is_finite() {
                out.velocity_kick = Some([delta.x as f32, delta.y as f32, delta.z as f32]);
            }
            self.sim_velocity = working_vel;
        } else if airborne_momentum_carry {
            // Apply realistic ballistic gravity + light Spider-Man aerodynamic drag (`0.986` vs vanilla `0.91`!)
            let carried = DVec3::new(
                working_vel.x * 0.986,
                (working_vel.y - 0.076).max(-2.8),
                working_vel.z * 0.986,
            );
            let delta = carried - player.vanilla_velocity;
            if delta.is_finite() {
                out.velocity_kick = Some([delta.x as f32, delta.y as f32, delta.z as f32]);
            }
            self.sim_velocity = carried;
        } else {
            self.sim_velocity = player.vanilla_velocity;
        }

        out.reset_fall_distance = self
            .traversal
            .should_negate_fall_damage(self.swing.phase.is_active())
            || on_tightrope;

        // 7. Auto third-person camera transitions
        let (cam_mode, _) = self
            .traversal
            .camera_mode(self.swing.phase.is_attached(), player.on_ground || on_tightrope);
        if self.config.camera_enabled && self.config.auto_third_person {
            if cam_mode.is_spider_traversal() && !player.on_ground {
                out.request_third_person = true;
                self.auto_switched_camera = true;
            } else if player.on_ground
                && !cam_mode.is_spider_traversal()
                && self.auto_switched_camera
                && !self.config.keep_third_person_on_ground
                && self.traversal.superhero_landing_ticks == 0
            {
                out.release_auto_third_person = true;
                self.auto_switched_camera = false;
            }
        }

        self.prev_tick_pos = Some(player.position);
        out
    }

    /// Evaluates the render-frame dynamic Spider-Cam pose at `partial_tick` (`0.0..=1.0`).
    pub fn evaluate_camera_frame(
        &mut self,
        world: &dyn WorldQuery,
        interp_feet: DVec3,
        player_yaw: f32,
        player_pitch: f32,
        on_ground: bool,
    ) -> Option<CameraPoseOutput> {
        if !self.config.mod_enabled || !self.config.camera_enabled {
            self.latest_cam_output = None;
            return None;
        }
        let (mode, surf_norm) = self
            .traversal
            .camera_mode(self.swing.phase.is_attached(), on_ground);
        let pose = self.camera.evaluate_frame(
            world,
            CameraFrameInput {
                interp_feet,
                velocity: self.sim_velocity,
                player_yaw,
                player_pitch,
                on_ground,
                mode,
                surface_normal: surf_norm,
                swing_bank: self.swing.lateral_bank,
                dt_sec: self.last_frame_dt_sec,
            },
            &self.config,
        );
        self.latest_cam_output = Some(pose);
        Some(pose)
    }

    /// Evaluates the local player's 3D Spider-Hero limb pose for `native_player_submit` / `native_mob_layer`.
    pub fn evaluate_player_pose(
        &mut self,
        head_yaw_rel_deg: f32,
        head_pitch_deg: f32,
        walk_pos: f32,
        walk_speed: f32,
        age_ticks: f32,
        on_ground: bool,
    ) -> crate::animations::SpiderRigPose {
        self.animations.evaluate(PoseEvalInput {
            swing_phase: self.swing.phase,
            traversal: self.traversal.state,
            active_trick: self.traversal.active_trick,
            superhero_landing_ticks: self.traversal.superhero_landing_ticks,
            on_ground,
            speed_blocks_per_tick: self.sim_velocity.length(),
            vertical_vel: self.sim_velocity.y,
            lateral_bank: self.swing.lateral_bank,
            head_yaw_rel_deg,
            head_pitch_deg,
            walk_pos,
            walk_speed,
            age_ticks,
            arm_shoot_anim_enabled: self.config.arm_shoot_animation,
            dt_sec: self.last_frame_dt_sec,
        })
    }
}

/// Authoritative server-side gadget & non-lethal fall-arrest manager for integrated
/// singleplayer and FlyingWeb-enabled dedicated/LAN servers.
#[derive(Clone, Debug, Default)]
pub struct FlyingWebServerRuntime {
    pub gadgets: GadgetManager,
    pub non_lethal_protection: bool,
    sync_counter: u8,
}

impl FlyingWebServerRuntime {
    pub fn new() -> Self {
        Self {
            gadgets: GadgetManager::default(),
            non_lethal_protection: true,
            sync_counter: 0,
        }
    }

    pub fn reset(&mut self) {
        self.gadgets.reset_world_state();
        self.sync_counter = 0;
    }

    pub fn handle_client_packet(
        &mut self,
        world: &dyn WorldQuery,
        data: &[u8],
        targets: &[TargetEntitySnapshot],
        impulses: &mut Vec<MobImpulseCommand>,
    ) {
        let Ok(req) = serde_json::from_slice::<GadgetNetRequest>(data) else {
            return;
        };
        let mut ignored_events = Vec::new();
        self.gadgets
            .execute_authoritative_request(world, req, targets, impulses, &mut ignored_events);
        self.sync_counter = 10; // Trigger immediate sync broadcast on next server tick
    }

    /// Steps server-side gadgets & returns `Some(serialized_broadcast)` when webbed entity state updates.
    pub fn step_server_tick(
        &mut self,
        world: &dyn WorldQuery,
        targets: &[TargetEntitySnapshot],
        impulses: &mut Vec<MobImpulseCommand>,
    ) -> Option<Vec<u8>> {
        let mut ignored_events = Vec::new();
        self.gadgets
            .tick(world, targets, impulses, &mut ignored_events);
        self.sync_counter = self.sync_counter.saturating_add(1);
        if self.sync_counter >= 5 {
            self.sync_counter = 0;
            let entries = self.gadgets.export_sync_entries();
            let msg = crate::gadgets::GadgetNetBroadcast::WebbedEntities(entries);
            return serde_json::to_vec(&msg).ok();
        }
        None
    }
}

#[cfg(test)]
mod tests;
