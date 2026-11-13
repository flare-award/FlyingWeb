use std::collections::HashSet;

use glam::{DVec3, IVec3};

use crate::{
    FlyingWebClientSession, LocalPlayerSnapshot,
    camera::{CameraFrameInput, CameraTraversalMode, SpiderCamera},
    config::{FlyingWebConfig, SuitRenderMode, SuitVariant},
    gadgets::{GadgetKind, GadgetManager, TargetEntitySnapshot},
    input::SpiderAction,
    movement::TraversalState,
    physics::{PendulumConstraint, PendulumStepInput, compute_release_velocity, step_pendulum},
    raycast::{
        VoxelSample, WorldQuery, find_best_swing_anchor, find_point_launch_perch, look_direction,
        raycast_blocks,
    },
    suit::{SuitAtlasManager, build_spider_suit_quads},
};

const SLAB_TOP_BOX: [[f64; 6]; 1] = [[0.0, 0.5, 0.0, 1.0, 1.0, 1.0]];

/// Deterministic mock voxel world for unit and integration tests.
struct TestWorld {
    solids: HashSet<IVec3>,
    slabs: HashSet<IVec3>,
    fluids: HashSet<IVec3>,
    unloaded_x_above: Option<i32>,
}

impl TestWorld {
    fn new() -> Self {
        Self {
            solids: HashSet::new(),
            slabs: HashSet::new(),
            fluids: HashSet::new(),
            unloaded_x_above: None,
        }
    }

    fn add_floor(&mut self, y: i32, half_extent: i32) {
        for x in -half_extent..=half_extent {
            for z in -half_extent..=half_extent {
                self.solids.insert(IVec3::new(x, y, z));
            }
        }
    }

    fn add_wall_x(&mut self, x: i32, y0: i32, y1: i32, z0: i32, z1: i32) {
        for y in y0..=y1 {
            for z in z0..=z1 {
                self.solids.insert(IVec3::new(x, y, z));
            }
        }
    }
}

impl WorldQuery for TestWorld {
    fn sample_voxel(&self, pos: IVec3) -> VoxelSample<'_> {
        if let Some(limit_x) = self.unloaded_x_above
            && pos.x > limit_x
        {
            return VoxelSample::UNLOADED;
        }
        if self.fluids.contains(&pos) {
            return VoxelSample {
                loaded: true,
                solid: false,
                fluid: true,
                blocks_camera: false,
                boxes: &[],
                step_sound: "",
            };
        }
        if self.slabs.contains(&pos) {
            return VoxelSample {
                loaded: true,
                solid: true,
                fluid: false,
                blocks_camera: true,
                boxes: &SLAB_TOP_BOX,
                step_sound: "minecraft:block.stone.step",
            };
        }
        if self.solids.contains(&pos) {
            return VoxelSample::SOLID_CUBE;
        }
        VoxelSample::AIR
    }
}

#[test]
fn raycast_rejects_air_fluids_and_unloaded_chunks_and_hits_subblock_slabs() {
    let mut world = TestWorld::new();
    // Empty sky -> None
    assert!(raycast_blocks(&world, DVec3::ZERO, DVec3::Z, 50.0, false).is_none());

    // Water block at (0, 0, 5) -> ignored, does not anchor
    world.fluids.insert(IVec3::new(0, 0, 5));
    assert!(raycast_blocks(&world, DVec3::new(0.5, 0.5, 0.5), DVec3::Z, 20.0, false).is_none());

    // Top-half slab at (0, 0, 8): ray at y=0.25 passes underneath; ray at y=0.75 hits!
    world.slabs.insert(IVec3::new(0, 0, 8));
    assert!(raycast_blocks(&world, DVec3::new(0.5, 0.25, 0.5), DVec3::Z, 20.0, false).is_none());
    let slab_hit =
        raycast_blocks(&world, DVec3::new(0.5, 0.75, 0.5), DVec3::Z, 20.0, false).unwrap();
    assert_eq!(slab_hit.block_pos, IVec3::new(0, 0, 8));
    assert_eq!(slab_hit.normal, DVec3::NEG_Z);

    // Unloaded chunk boundary stops ray before solid block behind it
    world.unloaded_x_above = Some(4);
    world.solids.insert(IVec3::new(10, 0, 0));
    assert!(raycast_blocks(&world, DVec3::new(0.5, 0.5, 0.5), DVec3::X, 20.0, false).is_none());
}

