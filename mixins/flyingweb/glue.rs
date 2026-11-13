// Bridge between Horizon 26.3's `Game<'_>` hooks/services and `flyingweb_core`.
// Strictly uses plain `//` comments and zero inner attributes as required by `horizon-mixins`.

use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use flyingweb_core::{
    FlyingWebClientSession, FlyingWebServerRuntime, LocalPlayerSnapshot,
    config::{FlyingWebConfig, SuitRenderMode},
    effects::ParticleFlavor,
    gadgets::{GADGET_NET_CHANNEL, GadgetNetBroadcast, MobImpulseCommand, TargetEntitySnapshot},
    input::{SpiderAction, WristSide},
    raycast::{VoxelSample, WorldQuery},
    suit::{ATLAS_HEIGHT, ATLAS_WIDTH, SuitQuad, build_mob_web_cocoon_quads, build_spider_suit_quads},
    swinging::SwingController,
    ui::{UiClickOutcome, UiDrawCmd},
    web::RenderSegment3D,
};
use glam::{DVec3, IVec3};
use horizon_client::{
    blocks::Fluid,
    extras_hud::Corners,
    first_person::Hand,
    hud::{HudBatch, MOD_IMAGE_SPRITE},
    items::ItemStack,
    mod_api::{NativeMod, NativeScreenView, ServerTickPhase, WorldPhase},
    particles::{ParticleOptions, ParticleType},
    text::Text,
    world_primitives::WorldPrimitives,
};

use super::{
    Game,
    native_block_entity_draw::BlockEntityDraw,
    native_hooks::NativeShake,
    native_mob_draw::{NativeMobRender, NativePlayerRender},
    perspective::CameraType,
};

pub const MOD_ID: &str = "flyingweb";
pub const MOD_NAME: &str = "FlyingWeb";
pub const MOD_VERSION: &str = "1.0.0";
pub const MOD_DESCRIPTION: &str = "Insomniac-inspired Spider-Hero pendulum web swinging, dynamic third-person Spider-Cam, Web Wings, wall/ceiling crawling, 8 non-lethal Spider-Gadgets, and skin-layer suits.";

const CONFIG_FILENAME: &str = "flyingweb.json";

thread_local! {
    static CLIENT: RefCell<ClientGlueState> = RefCell::new(ClientGlueState::new());
}

static SERVER: Mutex<Option<FlyingWebServerRuntime>> = Mutex::new(None);

struct ClientGlueState {
    session: FlyingWebClientSession,
    config_loaded: bool,
    saved_camera_before_auto: Option<CameraType>,
}

impl ClientGlueState {
    fn new() -> Self {
        Self {
            session: FlyingWebClientSession::default(),
            config_loaded: false,
            saved_camera_before_auto: None,
        }
    }
}

fn config_file_path() -> Option<PathBuf> {
    Game::native_config_dir()
        .ok()
        .map(|d| d.join(CONFIG_FILENAME))
}

fn ensure_client_initialized(state: &mut ClientGlueState, game: &Game<'_>) {
    if state.config_loaded {
        return;
    }
    state.config_loaded = true;

    if let Some(cfg_path) = config_file_path() {
        if let Ok(raw) = fs::read_to_string(&cfg_path) {
            state.session.config = FlyingWebConfig::from_json_lossless(&raw);
        } else {
            let _ = fs::write(&cfg_path, state.session.config.to_json_pretty());
        }
    }

    // Sync any user overrides stored in Horizon's `options.txt` mod keybinds
    for action in SpiderAction::ALL {
        let default_key = action.binding(&state.session.config.keys).to_owned();
        let chosen = game.native_mod_key(MOD_ID, action.id_slug(), &default_key);
        action.set_binding(&mut state.session.config.keys, chosen);
    }

    let data_dir = game.native_data_dir(MOD_ID);
    flyingweb_core::suit::SuitAtlasManager::ensure_disk_templates(&data_dir);
    state
        .session
        .suit_atlas
        .reload_from_config(&state.session.config, Some(&data_dir));
}

fn persist_client_config(state: &ClientGlueState, game: &mut Game<'_>) {
    if let Some(cfg_path) = config_file_path() {
        let _ = fs::write(&cfg_path, state.session.config.to_json_pretty());
    }
    for action in SpiderAction::ALL {
        let key = action.binding(&state.session.config.keys);
        let _ = game.native_set_mod_key(MOD_ID, action.id_slug(), Some(key));
    }
}

