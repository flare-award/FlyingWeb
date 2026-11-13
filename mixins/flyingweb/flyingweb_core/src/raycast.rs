use glam::{DVec3, IVec3};

const FULL_CUBE_BOX: [[f64; 6]; 1] = [[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]];

/// Collision & surface facts for one voxel queried by raycasts, anchor searches,
/// wall-running/crawling probes, and Spider-Cam obstacle avoidance.
#[derive(Clone, Debug)]
pub struct VoxelSample<'a> {
    /// True when the chunk containing `(x, z)` is loaded on the client.
    pub loaded: bool,
    /// True when this block can support a web anchor, wall run, or spider-crawl
    /// (solid geometry; false for air, water, lava, tall grass, flowers, torches, fire).
    pub solid: bool,
    /// True when this voxel contains water or lava.
    pub fluid: bool,
    /// True when this block visually/physically obstructs the third-person camera.
    pub blocks_camera: bool,
    /// Sub-block AABBs `[min_x, min_y, min_z, max_x, max_y, max_z]` in `0.0..=1.0` block space.
    /// Empty slice when `solid` is true falls back to the full unit cube `[0,0,0,1,1,1]`.
    pub boxes: &'a [[f64; 6]],
    /// Optional vanilla step sound event (`StateFacts.sounds.step`) for material-accurate footsteps.
    pub step_sound: &'a str,
}

impl<'a> VoxelSample<'a> {
    pub const UNLOADED: Self = Self {
        loaded: false,
        solid: false,
        fluid: false,
        blocks_camera: false,
        boxes: &[],
        step_sound: "",
    };

    pub const AIR: Self = Self {
        loaded: true,
        solid: false,
        fluid: false,
        blocks_camera: false,
        boxes: &[],
        step_sound: "",
    };

    pub const SOLID_CUBE: Self = Self {
        loaded: true,
        solid: true,
        fluid: false,
        blocks_camera: true,
        boxes: &FULL_CUBE_BOX,
        step_sound: "minecraft:block.stone.step",
    };

    pub fn effective_boxes(&self) -> &[[f64; 6]] {
        if self.boxes.is_empty() {
            &FULL_CUBE_BOX
        } else {
            self.boxes
        }
    }
}

/// Read-only abstraction over Horizon's client/server world blocks so all physics,
/// anchor raycasting, wall-crawling, and camera collision logic are 100% deterministic
/// and testable without a live GPU window.
pub trait WorldQuery {
    fn sample_voxel(&self, pos: IVec3) -> VoxelSample<'_>;

    fn is_solid_block(&self, pos: IVec3) -> bool {
        let s = self.sample_voxel(pos);
        s.loaded && s.solid && !s.fluid
    }
}

/// Exact hit result against solid world geometry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    pub position: DVec3,
    pub normal: DVec3,
    pub block_pos: IVec3,
    pub distance: f64,
}

/// Candidate web anchor evaluated by `find_best_swing_anchor`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnchorCandidate {
    pub hit: RayHit,
    pub score: f64,
    /// True if found directly on the player's crosshair ray.
    pub direct_aim: bool,
}

/// Convert Horizon yaw/pitch degrees (`yaw = 0` -> `+Z` south, `yaw = 90` -> `-X` west,
/// `pitch > 0` -> looking down) into a normalized 3D unit look vector.
pub fn look_direction(yaw_deg: f32, pitch_deg: f32) -> DVec3 {
    let yaw = (yaw_deg as f64).to_radians();
    let pitch = (pitch_deg as f64).to_radians();
    let cos_p = pitch.cos();
    DVec3::new(-yaw.sin() * cos_p, -pitch.sin(), yaw.cos() * cos_p).normalize_or_zero()
}

/// Horizontal forward unit vector on the XZ plane for `yaw_deg`.
pub fn horizontal_forward(yaw_deg: f32) -> DVec3 {
    let yaw = (yaw_deg as f64).to_radians();
    DVec3::new(-yaw.sin(), 0.0, yaw.cos()).normalize_or_zero()
}