#[test]
fn smart_cone_anchor_never_grabs_through_solid_wall() {
    let mut world = TestWorld::new();
    // Near wall completely blocking +Z at z = 4
    for x in -6..=6 {
        for y in -2..=12 {
            world.solids.insert(IVec3::new(x, y, 4));
        }
    }
    // Tall tower behind the wall at z = 18
    world.solids.insert(IVec3::new(0, 16, 18));

    let eye = DVec3::new(0.5, 2.6, 0.5);
    let wrist = DVec3::new(0.8, 2.2, 0.5);
    let aim = look_direction(0.0, -15.0); // Looking toward +Z and slightly up
    let cand = find_best_swing_anchor(
        &world,
        eye,
        wrist,
        aim,
        1.0,
        DVec3::ZERO,
        2.0,
        45.0,
        22.0,
    )
    .expect("should hit near wall face");

    // Must hit the front face of the near wall (z == 4), NEVER the occluded block at z == 18!
    assert_eq!(cand.hit.block_pos.z, 4);
}

#[test]
fn pendulum_swing_conserves_arc_mechanics_and_enforces_speed_ceiling() {
    let cfg = FlyingWebConfig::default();
    let anchor = DVec3::new(0.0, 25.0, 0.0);
    let rope_len = 15.0;

    // Start horizontally displaced at (-15, 25, 0) with zero velocity -> pure pendulum drop!
    let mut constraint = PendulumConstraint {
        anchor,
        anchor_normal: DVec3::NEG_Y,
        max_length: rope_len,
        min_length: 2.5,
        tension: 0.0,
        is_taut: true,
    };
    let mut pivot = DVec3::new(-15.0, 25.0, 0.0);
    let mut vel = DVec3::ZERO;

    let mut nadir_speed = 0.0_f64;
    let mut reached_bottom = false;
    let mut climbed_upswing = false;

    for _ in 0..45 {
        let res = step_pendulum(
            constraint,
            PendulumStepInput {
                pivot_pos: pivot,
                velocity: vel,
                wish_dir: DVec3::ZERO,
                forward_input: 0.0,
                reel_delta: 0.0,
                ground_clearance: None,
                on_ground: false,
                sneak_held: false,
            },
            &cfg,
        );
        vel = res.new_velocity;
        constraint = res.updated_constraint;
        pivot += vel;

        // Rope constraint keeps pivot near radius `rope_len` (never flying off infinitely)
        assert!(
            pivot.distance(anchor) <= rope_len + 0.65,
            "rope stretched too far: {}",
            pivot.distance(anchor)
        );

        if pivot.x >= -1.0 && !reached_bottom {
            reached_bottom = true;
            nadir_speed = vel.length();
            assert!(
                nadir_speed > 0.85,
                "downswing must build real kinetic speed, got {nadir_speed}"
            );
        }
        if reached_bottom && pivot.x > 6.0 && vel.y > 0.15 {
            climbed_upswing = true;
        }
    }
    assert!(reached_bottom, "pendulum must swing through nadir");
    assert!(
        climbed_upswing,
        "pendulum must convert kinetic speed into upward climb past nadir"
    );

    // Verify 500 ticks of continuous maximum pumping NEVER exceeds asymptotic speed cap!
    for _ in 0..500 {
        let res = step_pendulum(
            constraint,
            PendulumStepInput {
                pivot_pos: pivot,
                velocity: vel,
                wish_dir: DVec3::X,
                forward_input: 1.0,
                reel_delta: 0.0,
                ground_clearance: None,
                on_ground: false,
                sneak_held: false,
            },
            &cfg,
        );
        vel = res.new_velocity;
        constraint = res.updated_constraint;
        pivot += vel;
        assert!(
            vel.length() <= cfg.max_swing_speed * 1.06,
            "infinite speed exploit detected: {}",
            vel.length()
        );
    }

    // Verify Jump-Vault release preserves momentum and adds upward boost
    let pre_release = DVec3::new(1.2, 0.3, 0.0);
    let post_release = compute_release_velocity(pre_release, DVec3::X, true, &cfg);
    assert!(post_release.y > pre_release.y + 0.15);
    assert!(post_release.x > pre_release.x);
}