fn ensure_atlas_uploaded(state: &mut ClientGlueState, game: &mut Game<'_>) {
    let data_dir = game.native_data_dir(MOD_ID);
    state
        .session
        .suit_atlas
        .sync_with_config(&state.session.config, Some(&data_dir));
    if state.session.suit_atlas.dirty {
        if game.native_mod_image(ATLAS_WIDTH, ATLAS_HEIGHT, &state.session.suit_atlas.pixels) {
            state.session.suit_atlas.dirty = false;
        }
    }
}

/// Zero-allocation `WorldQuery` adapter backed directly by Horizon's loaded `GameLevel` and `BlockFacts`.
struct GameWorldView<'a, 'g> {
    game: &'a Game<'g>,
}

impl WorldQuery for GameWorldView<'_, '_> {
    fn sample_voxel(&self, pos: IVec3) -> VoxelSample<'_> {
        if !self.game.world.loaded(pos.x, pos.z) {
            return VoxelSample::UNLOADED;
        }
        let facts_table = self.game.res.facts();
        let state = self.game.world.block(pos.x, pos.y, pos.z);
        if state == facts_table.air {
            return VoxelSample::AIR;
        }
        let f = facts_table.get(state);
        let is_fluid = f.fluid != Fluid::None;
        let has_shape = !f.outline.is_empty();
        let solid = !is_fluid
            && !f.replaceable
            && (f.full_collision || f.legacy_solid || f.sturdy_full != 0 || (has_shape && f.hardness > 0.0));
        let blocks_cam = !is_fluid && (f.full_collision || f.can_occlude);

        VoxelSample {
            loaded: true,
            solid,
            fluid: is_fluid,
            blocks_camera: blocks_cam,
            boxes: &f.outline,
            step_sound: &f.sounds.step,
        }
    }
}

fn interpolated_local_feet(game: &Game<'_>, partial: f32) -> Option<DVec3> {
    let p = game.player.as_ref()?;
    let prev = DVec3::new(
        game.previous_position.x,
        game.previous_position.y,
        game.previous_position.z,
    );
    let curr = DVec3::new(p.position.x, p.position.y, p.position.z);
    Some(prev.lerp(curr, partial.clamp(0.0, 1.0) as f64))
}

fn collect_client_targets(game: &Game<'_>) -> Vec<TargetEntitySnapshot> {
    game.native_client_entities()
        .into_iter()
        .filter(|e| e.alive && !e.spectator)
        .map(|e| TargetEntitySnapshot {
            id: e.id,
            position: DVec3::from_array(e.position),
            height: (e.height as f64).clamp(0.4, 4.0),
            is_player: e.player_uuid.is_some(),
            can_restrain: e.player_uuid.is_none(),
        })
        .collect()
}

fn collect_server_targets(game: &Game<'_>) -> Vec<TargetEntitySnapshot> {
    game.mobs
        .iter()
        .filter(|m| !m.removed && m.alive())
        .map(|m| TargetEntitySnapshot {
            id: m.id,
            position: DVec3::new(m.position.x, m.position.y, m.position.z),
            height: (m.height() as f64).clamp(0.4, 4.0),
            is_player: false,
            can_restrain: true,
        })
        .collect()
}

fn apply_server_mob_impulses(game: &mut Game<'_>, impulses: &[MobImpulseCommand]) {
    let dim_id = game.dim.id();
    for cmd in impulses {
        let dv = [
            cmd.delta_velocity[0] as f64,
            cmd.delta_velocity[1] as f64,
            cmd.delta_velocity[2] as f64,
        ];
        let _ = game.native_push_mob(dim_id, cmd.entity_id, dv);
        if let Some(m) = game.mobs.iter_mut().find(|m| m.id == cmd.entity_id && !m.removed) {
            m.fall_distance = 0.0;
        }
    }
}

// ============================================================================
// Hook Entrypoints Called From `hook_*.rs`
// ============================================================================

pub fn on_mods_list(mods: &mut Vec<NativeMod>) {
    let default_cfg = FlyingWebConfig::default();
    let keys = SpiderAction::ALL
        .iter()
        .copied()
        .map(|a| {
            [
                a.id_slug().to_owned(),
                a.label().to_owned(),
                a.binding(&default_cfg.keys).to_owned(),
            ]
        })
        .collect();

    mods.push(NativeMod {
        id: MOD_ID.to_owned(),
        title: format!("{MOD_NAME} {MOD_VERSION}"),
        description: MOD_DESCRIPTION.to_owned(),
        keys,
    });
}