/// Horizontal right-hand unit vector on the XZ plane for `yaw_deg`.
pub fn horizontal_right(yaw_deg: f32) -> DVec3 {
    let fwd = horizontal_forward(yaw_deg);
    // Cross(forward, +Y): for yaw=0 (+Z), right is -X.
    DVec3::new(-fwd.z, 0.0, fwd.x)
}

/// Exact slab-method Ray vs AABB intersection returning `(t_hit, outward_face_normal)`.
fn intersect_ray_aabb(
    origin: DVec3,
    dir: DVec3,
    min: DVec3,
    max: DVec3,
    max_dist: f64,
) -> Option<(f64, DVec3)> {
    let mut t_min = 0.0_f64;
    let mut t_max = max_dist;
    let mut hit_normal = DVec3::Y;
    let mut inside = true;

    for axis in 0..3 {
        let o = origin[axis];
        let d = dir[axis];
        let b_min = min[axis];
        let b_max = max[axis];

        if o < b_min || o > b_max {
            inside = false;
        }

        if d.abs() < 1e-12 {
            if o < b_min || o > b_max {
                return None;
            }
        } else {
            let inv = 1.0 / d;
            let (mut t1, mut t2, n1) = if d >= 0.0 {
                (
                    (b_min - o) * inv,
                    (b_max - o) * inv,
                    match axis {
                        0 => DVec3::NEG_X,
                        1 => DVec3::NEG_Y,
                        _ => DVec3::NEG_Z,
                    },
                )
            } else {
                (
                    (b_max - o) * inv,
                    (b_min - o) * inv,
                    match axis {
                        0 => DVec3::X,
                        1 => DVec3::Y,
                        _ => DVec3::Z,
                    },
                )
            };

            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }
            if t1 > t_min {
                t_min = t1;
                hit_normal = n1;
            }
            if t2 < t_max {
                t_max = t2;
            }
            if t_min > t_max {
                return None;
            }
        }
    }

    if inside {
        // Origin started slightly inside a solid box: push out opposite ray direction.
        return Some((0.0, (-dir).normalize_or_zero()));
    }

    (t_min >= 0.0 && t_min <= max_dist).then_some((t_min, hit_normal))
}