#[test]
fn spider_camera_avoids_wall_clipping_and_adapts_fov_to_speed() {
    let mut world = TestWorld::new();
    // Place a solid wall 2.0 blocks behind the player (`yaw = 0` looks +Z, camera is at -Z)
    for x in -4..=4 {
        for y in 0..=6 {
            world.solids.insert(IVec3::new(x, y, -2));
        }
    }

    let cfg = FlyingWebConfig::default();
    let mut cam = SpiderCamera::default();
    let pose_blocked = cam.evaluate_frame(
        &world,
        CameraFrameInput {
            interp_feet: DVec3::new(0.5, 1.0, 0.5),
            velocity: DVec3::ZERO,
            player_yaw: 0.0,
            player_pitch: 0.0,
            on_ground: true,
            mode: CameraTraversalMode::Grounded,
            surface_normal: DVec3::Y,
            swing_bank: 0.0,
            dt_sec: 0.05,
        },
        &cfg,
    );
    // Wall front face is at z = -1.0; camera must stay safely in front of z = -1.0!
    assert!(
        pose_blocked.position[2] > -0.95,
        "camera clipped into wall behind player: z = {}",
        pose_blocked.position[2]
    );

    // Clear wall and verify high-speed swing expands dynamic FOV & boom distance smoothly
    world.solids.clear();
    let mut high_speed_pose = pose_blocked;
    for _ in 0..30 {
        high_speed_pose = cam.evaluate_frame(
            &world,
            CameraFrameInput {
                interp_feet: DVec3::new(0.5, 20.0, 0.5),
                velocity: DVec3::new(0.0, -0.2, 2.1),
                player_yaw: 0.0,
                player_pitch: 8.0,
                on_ground: false,
                mode: CameraTraversalMode::WebSwinging,
                surface_normal: DVec3::Y,
                swing_bank: 0.75,
                dt_sec: 1.0 / 60.0,
            },
            &cfg,
        );
    }
    assert!(high_speed_pose.fov_bonus_deg > 10.0);
    assert!(high_speed_pose.effective_distance > cfg.camera_base_distance + 1.0);
    assert!(high_speed_pose.roll_deg.abs() > 2.0);
}

#[test]
fn web_wings_gliding_loses_altitude_and_is_not_creative_flight() {
    let world = TestWorld::new();
    let mut session = FlyingWebClientSession::default();
    let mut pos = DVec3::new(0.0, 80.0, 0.0);
    let mut prev = pos;

    // Deploy Web Wings in mid-air
    session.input.set_action_state(SpiderAction::WebWings, true);
    let res = session.step_client_tick(
        &world,
        LocalPlayerSnapshot {
            position: pos,
            previous_position: prev,
            vanilla_velocity: DVec3::new(0.0, -0.1, 0.8),
            yaw_deg: 0.0,
            pitch_deg: 5.0, // Nearly level glide
            on_ground: false,
            horizontal_collision: false,
            vertical_collision: false,
            in_fluid: false,
            creative_flying: false,
            spectator: false,
        },
        &[],
    );
    session.input.set_action_state(SpiderAction::WebWings, false);
    assert!(matches!(
        session.traversal.state,
        TraversalState::WebWingsGliding { .. }
    ));
    let mut vel = DVec3::new(0.0, -0.1, 0.8)
        + DVec3::from_array(res.velocity_kick.unwrap().map(|c| c as f64));

    let start_y = pos.y;
    for _ in 0..40 {
        prev = pos;
        pos += vel;
        let tick_out = session.step_client_tick(
            &world,
            LocalPlayerSnapshot {
                position: pos,
                previous_position: prev,
                vanilla_velocity: vel * 0.91,
                yaw_deg: 0.0,
                pitch_deg: 4.0,
                on_ground: false,
                horizontal_collision: false,
                vertical_collision: false,
                in_fluid: false,
                creative_flying: false,
                spectator: false,
            },
            &[],
        );
        let kick = tick_out.velocity_kick.unwrap();
        vel = vel * 0.91 + DVec3::new(kick[0] as f64, kick[1] as f64, kick[2] as f64);
    }

    // Must cover substantial forward horizontal distance while naturally losing altitude!
    assert!(pos.z > 18.0, "Web Wings should glide forward: z={}", pos.z);
    assert!(
        pos.y < start_y - 1.0,
        "Web Wings level glide must lose altitude (not creative flight): y={}",
        pos.y
    );
}