pub fn on_settings_buttons(game: &mut Game<'_>, mod_id: &str, buttons: &mut Vec<String>) {
    if mod_id != MOD_ID {
        return;
    }
    CLIENT.with(|cell| {
        let mut st = cell.borrow_mut();
        ensure_client_initialized(&mut st, game);
        let cfg = &st.session.config;
        buttons.push(format!(
            "FlyingWeb Master: {}",
            if cfg.mod_enabled { "ENABLED" } else { "DISABLED" }
        ));
        buttons.push(format!("Suit Render Mode: {}", cfg.suit_mode.label()));
        buttons.push(format!("Suit Variant: {}", cfg.suit_variant.label()));
        buttons.push(format!(
            "Dynamic Spider-Cam: {}",
            if cfg.camera_enabled { "ON" } else { "OFF" }
        ));
        buttons.push(format!(
            "Hold-To-Swing: {}",
            if cfg.hold_to_swing { "ON" } else { "OFF" }
        ));
        buttons.push("Open Full In-World Spider Settings (In World: Key K / /flyingweb)".to_owned());
    });
}

pub fn on_settings_click(game: &mut Game<'_>, mod_id: &str, button: usize) {
    if mod_id != MOD_ID {
        return;
    }
    let mut open_full_ui = false;
    CLIENT.with(|cell| {
        let mut st = cell.borrow_mut();
        ensure_client_initialized(&mut st, game);
        match button {
            0 => st.session.config.mod_enabled = !st.session.config.mod_enabled,
            1 => st.session.config.suit_mode = st.session.config.suit_mode.next(),
            2 => {
                st.session.config.suit_variant = st.session.config.suit_variant.next();
                let dir = game.native_data_dir(MOD_ID);
                st.session
                    .suit_atlas
                    .reload_from_config(&st.session.config, Some(&dir));
            }
            3 => st.session.config.camera_enabled = !st.session.config.camera_enabled,
            4 => st.session.config.hold_to_swing = !st.session.config.hold_to_swing,
            5 => open_full_ui = true,
            _ => {}
        }
        persist_client_config(&st, game);
    });
    if open_full_ui {
        let _ = game.native_mod_screen_open(MOD_ID);
    }
}

pub fn on_commands_register(commands: &mut Vec<&'static str>) {
    commands.push("flyingweb");
    commands.push("spidersuit");
}

pub fn on_command_run(game: &mut Game<'_>, command: &str, ran: &mut bool) {
    if !game.native_on(MOD_ID) {
        return;
    }
    let mut parts = command.split_whitespace();
    let Some(cmd) = parts.next() else {
        return;
    };
    if cmd != "flyingweb" && cmd != "spidersuit" {
        return;
    }
    *ran = true;

    let sub = parts.next().unwrap_or("ui");
    match sub {
        "reload" => {
            CLIENT.with(|cell| {
                let mut st = cell.borrow_mut();
                ensure_client_initialized(&mut st, game);
                let dir = game.native_data_dir(MOD_ID);
                st.session
                    .suit_atlas
                    .reload_from_config(&st.session.config, Some(&dir));
            });
            ensure_client_atlas_now(game);
            game.native_chat("FlyingWeb: Reloaded custom suit skin & configuration.");
        }
        _ if sub == "suit" || cmd == "spidersuit" => {
            let label = CLIENT.with(|cell| {
                let mut st = cell.borrow_mut();
                ensure_client_initialized(&mut st, game);
                st.session.config.suit_variant = st.session.config.suit_variant.next();
                let chosen = st.session.config.suit_variant.label();
                let dir = game.native_data_dir(MOD_ID);
                st.session
                    .suit_atlas
                    .reload_from_config(&st.session.config, Some(&dir));
                persist_client_config(&st, game);
                chosen
            });
            ensure_client_atlas_now(game);
            game.native_chat(&format!("FlyingWeb Suit: {label}"));
        }
        _ => {
            if !game.native_mod_screen_open(MOD_ID) {
                game.native_chat("FlyingWeb: Enter a world and press K (or /flyingweb) to open Spider Settings.");
            }
        }
    }
}

fn ensure_client_atlas_now(game: &mut Game<'_>) {
    CLIENT.with(|cell| {
        let mut st = cell.borrow_mut();
        ensure_atlas_uploaded(&mut st, game);
    });
}