/// 3D Amanatides-Woo DDA voxel raycast with sub-block AABB shape precision.
///
/// Stops immediately if the ray enters an unloaded chunk so webs NEVER anchor
/// across unloaded void boundaries or through walls.
pub fn raycast_blocks(
    world: &dyn WorldQuery,
    origin: DVec3,
    direction: DVec3,
    max_distance: f64,
    camera_only: bool,
) -> Option<RayHit> {
    if !origin.is_finite() || !direction.is_finite() || max_distance <= 0.0 {
        return None;
    }
    let dir = direction.normalize_or_zero();
    if dir.length_squared() < 0.5 {
        return None;
    }

    let mut voxel = IVec3::new(
        origin.x.floor() as i32,
        origin.y.floor() as i32,
        origin.z.floor() as i32,
    );

    let step = IVec3::new(
        if dir.x > 0.0 { 1 } else if dir.x < 0.0 { -1 } else { 0 },
        if dir.y > 0.0 { 1 } else if dir.y < 0.0 { -1 } else { 0 },
        if dir.z > 0.0 { 1 } else if dir.z < 0.0 { -1 } else { 0 },
    );

    let t_delta = DVec3::new(
        if dir.x.abs() > 1e-12 { (1.0 / dir.x).abs() } else { f64::INFINITY },
        if dir.y.abs() > 1e-12 { (1.0 / dir.y).abs() } else { f64::INFINITY },
        if dir.z.abs() > 1e-12 { (1.0 / dir.z).abs() } else { f64::INFINITY },
    );

    let next_boundary = |coord: f64, v: i32, d: f64| -> f64 {
        if d > 0.0 {
            ((v as f64 + 1.0) - coord) / d
        } else if d < 0.0 {
            (coord - v as f64) / (-d)
        } else {
            f64::INFINITY
        }
    };

    let mut t_max = DVec3::new(
        next_boundary(origin.x, voxel.x, dir.x),
        next_boundary(origin.y, voxel.y, dir.y),
        next_boundary(origin.z, voxel.z, dir.z),
    );

    let max_steps = (max_distance.ceil() as usize * 3).clamp(8, 512);
    let mut traveled = 0.0_f64;

    for _ in 0..=max_steps {
        if traveled > max_distance {
            break;
        }

        let sample = world.sample_voxel(voxel);
        if !sample.loaded {
            // Unloaded chunk boundary: refuse to raycast through unloaded space.
            return None;
        }

        let relevant = if camera_only {
            sample.blocks_camera && !sample.fluid
        } else {
            sample.solid && !sample.fluid
        };

        if relevant {
            let base = DVec3::new(voxel.x as f64, voxel.y as f64, voxel.z as f64);
            let mut best_box_hit: Option<(f64, DVec3)> = None;
            for b in sample.effective_boxes() {
                let min = base + DVec3::new(b[0], b[1], b[2]);
                let max = base + DVec3::new(b[3], b[4], b[5]);
                if let Some((t_hit, normal)) =
                    intersect_ray_aabb(origin, dir, min, max, max_distance)
                    && best_box_hit.is_none_or(|(prev_t, _)| t_hit < prev_t)
                {
                    best_box_hit = Some((t_hit, normal));
                }
            }

            if let Some((t_hit, normal)) = best_box_hit {
                return Some(RayHit {
                    position: origin + dir * t_hit,
                    normal,
                    block_pos: voxel,
                    distance: t_hit,
                });
            }
        }

        // Advance to next voxel along smallest t_max.
        if t_max.x < t_max.y {
            if t_max.x < t_max.z {
                traveled = t_max.x;
                voxel.x += step.x;
                t_max.x += t_delta.x;
            } else {
                traveled = t_max.z;
                voxel.z += step.z;
                t_max.z += t_delta.z;
            }
        } else if t_max.y < t_max.z {
            traveled = t_max.y;
            voxel.y += step.y;
            t_max.y += t_delta.y;
        } else {
            traveled = t_max.z;
            voxel.z += step.z;
            t_max.z += t_delta.z;
        }
    }

    None
}

/// Verifies unobstructed line-of-sight between `from` (e.g. player shoulder/wrist)
/// and `anchor_hit` so an off-axis cone ray never grabs behind an intervening wall.
pub fn has_clear_line_of_sight(world: &dyn WorldQuery, from: DVec3, target: &RayHit) -> bool {
    let delta = target.position - from;
    let dist = delta.length();
    if dist < 1e-4 {
        return true;
    }
    let dir = delta / dist;
    // Stop 0.08 blocks short of the target surface so we don't collide with the target face itself.
    let probe_len = (dist - 0.08).max(0.0);
    if probe_len <= 0.05 {
        return true;
    }
    match raycast_blocks(world, from, dir, probe_len, false) {
        None => true,
        Some(obstruction) => obstruction.block_pos == target.block_pos,
    }
}

/// Build an orthonormal basis `(right, up)` around `forward`.
fn orthonormal_basis(forward: DVec3) -> (DVec3, DVec3) {
    let ref_up = if forward.y.abs() > 0.94 {
        DVec3::Z
    } else {
        DVec3::Y
    };
    let right = forward.cross(ref_up).normalize_or_zero();
    let up = right.cross(forward).normalize_or_zero();
    (right, up)
}