#[test]
fn point_launch_wall_run_and_ceiling_crawl_work_against_world_geometry() {
    let mut world = TestWorld::new();
    // Build a rooftop pillar at (0, 6, 10) with clear air above
    world.solids.insert(IVec3::new(0, 6, 10));
    let perch = find_point_launch_perch(
        &world,
        DVec3::new(0.5, 5.62, 0.5),
        look_direction(0.0, -6.0),
        3.0,
        30.0,
    )
    .expect("should detect valid Point-Launch rooftop perch");
    assert!((perch.position.y - 7.0).abs() < 1e-4);

    // Build an overhead ceiling at y = 10 and verify inverted Ceiling Crawl attaches cleanly!
    for x in -3..=3 {
        for z in -3..=3 {
            world.solids.insert(IVec3::new(x, 10, z));
        }
    }
    let mut session = FlyingWebClientSession::default();
    session.input.record_movement_axes(1.0, 0.0, false, false);
    session
        .input
        .set_action_state(SpiderAction::WallCrawlToggle, true);
    let _ = session.step_client_tick(
        &world,
        LocalPlayerSnapshot {
            position: DVec3::new(0.5, 8.15, 0.5), // Head at 9.87 -> 0.13 blocks below ceiling y=10!
            previous_position: DVec3::new(0.5, 8.15, 0.5),
            vanilla_velocity: DVec3::ZERO,
            yaw_deg: 0.0,
            pitch_deg: 0.0,
            on_ground: false,
            horizontal_collision: false,
            vertical_collision: false,
            in_fluid: false,
            creative_flying: false,
            spectator: false,
        },
        &[],
    );
    assert!(matches!(
        session.traversal.state,
        TraversalState::SurfaceCrawling { ceiling: true, .. }
    ));

    // Build a vertical wall at x = 1 and verify Wall Run attaches when sprinting forward in air!
    session.reset_transient_state();
    world.solids.clear();
    world.add_wall_x(1, 0, 10, -5, 25);
    session.input.record_movement_axes(1.0, 0.0, false, false);
    let _ = session.step_client_tick(
        &world,
        LocalPlayerSnapshot {
            position: DVec3::new(0.45, 3.0, 0.0),
            previous_position: DVec3::new(0.45, 3.0, -0.5),
            vanilla_velocity: DVec3::new(0.0, 0.0, 0.5),
            yaw_deg: 0.0,
            pitch_deg: 0.0,
            on_ground: false,
            horizontal_collision: false,
            vertical_collision: false,
            in_fluid: false,
            creative_flying: false,
            spectator: false,
        },
        &[],
    );
    assert!(matches!(
        session.traversal.state,
        TraversalState::WallRunning { .. }
    ));
}