pub fn on_frame(game: &mut Game<'_>, elapsed: Duration) {
    if !game.native_window() || !game.native_on(MOD_ID) {
        return;
    }
    CLIENT.with(|cell| {
        let mut st = cell.borrow_mut();
        ensure_client_initialized(&mut st, game);
        st.session.last_frame_dt_sec = elapsed.as_secs_f32().clamp(0.0005, 0.05);
        ensure_atlas_uploaded(&mut st, game);
    });
}

pub fn on_key(game: &mut Game<'_>, key: &str, pressed: bool) {
    if !game.native_window() || !game.native_on(MOD_ID) {
        return;
    }
    // Ignore gameplay keybinds while typing in chat or container menus unless our own settings screen is open
    if game.native_mod_screen() != Some(MOD_ID) && game.screen_over_playing() {
        return;
    }
    CLIENT.with(|cell| {
        let mut st = cell.borrow_mut();
        ensure_client_initialized(&mut st, game);
        let rebound = st
            .session
            .input
            .on_key_event(key, pressed, &mut st.session.config.keys);
        if rebound {
            persist_client_config(&st, game);
        }
    });
}

pub fn on_wheel(game: &mut Game<'_>, steps: f32, claimed: &mut bool) {
    if !game.native_window() || !game.native_on(MOD_ID) || game.screen_over_playing() {
        return;
    }
    CLIENT.with(|cell| {
        let mut st = cell.borrow_mut();
        let intercept = st.session.swing.phase.is_attached()
            || st.session.input.is_held(SpiderAction::CycleGadget);
        if intercept {
            st.session.input.record_wheel(steps);
            *claimed = true;
        }
    });
}

pub fn on_movement_input(
    game: &Game<'_>,
    forward: &mut f32,
    strafe: &mut f32,
    jump: &mut bool,
    sneak: &mut bool,
) {
    if !game.native_window() || !game.native_on(MOD_ID) {
        return;
    }
    CLIENT.with(|cell| {
        let mut st = cell.borrow_mut();
        st.session
            .input
            .record_movement_axes(*forward, *strafe, *jump, *sneak);

        // Suppress vanilla walk/jump friction when Spider-Man is clinging to a ceiling/wall,
        // charging a Slingshot, or swinging mid-air so vanilla jump doesn't double-fire!
        if st.session.traversal.is_suppressing_vanilla_walk() {
            *forward = 0.0;
            *strafe = 0.0;
            *jump = false;
            if matches!(
                st.session.traversal.state,
                flyingweb_core::movement::TraversalState::SurfaceCrawling { .. }
            ) {
                *sneak = false;
            }
        } else if st.session.swing.phase.is_attached() {
            *jump = false;
        }
    });
}