/// Insomniac-inspired smart web-swing anchor finder:
/// 1. Casts the exact crosshair ray first.
/// 2. Casts a 3-ring golden-angle cone fan within `cone_degrees` (biased upward
///    when swinging horizontally so skyscrapers, trees, cliffs, and ceiling beams
///    register smoothly at high speed without pixel-hunting).
/// 3. Strictly requires a real loaded solid block surface within `[min_range, max_range]`
///    AND clear line-of-sight from `wrist_origin` — never attaches to air or through walls.
pub fn find_best_swing_anchor(
    world: &dyn WorldQuery,
    eye_origin: DVec3,
    wrist_origin: DVec3,
    aim_dir: DVec3,
    player_feet_y: f64,
    player_velocity: DVec3,
    min_range: f64,
    max_range: f64,
    cone_degrees: f32,
) -> Option<AnchorCandidate> {
    let aim = aim_dir.normalize_or_zero();
    if aim.length_squared() < 0.5 || max_range <= min_range {
        return None;
    }

    let horiz_vel = DVec3::new(player_velocity.x, 0.0, player_velocity.z);
    let horiz_speed = horiz_vel.length();
    let travel_dir = if horiz_speed > 0.12 {
        horiz_vel / horiz_speed
    } else {
        DVec3::new(aim.x, 0.0, aim.z).normalize_or_zero()
    };

    let mut best: Option<AnchorCandidate> = None;

    let mut evaluate_ray = |ray_dir: DVec3, is_direct: bool| {
        let Some(hit) = raycast_blocks(world, eye_origin, ray_dir, max_range, false) else {
            return;
        };
        let wrist_dist = hit.position.distance(wrist_origin);
        if wrist_dist < min_range || wrist_dist > max_range + 1.2 {
            return;
        }
        if !has_clear_line_of_sight(world, wrist_origin, &hit) {
            return;
        }

        // Score components:
        let to_hit = (hit.position - eye_origin).normalize_or_zero();
        let aim_align = to_hit.dot(aim).clamp(-1.0, 1.0);

        // Strong preference for anchors above shoulder height so pendulum swings
        // arc cleanly overhead instead of pulling Spider-Man into the floor.
        let rel_height = hit.position.y - (player_feet_y + 1.4);
        if !is_direct && rel_height < -2.5 {
            return;
        }
        let height_score = if rel_height >= 2.0 {
            (rel_height / 18.0).min(1.15) * 0.42
        } else if rel_height >= -1.0 {
            0.05
        } else {
            -0.55
        };

        // Forward swing arc lead along current horizontal travel direction.
        let horiz_delta = DVec3::new(
            hit.position.x - eye_origin.x,
            0.0,
            hit.position.z - eye_origin.z,
        );
        let forward_proj = if travel_dir.length_squared() > 0.2 {
            horiz_delta.dot(travel_dir)
        } else {
            horiz_delta.length()
        };
        if !is_direct && horiz_speed > 0.35 && forward_proj < -2.0 {
            // Never auto-cone-select an anchor behind a fast-moving player.
            return;
        }
        let lead_score = (forward_proj / 24.0).clamp(-0.35, 0.45);

        // Ideal pendulum lever arm sweet spot around 14..36 blocks.
        let ideal_dist = 24.0;
        let dist_penalty = ((wrist_dist - ideal_dist).abs() / max_range) * 0.24;

        let direct_bonus = if is_direct && rel_height >= -1.5 {
            0.34
        } else if is_direct {
            0.12
        } else {
            0.0
        };

        let score = aim_align * 1.35 + height_score + lead_score - dist_penalty + direct_bonus;
        let candidate = AnchorCandidate {
            hit,
            score,
            direct_aim: is_direct,
        };
        if best.is_none_or(|prev| candidate.score > prev.score) {
            best = Some(candidate);
        }
    };

    // 1. Direct crosshair ray
    evaluate_ray(aim, true);

    // 2. Multi-ring cone fan around aim (with slight upward swing bias when looking near horizon)
    let cone_rad = (cone_degrees.clamp(0.0, 45.0) as f64).to_radians();
    if cone_rad > 1e-3 {
        // Bias cone center slightly upward (+10 deg) when player looks nearly horizontal
        // so swinging forward down a street catches upper building facades effortlessly.
        let upward_bias = ((1.0 - aim.y.abs()).max(0.0)) * 0.18;
        let search_center = (aim + DVec3::Y * upward_bias).normalize_or(aim);
        let (right, up) = orthonormal_basis(search_center);

        // Golden-angle spiral samples across 3 concentric rings (24 rays total: ultra fast < 0.04 ms)
        const GOLDEN_ANGLE: f64 = 2.399_963_229_728_653;
        let sample_count = 24_usize;
        for i in 0..sample_count {
            let frac = ((i as f64) + 0.5) / (sample_count as f64);
            let theta = cone_rad * frac.sqrt();
            let phi = (i as f64) * GOLDEN_ANGLE;
            let offset = right * (phi.cos() * theta.sin()) + up * (phi.sin() * theta.sin());
            let dir = (search_center * theta.cos() + offset).normalize_or_zero();
            if dir.length_squared() > 0.5 {
                evaluate_ray(dir, false);
            }
        }
    }

    best
}