#[test]
fn all_eight_gadgets_and_non_lethal_fall_protection_operate_correctly() {
    let mut world = TestWorld::new();
    world.add_floor(0, 20);
    // Opposing walls at z = -6 and z = 8 for WebLine tightrope
    for y in 1..=8 {
        for x in -4..=4 {
            world.solids.insert(IVec3::new(x, y, -6));
            world.solids.insert(IVec3::new(x, y, 8));
        }
    }

    let cfg = FlyingWebConfig::default();
    let mut mgr = GadgetManager::default();
    let mut events = Vec::new();
    let mut impulses = Vec::new();

    // 1. Cast Traversable WebLine between z=8 and z=-6 at y=4.5
    assert!(
        mgr.try_use_gadget(
            &world,
            GadgetKind::WebLine,
            DVec3::new(0.5, 4.5, 0.5),
            DVec3::Z,
            false,
            &cfg,
            &mut events,
        )
        .is_some()
    );
    assert_eq!(mgr.web_lines.len(), 1);
    // Player stepping onto the tightrope at (0.5, 4.6, 2.0) gets supported!
    let support = mgr.evaluate_tightrope_support(
        DVec3::new(0.5, 4.6, 2.0),
        DVec3::new(0.0, -0.1, 0.2),
        false,
    );
    assert!(support.is_some());

    // 2. Impact Web webs target mob and grants non-lethal fall-damage protection!
    let mobs = vec![
        TargetEntitySnapshot {
            id: 42,
            position: DVec3::new(0.5, 1.0, 4.0),
            height: 1.95,
            is_player: false,
            can_restrain: true,
        },
        TargetEntitySnapshot {
            id: 43,
            position: DVec3::new(2.5, 1.0, 5.5),
            height: 1.95,
            is_player: false,
            can_restrain: true,
        },
    ];
    let _ = mgr.try_use_gadget(
        &world,
        GadgetKind::ImpactWeb,
        DVec3::new(0.5, 2.0, 1.0),
        DVec3::Z,
        false,
        &cfg,
        &mut events,
    );
    for _ in 0..4 {
        mgr.tick(&world, &mobs, &mut impulses, &mut events);
    }
    assert!(mgr.is_entity_visually_webbed(42));
    assert!(mgr.should_protect_entity_from_fall(42, "minecraft:fall"));
    assert!(!mgr.should_protect_entity_from_fall(42, "minecraft:player_attack"));

    // 3. Ricochet Web chains from mob 42 -> mob 43!
    let mut ric_mgr = GadgetManager::default();
    let _ = ric_mgr.try_use_gadget(
        &world,
        GadgetKind::RicochetWeb,
        DVec3::new(0.5, 1.9, 1.0),
        DVec3::Z,
        false,
        &cfg,
        &mut events,
    );
    for _ in 0..8 {
        ric_mgr.tick(&world, &mobs, &mut impulses, &mut events);
    }
    assert!(ric_mgr.is_entity_visually_webbed(42));
    assert!(ric_mgr.is_entity_visually_webbed(43));

    // 4. Remote Mine place + remote detonate webs nearby mobs!
    let mut rem_mgr = GadgetManager::default();
    let _ = rem_mgr.try_use_gadget(
        &world,
        GadgetKind::RemoteMine,
        DVec3::new(0.5, 2.5, 4.0),
        DVec3::NEG_Y,
        false,
        &cfg,
        &mut events,
    );
    for _ in 0..5 {
        rem_mgr.tick(&world, &mobs, &mut impulses, &mut events);
    }
    assert!(rem_mgr.has_armed_remote_mines());
    rem_mgr.detonate_remote_mines(&mobs, &mut impulses, &mut events);
    assert!(!rem_mgr.has_armed_remote_mines());
    assert!(rem_mgr.is_entity_visually_webbed(42));
}

#[test]
fn suit_png_atlas_and_session_reset_are_lossless_and_leak_free() {
    let mut atlas = SuitAtlasManager::default();
    for variant in [
        SuitVariant::Classic,
        SuitVariant::Advanced,
        SuitVariant::Miles,
        SuitVariant::Symbiote,
    ] {
        atlas.load_builtin_suit(variant);
        assert!(atlas.dirty);
    }

    let mut cfg = FlyingWebConfig::default();
    cfg.suit_mode = SuitRenderMode::SkinSuit;
    cfg.show_web_wings_model = true;
    let mut pose = crate::animations::SpiderRigPose::default();
    pose.web_wings_deploy = 1.0;

    let mut quads = Vec::new();
    build_spider_suit_quads(&pose, &cfg, false, &mut quads);
    // 6 limbs * 2 layers * 6 faces = 72 quads + 4 Web-Wings quads = 76 quads!
    assert_eq!(quads.len(), 76);

    // Config JSON round-trip + NaN sanitization
    let mut broken = FlyingWebConfig::default();
    broken.max_web_range = f64::NAN;
    broken.camera_base_distance = -999.0;
    broken.custom_suit_filename = "../escape.png".to_owned();
    broken.sanitize();
    assert!(broken.max_web_range.is_finite());
    assert!(broken.camera_base_distance >= 2.0);
    assert_eq!(broken.custom_suit_filename, "custom_suit.png");

    let json = cfg.to_json_pretty();
    let parsed = FlyingWebConfig::from_json_lossless(&json);
    assert_eq!(cfg, parsed);

    // World exit / death reset clears every active strand and transient state
    let mut session = FlyingWebClientSession::default();
    session.strands.spawn_swing_strand(
        crate::input::WristSide::Right,
        DVec3::ZERO,
        DVec3::new(0.0, 10.0, 0.0),
        DVec3::NEG_Y,
        0xFFFF_FFFF,
    );
    assert_eq!(session.strands.active_count(), 1);
    session.reset_transient_state();
    assert_eq!(session.strands.active_count(), 0);
}