pub fn on_client_tick(game: &mut Game<'_>) {
    if !game.native_window() || !game.native_on(MOD_ID) {
        return;
    }

    let Some(p_snap) = game.player.as_ref().map(|p| LocalPlayerSnapshot {
        position: DVec3::new(p.position.x, p.position.y, p.position.z),
        previous_position: DVec3::new(
            game.previous_position.x,
            game.previous_position.y,
            game.previous_position.z,
        ),
        vanilla_velocity: DVec3::new(p.velocity.x, p.velocity.y, p.velocity.z),
        yaw_deg: game.yaw,
        pitch_deg: game.pitch,
        on_ground: p.ground.on_ground,
        horizontal_collision: p.collision.horizontal,
        vertical_collision: p.collision.vertical,
        in_fluid: p.swimming,
        creative_flying: p.flying,
        spectator: p.spectator,
    }) else {
        return;
    };

    let targets = collect_client_targets(game);

    let tick_res = CLIENT.with(|cell| {
        let mut st = cell.borrow_mut();
        ensure_client_initialized(&mut st, game);
        if game.screen_over_playing() && game.native_mod_screen() != Some(MOD_ID) {
            st.session.input.reset_all();
        }
        let world_view = GameWorldView { game };
        st.session.step_client_tick(&world_view, p_snap, &targets)
    });

    if let Some(kick) = tick_res.velocity_kick {
        game.native_push_player(kick);
    }
    if tick_res.reset_fall_distance
        && let Some(p) = game.player.as_mut()
    {
        p.fall_distance = 0.0;
    }

    // Smooth auto-third-person switching for Insomniac Spider-Cam!
    if tick_res.request_third_person {
        if game.camera_type.first_person() {
            CLIENT.with(|cell| {
                cell.borrow_mut().saved_camera_before_auto = Some(game.camera_type);
            });
            game.camera_type = CameraType::ThirdPersonBack;
            game.check_entity_post_effect(false);
        }
    } else if tick_res.release_auto_third_person {
        let prev = CLIENT.with(|cell| cell.borrow_mut().saved_camera_before_auto.take());
        if let Some(saved) = prev {
            let was_fp = game.camera_type.first_person();
            game.camera_type = saved;
            if was_fp != game.camera_type.first_person() {
                game.check_entity_post_effect(game.camera_type.first_person());
            }
        }
    }

    if tick_res.open_settings_screen {
        if game.native_mod_screen() == Some(MOD_ID) {
            game.native_mod_screen_close(MOD_ID);
        } else {
            let _ = game.native_mod_screen_open(MOD_ID);
        }
    }

    if let Some(pkt) = tick_res.outbound_gadget_packet
        && let Ok(bytes) = serde_json::to_vec(&pkt)
    {
        let _ = game.native_net_send_to_server(GADGET_NET_CHANNEL, &bytes);
    }

    if let Some(notice) = tick_res.action_bar_notice {
        game.native_client_action_bar(Text::literal(&notice), false);
    }

    for cue in tick_res.sounds {
        let _ = game.native_play_local_sound(&cue.sound, cue.at, cue.volume, cue.pitch, cue.delay_ticks);
    }

    for pc in tick_res.particles {
        let ty = match pc.flavor {
            ParticleFlavor::SilkMist => ParticleType::Poof,
            ParticleFlavor::AnchorSpark => ParticleType::Crit,
            ParticleFlavor::WindStreak => ParticleType::WhiteSmoke,
            ParticleFlavor::GadgetBurst => ParticleType::Poof,
        };
        let opts = ParticleOptions::simple(ty);
        let _ = game.native_add_particle(&opts, pc.at, pc.velocity);
    }
}

pub fn on_camera_distance(game: &Game<'_>, mirrored: bool, distance: &mut f32) {
    if !game.native_window() || !game.native_on(MOD_ID) || mirrored {
        return;
    }
    CLIENT.with(|cell| {
        let st = cell.borrow();
        if st.session.config.mod_enabled && st.session.config.camera_enabled {
            *distance = st
                .session
                .latest_cam_output
                .map_or(st.session.config.camera_base_distance, |o| o.effective_distance);
        }
    });
}

pub fn on_camera_pose(
    game: &Game<'_>,
    partial: f32,
    position: &mut [f64; 3],
    yaw: &mut f32,
    pitch: &mut f32,
) {
    if !game.native_window()
        || !game.native_on(MOD_ID)
        || game.camera_type != CameraType::ThirdPersonBack
    {
        return;
    }
    let Some(interp_feet) = interpolated_local_feet(game, partial) else {
        return;
    };
    let on_ground = game.player.as_ref().is_none_or(|p| p.ground.on_ground);

    CLIENT.with(|cell| {
        let mut st = cell.borrow_mut();
        let world_view = GameWorldView { game };
        if let Some(out) = st.session.evaluate_camera_frame(
            &world_view,
            interp_feet,
            game.yaw,
            game.pitch,
            on_ground,
        ) {
            *position = out.position;
            *yaw = out.yaw;
            *pitch = out.pitch;
        }
    });
}

pub fn on_camera_shake(game: &Game<'_>, shake: &mut NativeShake) {
    if !game.native_window()
        || !game.native_on(MOD_ID)
        || game.camera_type == CameraType::FirstPerson
    {
        return;
    }
    CLIENT.with(|cell| {
        let st = cell.borrow();
        if st.session.config.mod_enabled
            && st.session.config.camera_enabled
            && let Some(cam) = st.session.latest_cam_output
        {
            shake.roll += cam.roll_deg;
        }
    });
}

pub fn on_compute_fov(game: &Game<'_>, degrees: &mut f32) {
    if !game.native_window() || !game.native_on(MOD_ID) {
        return;
    }
    CLIENT.with(|cell| {
        let st = cell.borrow();
        if st.session.config.mod_enabled
            && st.session.config.camera_enabled
            && let Some(cam) = st.session.latest_cam_output
        {
            *degrees = (*degrees + cam.fov_bonus_deg).clamp(30.0, 138.0);
        }
    });
}