/// Finds a precise Point Launch perch target near the player's crosshair:
/// prefers top ledges, pillar caps, roof edges, and solid perches with standing clearance above.
pub fn find_point_launch_perch(
    world: &dyn WorldQuery,
    eye_origin: DVec3,
    aim_dir: DVec3,
    min_range: f64,
    max_range: f64,
) -> Option<RayHit> {
    let aim = aim_dir.normalize_or_zero();
    if aim.length_squared() < 0.5 {
        return None;
    }

    let (right, up) = orthonormal_basis(aim);
    let cone_rad = 11.0_f64.to_radians();
    let mut best: Option<(f64, RayHit)> = None;

    let mut check_dir = |dir: DVec3, is_center: bool| {
        let Some(hit) = raycast_blocks(world, eye_origin, dir, max_range, false) else {
            return;
        };
        if hit.distance < min_range {
            return;
        }
        // Check if the block or the block 1 above has 2 blocks of headroom to perch on!
        let stand_block = if hit.normal.y > 0.5 {
            hit.block_pos
        } else if !world.is_solid_block(hit.block_pos + IVec3::Y) {
            hit.block_pos
        } else if !world.is_solid_block(hit.block_pos + IVec3::Y * 2) {
            hit.block_pos + IVec3::Y
        } else {
            return;
        };

        let head_1 = stand_block + IVec3::Y;
        let head_2 = stand_block + IVec3::Y * 2;
        if world.is_solid_block(head_1) || world.is_solid_block(head_2) {
            return;
        }

        let perch_pos = DVec3::new(
            stand_block.x as f64 + 0.5,
            stand_block.y as f64 + 1.0,
            stand_block.z as f64 + 0.5,
        );
        let to_perch = (perch_pos - eye_origin).normalize_or_zero();
        let align = to_perch.dot(aim);
        let score = align * 2.0 + if is_center { 0.25 } else { 0.0 } - (hit.distance / max_range) * 0.2;

        let perch_hit = RayHit {
            position: perch_pos,
            normal: DVec3::Y,
            block_pos: stand_block,
            distance: perch_pos.distance(eye_origin),
        };
        if best.is_none_or(|(prev, _)| score > prev) {
            best = Some((score, perch_hit));
        }
    };

    check_dir(aim, true);
    const GOLDEN_ANGLE: f64 = 2.399_963_229_728_653;
    for i in 0..14 {
        let frac = ((i as f64) + 0.5) / 14.0;
        let theta = cone_rad * frac.sqrt();
        let phi = (i as f64) * GOLDEN_ANGLE;
        let dir = (aim * theta.cos()
            + right * (phi.cos() * theta.sin())
            + up * (phi.sin() * theta.sin()))
        .normalize_or_zero();
        check_dir(dir, false);
    }

    best.map(|(_, h)| h)
}