pub fn on_world_primitives(
    game: &Game<'_>,
    partial: f32,
    primitives: &mut WorldPrimitives,
) {
    if !game.native_window() || !game.native_on(MOD_ID) {
        return;
    }
    let Some(interp_feet) = interpolated_local_feet(game, partial) else {
        return;
    };
    let targets = collect_client_targets(game);

    let mut segments = Vec::<RenderSegment3D>::with_capacity(64);
    CLIENT.with(|cell| {
        let st = cell.borrow();
        if !st.session.config.mod_enabled {
            return;
        }
        let reaching = st.session.swing.phase.is_active();
        let yaw = game.yaw;
        let pitch = game.pitch;
        st.session.strands.build_render_segments(
            partial,
            st.session.config.web_strand_thickness,
            st.session.config.web_sag_visuals,
            |wrist: WristSide| {
                SwingController::compute_wrist_origin(interp_feet, yaw, pitch, wrist, reaching)
            },
            &mut segments,
        );
        st.session.gadgets.build_render_segments(
            partial,
            st.session.config.suit_variant.web_tint_argb(),
            &targets,
            &mut segments,
        );
    });

    for seg in segments {
        primitives.line(seg.from, seg.to, seg.color_argb, seg.width);
    }
}

pub fn on_player_render_listened(game: &Game<'_>, listened: &mut bool) {
    if !game.native_window() || !game.native_on(MOD_ID) {
        return;
    }
    CLIENT.with(|cell| {
        let st = cell.borrow();
        if st.session.config.mod_enabled
            && (st.session.config.suit_mode != SuitRenderMode::Hidden
                || !st.session.gadgets.webbed_entities.is_empty())
        {
            *listened = true;
        }
    });
}

pub fn on_player_submit(
    game: &Game<'_>,
    player: &NativeMobRender<'_>,
    who: &NativePlayerRender<'_>,
    draw: &mut BlockEntityDraw,
    drawn: &mut bool,
) {
    if !who.local || *drawn || !game.native_window() || !game.native_on(MOD_ID) {
        return;
    }

    let mut quads = Vec::<SuitQuad>::new();
    CLIENT.with(|cell| {
        let mut st = cell.borrow_mut();
        if !st.session.config.mod_enabled || st.session.config.suit_mode != SuitRenderMode::SkinSuit {
            return;
        }
        let pose = st.session.evaluate_player_pose(
            player.head_yaw,
            player.pitch,
            player.walk_position,
            player.walk_speed,
            player.age,
            player.on_ground,
        );
        build_spider_suit_quads(&pose, &st.session.config, player.hurt_time > 0, &mut quads);
    });

    if quads.is_empty() {
        return;
    }

    let mut any_ok = false;
    for q in quads {
        if game.native_draw_quad(draw, MOD_IMAGE_SPRITE, q.corners, q.uv_16, q.tint_rgb) {
            any_ok = true;
        }
    }
    if any_ok {
        *drawn = true;
    }
}

pub fn on_mob_layer(
    game: &Game<'_>,
    mob: &NativeMobRender<'_>,
    who: Option<&NativePlayerRender<'_>>,
    draw: &mut BlockEntityDraw,
) {
    if !game.native_window() || !game.native_on(MOD_ID) {
        return;
    }

    let mut quads = Vec::<SuitQuad>::new();
    CLIENT.with(|cell| {
        let mut st = cell.borrow_mut();
        if !st.session.config.mod_enabled {
            return;
        }
        if let Some(player_who) = who {
            // Local player in `WebShootersOnly` overlay mode: draw wrist shooters + Web Wings on vanilla skin!
            if player_who.local && st.session.config.suit_mode == SuitRenderMode::WebShootersOnly {
                let pose = st.session.evaluate_player_pose(
                    mob.head_yaw,
                    mob.pitch,
                    mob.walk_position,
                    mob.walk_speed,
                    mob.age,
                    mob.on_ground,
                );
                build_spider_suit_quads(&pose, &st.session.config, mob.hurt_time > 0, &mut quads);
            }
        } else if st.session.gadgets.is_entity_visually_webbed(mob.id) {
            // Non-player mob restrained by Spider-Gadget: draw 3D spider-silk cocoon wrap!
            build_mob_web_cocoon_quads(&mut quads);
        }
    });

    for q in quads {
        let _ = game.native_draw_quad(draw, MOD_IMAGE_SPRITE, q.corners, q.uv_16, q.tint_rgb);
    }
}

pub fn on_render_hand_pose(
    game: &Game<'_>,
    hand: Hand,
    _stack: Option<&ItemStack>,
    pitch: &mut f32,
    y: &mut f32,
    z: &mut f32,
) {
    if !game.native_window() || !game.native_on(MOD_ID) {
        return;
    }
    CLIENT.with(|cell| {
        let st = cell.borrow();
        if !st.session.config.mod_enabled || !st.session.config.arm_shoot_animation {
            return;
        }
        let pose = st.session.animations.current_pose();
        let impulse = match hand {
            Hand::Main => pose.fp_right_shot,
            Hand::Off => pose.fp_left_shot,
        };
        if impulse > 0.02 {
            *pitch -= impulse * 28.0;
            *y += impulse * 0.14;
            *z -= impulse * 0.18;
        }
    });
}

pub fn on_hud(game: &mut Game<'_>, batch: &mut HudBatch, _corners: &mut Corners) {
    if !game.native_window() || !game.native_on(MOD_ID) || game.debug {
        return;
    }
    if game.player.is_none() || game.screen_over_playing() {
        return;
    }

    let mut cmds = Vec::<UiDrawCmd>::with_capacity(28);
    CLIENT.with(|cell| {
        let mut st = cell.borrow_mut();
        ensure_client_initialized(&mut st, game);
        ensure_atlas_uploaded(&mut st, game);
        let speed = st.session.authoritative_velocity().length();
        st.session.ui.build_hud(
            batch.width,
            batch.height,
            &st.session.config,
            &st.session.swing,
            &st.session.traversal,
            &st.session.gadgets,
            speed,
            &mut cmds,
        );
    });

    paint_ui_commands(game, batch, &cmds);
}

pub fn on_screen_render_post(
    game: &Game<'_>,
    batch: &mut HudBatch,
    screen: &NativeScreenView<'_>,
) {
    if !game.native_window() || game.native_mod_screen() != Some(MOD_ID) {
        return;
    }
    let _ = game.native_apply_screen_blur();

    let mut cmds = Vec::<UiDrawCmd>::with_capacity(96);
    CLIENT.with(|cell| {
        let st = cell.borrow();
        st.session.ui.build_settings_screen(
            screen.rect,
            screen.mouse,
            &st.session.config,
            &st.session.input,
            &mut cmds,
        );
    });

    paint_ui_commands(game, batch, &cmds);
}

pub fn on_screen_mouse_pressed(
    game: &Game<'_>,
    screen: &NativeScreenView<'_>,
    button: i32,
    canceled: &mut bool,
) {
    if !game.native_window() || game.native_mod_screen() != Some(MOD_ID) {
        return;
    }
    *canceled = true;

    let mut save_to_disk = false;
    let mut reload_suit = false;
    let mut close_now = false;

    CLIENT.with(|cell| {
        let mut st = cell.borrow_mut();
        match st.session.ui.on_mouse_press(
            screen.rect,
            screen.mouse,
            button,
            &mut st.session.config,
            &mut st.session.input,
        ) {
            UiClickOutcome::ConfigChanged => {
                save_to_disk = true;
                reload_suit = true;
            }
            UiClickOutcome::ReloadSuitTexture => {
                reload_suit = true;
            }
            UiClickOutcome::CloseScreen => {
                close_now = true;
            }
            UiClickOutcome::None => {}
        }
        if reload_suit {
            let dir = game.native_data_dir(MOD_ID);
            st.session
                .suit_atlas
                .reload_from_config(&st.session.config, Some(&dir));
        }
        if save_to_disk && let Some(cfg_path) = config_file_path() {
            let _ = fs::write(&cfg_path, st.session.config.to_json_pretty());
        }
        if close_now {
            // Queue close on next key/tick or Escape; also notify user
            st.session.ui.set_banner("Press Esc or K to return to game");
        }
    });
}

pub fn on_screen_key_pressed(
    game: &Game<'_>,
    key: &str,
    canceled: &mut bool,
) {
    if !game.native_window() || game.native_mod_screen() != Some(MOD_ID) {
        return;
    }
    CLIENT.with(|cell| {
        let mut st = cell.borrow_mut();
        if st.session.input.awaiting_rebind.is_some() {
            let changed = st
                .session
                .input
                .on_key_event(key, true, &mut st.session.config.keys);
            if changed && let Some(cfg_path) = config_file_path() {
                let _ = fs::write(&cfg_path, st.session.config.to_json_pretty());
            }
            *canceled = true;
        }
    });
}

fn paint_ui_commands(game: &Game<'_>, batch: &mut HudBatch, cmds: &[UiDrawCmd]) {
    for cmd in cmds {
        match cmd {
            UiDrawCmd::FillRect { rect, color_argb } => {
                game.native_fill(batch, *rect, *color_argb);
            }
            UiDrawCmd::ModSprite { src, dst, .. } => {
                let _ = game.native_sprite(batch, MOD_IMAGE_SPRITE, *src, *dst);
            }
            UiDrawCmd::Text {
                text,
                at,
                color_argb,
            } => {
                game.native_text(batch, text, *at, *color_argb & 0x00FF_FFFF, true);
            }
        }
    }
}

// ============================================================================
// Client/Server Network, Gadget Authority & Clean Lifecycle Resets
// ============================================================================

pub fn on_net_from_client(game: &mut Game<'_>, _player: u128, channel: &str, data: &[u8]) {
    if channel != GADGET_NET_CHANNEL {
        return;
    }
    let targets = collect_server_targets(game);
    let mut impulses = Vec::<MobImpulseCommand>::new();
    {
        let Ok(mut guard) = SERVER.lock() else {
            return;
        };
        let srv = guard.get_or_insert_with(FlyingWebServerRuntime::new);
        let world_view = GameWorldView { game };
        srv.handle_client_packet(&world_view, data, &targets, &mut impulses);
    }
    apply_server_mob_impulses(game, &impulses);
}

pub fn on_server_tick(game: &mut Game<'_>, phase: ServerTickPhase) {
    if !matches!(phase, ServerTickPhase::End) {
        return;
    }
    let targets = collect_server_targets(game);
    let mut impulses = Vec::<MobImpulseCommand>::new();
    let broadcast = {
        let Ok(mut guard) = SERVER.lock() else {
            return;
        };
        let srv = guard.get_or_insert_with(FlyingWebServerRuntime::new);
        let world_view = GameWorldView { game };
        srv.step_server_tick(&world_view, &targets, &mut impulses)
    };
    apply_server_mob_impulses(game, &impulses);

    if let Some(payload) = broadcast {
        // Sync webbed mob cocoons & tethers to host player and any connected LAN/SMP players!
        let recipients: Vec<u128> = game
            .native_server_players()
            .into_iter()
            .map(|p| p.uuid)
            .collect();
        for uuid in recipients {
            let _ = game.native_net_send_to_player(uuid, GADGET_NET_CHANNEL, &payload);
        }
    }
}

pub fn on_net_from_server(channel: &str, data: &[u8]) {
    if channel != GADGET_NET_CHANNEL {
        return;
    }
    let Ok(msg) = serde_json::from_slice::<GadgetNetBroadcast>(data) else {
        return;
    };
    CLIENT.with(|cell| {
        let mut st = cell.borrow_mut();
        match msg {
            GadgetNetBroadcast::WebbedEntities(entries) => {
                st.session.gadgets.apply_sync_entries(entries);
            }
        }
    });
}

pub fn on_living_incoming_damage(entity: i32, damage_type: &str, canceled: &mut bool) {
    if *canceled || entity < 0 {
        return;
    }
    let Ok(guard) = SERVER.lock() else {
        return;
    };
    if let Some(srv) = guard.as_ref()
        && srv.non_lethal_protection
        && srv
            .gadgets
            .should_protect_entity_from_fall(entity as u32, damage_type)
    {
        *canceled = true;
    }
}

pub fn on_client_world_reset(game: &mut Game<'_>) {
    let restore_cam = CLIENT.with(|cell| {
        let mut st = cell.borrow_mut();
        st.session.reset_transient_state();
        st.saved_camera_before_auto.take()
    });
    if let Some(saved) = restore_cam {
        let was_fp = game.camera_type.first_person();
        game.camera_type = saved;
        if was_fp != game.camera_type.first_person() {
            game.check_entity_post_effect(game.camera_type.first_person());
        }
    }
}

pub fn on_server_player_reset() {
    CLIENT.with(|cell| {
        cell.borrow_mut().session.reset_transient_state();
    });
}

pub fn on_world_lifecycle(phase: WorldPhase) {
    if matches!(phase, WorldPhase::Stopping | WorldPhase::Stopped) {
        if let Ok(mut guard) = SERVER.lock()
            && let Some(srv) = guard.as_mut()
        {
            srv.reset();
        }
        CLIENT.with(|cell| {
            cell.borrow_mut().session.reset_transient_state();
        });
    }
}
