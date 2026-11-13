use glam::DVec3;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::{
    config::FlyingWebConfig,
    raycast::{WorldQuery, raycast_blocks},
    web::{RenderSegment3D, scale_alpha},
};

/// Network channel used between client and integrated/dedicated server for Spider-Gadgets.
pub const GADGET_NET_CHANNEL: &str = "flyingweb:v1";

/// All 8 Spider-Hero Gadgets adapted non-lethally to Minecraft's sandbox world.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GadgetKind {
    #[default]
    ImpactWeb,
    WebGrabber,
    WebBomb,
    TripMine,
    RemoteMine,
    WebLine,
    WebTrap,
    RicochetWeb,
}

impl GadgetKind {
    pub const ALL: [Self; 8] = [
        Self::ImpactWeb,
        Self::WebGrabber,
        Self::WebBomb,
        Self::TripMine,
        Self::RemoteMine,
        Self::WebLine,
        Self::WebTrap,
        Self::RicochetWeb,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::ImpactWeb => "Impact Web",
            Self::WebGrabber => "Web Grabber",
            Self::WebBomb => "Web Bomb",
            Self::TripMine => "Trip Mine",
            Self::RemoteMine => "Remote Mine",
            Self::WebLine => "Web Line (Tightrope)",
            Self::WebTrap => "Web Trap",
            Self::RicochetWeb => "Ricochet Web",
        }
    }

    pub fn short_tag(self) -> &'static str {
        match self {
            Self::ImpactWeb => "IMP",
            Self::WebGrabber => "GRB",
            Self::WebBomb => "BMB",
            Self::TripMine => "TRP",
            Self::RemoteMine => "RMT",
            Self::WebLine => "LIN",
            Self::WebTrap => "SNR",
            Self::RicochetWeb => "RIC",
        }
    }

    pub fn max_ammo(self) -> u8 {
        match self {
            Self::ImpactWeb => 4,
            Self::WebGrabber => 2,
            Self::WebBomb => 3,
            Self::TripMine => 3,
            Self::RemoteMine => 3,
            Self::WebLine => 4,
            Self::WebTrap => 3,
            Self::RicochetWeb => 4,
        }
    }

    /// Recharge time in 20 Hz ticks for one cartridge pip.
    pub fn recharge_ticks(self) -> u16 {
        match self {
            Self::ImpactWeb => 90,
            Self::WebGrabber => 160,
            Self::WebBomb => 130,
            Self::TripMine => 140,
            Self::RemoteMine => 140,
            Self::WebLine => 80,
            Self::WebTrap => 130,
            Self::RicochetWeb => 95,
        }
    }

    pub fn is_enabled(self, cfg: &FlyingWebConfig) -> bool {
        if !cfg.gadgets_enabled {
            return false;
        }
        match self {
            Self::ImpactWeb => cfg.enable_impact_web,
            Self::WebGrabber => cfg.enable_web_grabber,
            Self::WebBomb => cfg.enable_web_bomb,
            Self::TripMine => cfg.enable_trip_mine,
            Self::RemoteMine => cfg.enable_remote_mine,
            Self::WebLine => cfg.enable_web_line,
            Self::WebTrap => cfg.enable_web_trap,
            Self::RicochetWeb => cfg.enable_ricochet_web,
        }
    }
}

/// Snapshot of a nearby living mob / targetable entity passed into the gadget simulator.
#[derive(Clone, Debug)]
pub struct TargetEntitySnapshot {
    pub id: u32,
    pub position: DVec3,
    pub height: f64,
    pub is_player: bool,
    pub can_restrain: bool,
}

/// Authoritative server-side velocity impulse or soft hold requested by a gadget tick.
#[derive(Clone, Copy, Debug)]
pub struct MobImpulseCommand {
    pub entity_id: u32,
    /// Desired additive velocity push `[vx, vy, vz]` via `native_push_mob`.
    pub delta_velocity: [f32; 3],
}

/// Wire packet sent client -> server on `GADGET_NET_CHANNEL`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum GadgetNetRequest {
    Fire {
        kind: GadgetKind,
        origin: [f64; 3],
        direction: [f64; 3],
    },
    DetonateRemoteMines,
}

/// Wire packet sent server -> client on `GADGET_NET_CHANNEL` to sync webbed mob visuals.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GadgetSyncEntry {
    pub entity_id: u32,
    pub anchor: Option<[f64; 3]>,
    pub ticks_left: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum GadgetNetBroadcast {
    WebbedEntities(Vec<GadgetSyncEntry>),
}

/// Active flying gadget projectile.
#[derive(Clone, Debug)]
pub struct GadgetProjectile {
    pub kind: GadgetKind,
    pub prev_pos: DVec3,
    pub pos: DVec3,
    pub vel: DVec3,
    pub gravity: f64,
    pub bounces_left: u8,
    pub age_ticks: u16,
    pub hit_ids: Vec<u32>,
}

/// Deployed world gadget device (Trip Mine, Remote Mine, Web Grabber vortex, or Web Trap).
#[derive(Clone, Debug)]
pub enum DeployedGadget {
    WebGrabberCore {
        pos: DVec3,
        ticks_left: u16,
    },
    TripMine {
        pos: DVec3,
        normal: DVec3,
        beam_len: f64,
        ticks_left: u16,
    },
    RemoteMine {
        pos: DVec3,
        normal: DVec3,
        ticks_left: u16,
    },
    WebTrap {
        pos: DVec3,
        normal: DVec3,
        arm_delay: u8,
        ticks_left: u16,
    },
}

/// Structural horizontal tightrope cast by the `WebLine` gadget that Spider-Man can walk & perch on!
#[derive(Clone, Copy, Debug)]
pub struct TraversableWebLine {
    pub start: DVec3,
    pub end: DVec3,
    pub ticks_left: u32,
}

/// Non-lethal restraint state applied to a webbed/pinned mob.
#[derive(Clone, Copy, Debug)]
pub struct WebbedEntityState {
    pub entity_id: u32,
    /// Optional wall/floor/focal anchor to which this mob is tethered.
    pub pin_anchor: Option<DVec3>,
    pub web_ticks_left: u16,
    /// Extra post-restraint ticks during which lethal fall damage remains safely cushioned.
    pub fall_arrest_grace_ticks: u16,
}

/// Audio/VFX events produced by gadget actions.
#[derive(Clone, Copy, Debug)]
pub enum GadgetEvent {
    Fired { kind: GadgetKind, at: DVec3 },
    Burst { kind: GadgetKind, at: DVec3 },
    RicochetBounce { at: DVec3 },
    EntityWebbed { entity_id: u32, at: DVec3 },
    WebLineAnchored { start: DVec3, end: DVec3 },
}

/// Per-gadget ammo + recharge state and active simulated projectiles, mines, tightropes, and restraints.
#[derive(Clone, Debug)]
pub struct GadgetManager {
    pub selected: GadgetKind,
    ammo: [u8; 8],
    recharge_progress: [u16; 8],

    pub projectiles: Vec<GadgetProjectile>,
    pub deployed: Vec<DeployedGadget>,
    pub web_lines: Vec<TraversableWebLine>,
    pub webbed_entities: FxHashMap<u32, WebbedEntityState>,
}

impl Default for GadgetManager {
    fn default() -> Self {
        let mut ammo = [0_u8; 8];
        for (i, g) in GadgetKind::ALL.iter().enumerate() {
            ammo[i] = g.max_ammo();
        }
        Self {
            selected: GadgetKind::ImpactWeb,
            ammo,
            recharge_progress: [0; 8],
            projectiles: Vec::new(),
            deployed: Vec::new(),
            web_lines: Vec::new(),
            webbed_entities: FxHashMap::default(),
        }
    }
}

impl GadgetManager {
    fn idx(kind: GadgetKind) -> usize {
        match kind {
            GadgetKind::ImpactWeb => 0,
            GadgetKind::WebGrabber => 1,
            GadgetKind::WebBomb => 2,
            GadgetKind::TripMine => 3,
            GadgetKind::RemoteMine => 4,
            GadgetKind::WebLine => 5,
            GadgetKind::WebTrap => 6,
            GadgetKind::RicochetWeb => 7,
        }
    }

    pub fn reset_world_state(&mut self) {
        self.projectiles.clear();
        self.deployed.clear();
        self.web_lines.clear();
        self.webbed_entities.clear();
        for (i, g) in GadgetKind::ALL.iter().enumerate() {
            self.ammo[i] = g.max_ammo();
            self.recharge_progress[i] = 0;
        }
    }

    pub fn ammo_of(&self, kind: GadgetKind) -> (u8, u8, f32) {
        let i = Self::idx(kind);
        let max = kind.max_ammo();
        let cur = self.ammo[i];
        let frac = if cur >= max {
            1.0
        } else {
            (self.recharge_progress[i] as f32) / (kind.recharge_ticks().max(1) as f32)
        };
        (cur, max, frac.clamp(0.0, 1.0))
    }

    pub fn cycle_selected(&mut self, delta: i32, cfg: &FlyingWebConfig) {
        if !cfg.gadgets_enabled {
            return;
        }
        let cur_idx = GadgetKind::ALL
            .iter()
            .position(|&g| g == self.selected)
            .unwrap_or(0) as i32;
        let len = GadgetKind::ALL.len() as i32;
        let step = if delta >= 0 { 1 } else { -1 };
        for offset in 1..=len {
            let next_i = (cur_idx + step * offset).rem_euclid(len) as usize;
            let cand = GadgetKind::ALL[next_i];
            if cand.is_enabled(cfg) {
                self.selected = cand;
                return;
            }
        }
    }

    pub fn has_armed_remote_mines(&self) -> bool {
        self.deployed
            .iter()
            .any(|d| matches!(d, DeployedGadget::RemoteMine { .. }))
    }

    /// Fires or triggers `kind` from `origin` along `direction`.
    /// Returns `Some(GadgetNetRequest)` when the action succeeded so the client can sync with the server.
    pub fn try_use_gadget(
        &mut self,
        world: &dyn WorldQuery,
        kind: GadgetKind,
        origin: DVec3,
        direction: DVec3,
        sneak_detonate: bool,
        cfg: &FlyingWebConfig,
        events: &mut Vec<GadgetEvent>,
    ) -> Option<GadgetNetRequest> {
        if !cfg.abilities_active() || !kind.is_enabled(cfg) {
            return None;
        }

        // If Remote Mine is selected and player sneaks (or has max 3 mines placed), detonate armed mines!
        let active_remote_count = self
            .deployed
            .iter()
            .filter(|d| matches!(d, DeployedGadget::RemoteMine { .. }))
            .count();
        if kind == GadgetKind::RemoteMine
            && active_remote_count > 0
            && (sneak_detonate || active_remote_count >= 3 || self.ammo[Self::idx(kind)] == 0)
        {
            return Some(GadgetNetRequest::DetonateRemoteMines);
        }

        let i = Self::idx(kind);
        if self.ammo[i] == 0 {
            return None;
        }

        let dir = direction.normalize_or_zero();
        if dir.length_squared() < 0.5 {
            return None;
        }

        // WebLine casts immediately between two opposing solid surfaces!
        if kind == GadgetKind::WebLine {
            if self.spawn_web_line(world, origin, dir, events) {
                self.ammo[i] -= 1;
                return Some(GadgetNetRequest::Fire {
                    kind,
                    origin: origin.to_array(),
                    direction: dir.to_array(),
                });
            }
            return None;
        }

        self.ammo[i] -= 1;
        self.spawn_projectile(kind, origin, dir, events);
        Some(GadgetNetRequest::Fire {
            kind,
            origin: origin.to_array(),
            direction: dir.to_array(),
        })
    }

    /// Server/simulation entrypoint for a network-received gadget command.
    pub fn execute_authoritative_request(
        &mut self,
        world: &dyn WorldQuery,
        req: GadgetNetRequest,
        targets: &[TargetEntitySnapshot],
        impulses: &mut Vec<MobImpulseCommand>,
        events: &mut Vec<GadgetEvent>,
    ) {
        match req {
            GadgetNetRequest::Fire {
                kind,
                origin,
                direction,
            } => {
                let orig = DVec3::from_array(origin);
                let dir = DVec3::from_array(direction).normalize_or_zero();
                if !orig.is_finite() || dir.length_squared() < 0.5 {
                    return;
                }
                if kind == GadgetKind::WebLine {
                    let _ = self.spawn_web_line(world, orig, dir, events);
                } else {
                    self.spawn_projectile(kind, orig, dir, events);
                }
            }
            GadgetNetRequest::DetonateRemoteMines => {
                self.detonate_remote_mines(targets, impulses, events);
            }
        }
    }

    pub fn spawn_web_line(
        &mut self,
        world: &dyn WorldQuery,
        origin: DVec3,
        dir: DVec3,
        events: &mut Vec<GadgetEvent>,
    ) -> bool {
        // Raycast forward to target wall/pillar (up to 32 blocks)
        let Some(front_hit) = raycast_blocks(world, origin, dir, 32.0, false) else {
            return false;
        };
        // Cast horizontal opposing ray from `front_hit` back across the gap (up to 36 blocks)
        let horiz_back = DVec3::new(-dir.x, 0.0, -dir.z).normalize_or(front_hit.normal);
        let probe_start = front_hit.position + front_hit.normal * 0.15;
        let back_anchor = if let Some(back_hit) = raycast_blocks(world, probe_start, horiz_back, 36.0, false) {
            back_hit.position
        } else if let Some(floor_back) = raycast_blocks(world, origin, DVec3::NEG_Y, 3.0, false) {
            floor_back.position + DVec3::Y * 1.1
        } else {
            return false;
        };

        if front_hit.position.distance(back_anchor) < 2.2 {
            return false;
        }

        while self.web_lines.len() >= 6 {
            self.web_lines.remove(0);
        }
        self.web_lines.push(TraversableWebLine {
            start: back_anchor,
            end: front_hit.position,
            ticks_left: 20 * 90, // 90 seconds persistent structural tightrope!
        });
        events.push(GadgetEvent::WebLineAnchored {
            start: back_anchor,
            end: front_hit.position,
        });
        true
    }

    fn spawn_projectile(
        &mut self,
        kind: GadgetKind,
        origin: DVec3,
        dir: DVec3,
        events: &mut Vec<GadgetEvent>,
    ) {
        let (speed, gravity, bounces) = match kind {
            GadgetKind::ImpactWeb => (3.1, 0.012, 0),
            GadgetKind::WebGrabber => (1.55, 0.045, 0),
            GadgetKind::WebBomb => (1.65, 0.048, 0),
            GadgetKind::TripMine => (2.40, 0.018, 0),
            GadgetKind::RemoteMine => (1.85, 0.040, 0),
            GadgetKind::WebLine => (3.0, 0.0, 0),
            GadgetKind::WebTrap => (1.50, 0.050, 0),
            GadgetKind::RicochetWeb => (2.85, 0.008, 4),
        };
        while self.projectiles.len() >= 16 {
            self.projectiles.remove(0);
        }
        self.projectiles.push(GadgetProjectile {
            kind,
            prev_pos: origin,
            pos: origin,
            vel: dir * speed,
            gravity,
            bounces_left: bounces,
            age_ticks: 0,
            hit_ids: Vec::new(),
        });
        events.push(GadgetEvent::Fired { kind, at: origin });
    }

    pub fn detonate_remote_mines(
        &mut self,
        targets: &[TargetEntitySnapshot],
        impulses: &mut Vec<MobImpulseCommand>,
        events: &mut Vec<GadgetEvent>,
    ) {
        let mut detonated_positions = Vec::new();
        self.deployed.retain(|d| {
            if let DeployedGadget::RemoteMine { pos, .. } = *d {
                detonated_positions.push(pos);
                false
            } else {
                true
            }
        });

        for center in detonated_positions {
            events.push(GadgetEvent::Burst {
                kind: GadgetKind::RemoteMine,
                at: center,
            });
            for mob in targets {
                if !mob.can_restrain || mob.is_player {
                    continue;
                }
                let mob_center = mob.position + DVec3::Y * (mob.height * 0.5);
                let delta = mob_center - center;
                let dist = delta.length();
                if dist <= 6.2 {
                    let push_dir = (delta + DVec3::Y * 0.45).normalize_or(DVec3::Y);
                    let strength = ((1.0 - dist / 6.5) * 0.85).clamp(0.22, 0.85) as f32;
                    impulses.push(MobImpulseCommand {
                        entity_id: mob.id,
                        delta_velocity: [
                            push_dir.x as f32 * strength,
                            (push_dir.y as f32 * strength).clamp(0.15, 0.52),
                            push_dir.z as f32 * strength,
                        ],
                    });
                    self.apply_web_restraint(mob.id, None, 85);
                    events.push(GadgetEvent::EntityWebbed {
                        entity_id: mob.id,
                        at: mob_center,
                    });
                }
            }
        }
    }

    /// Checks whether the player's feet should perch/walk atop an active `WebLine` tightrope!
    /// Returns `Some((snapped_y, adjusted_velocity))` when walking on a tightrope.
    pub fn evaluate_tightrope_support(
        &self,
        player_feet: DVec3,
        velocity: DVec3,
        sneak_drop: bool,
    ) -> Option<(f64, DVec3)> {
        if sneak_drop || velocity.y > 0.22 {
            return None;
        }
        for line in &self.web_lines {
            let seg = line.end - line.start;
            let len_sq = seg.length_squared();
            if len_sq < 0.25 {
                continue;
            }
            let t = ((player_feet - line.start).dot(seg) / len_sq).clamp(0.0, 1.0);
            let closest = line.start + seg * t;
            let horiz_dist = DVec3::new(
                player_feet.x - closest.x,
                0.0,
                player_feet.z - closest.z,
            )
            .length();
            let dy = player_feet.y - closest.y;
            if horiz_dist <= 0.44 && (-0.38..=0.45).contains(&dy) {
                // Smoothly pull feet laterally toward center of tightrope and support Y!
                let lateral_corr = DVec3::new(
                    (closest.x - player_feet.x) * 0.28,
                    (closest.y - player_feet.y).clamp(-0.25, 0.25),
                    (closest.z - player_feet.z) * 0.28,
                );
                let supported_vel = DVec3::new(
                    velocity.x * 0.86 + lateral_corr.x,
                    lateral_corr.y.max(0.0),
                    velocity.z * 0.86 + lateral_corr.z,
                );
                return Some((closest.y, supported_vel));
            }
        }
        None
    }

    /// Advances ammo recharge, projectiles, deployed mines/traps/grabbers, tightropes,
    /// and non-lethal mob web restraints by one 20 Hz tick.
    pub fn tick(
        &mut self,
        world: &dyn WorldQuery,
        targets: &[TargetEntitySnapshot],
        impulses: &mut Vec<MobImpulseCommand>,
        events: &mut Vec<GadgetEvent>,
    ) {
        // 1. Recharge gadget cartridges smoothly
        for (i, kind) in GadgetKind::ALL.iter().copied().enumerate() {
            let max = kind.max_ammo();
            if self.ammo[i] < max {
                self.recharge_progress[i] = self.recharge_progress[i].saturating_add(1);
                if self.recharge_progress[i] >= kind.recharge_ticks() {
                    self.recharge_progress[i] = 0;
                    self.ammo[i] += 1;
                }
            } else {
                self.recharge_progress[i] = 0;
            }
        }

        // 2. Step active gadget projectiles
        let mut next_projectiles = Vec::with_capacity(self.projectiles.len());
        let old_projectiles = std::mem::take(&mut self.projectiles);
        for mut proj in old_projectiles {
            proj.age_ticks = proj.age_ticks.saturating_add(1);
            if proj.age_ticks > 80 {
                continue;
            }
            proj.prev_pos = proj.pos;
            proj.vel.y -= proj.gravity;
            let step_len = proj.vel.length();
            if step_len < 1e-5 {
                continue;
            }
            let dir = proj.vel / step_len;

            // Check entity hit along segment first
            let mut hit_mob: Option<&TargetEntitySnapshot> = None;
            let mut best_mob_dist = step_len + 0.55;
            for mob in targets {
                if !mob.can_restrain || mob.is_player || proj.hit_ids.contains(&mob.id) {
                    continue;
                }
                let mob_mid = mob.position + DVec3::Y * (mob.height * 0.5);
                let to_mob = mob_mid - proj.pos;
                let along = to_mob.dot(dir).clamp(0.0, step_len + 0.45);
                let closest = proj.pos + dir * along;
                if closest.distance(mob_mid) <= 0.85 && along < best_mob_dist {
                    best_mob_dist = along;
                    hit_mob = Some(mob);
                }
            }

            let block_hit = raycast_blocks(world, proj.pos, dir, step_len, false);
            let block_hit_first = block_hit
                .as_ref()
                .is_some_and(|bh| bh.distance < best_mob_dist);

            if !block_hit_first && let Some(mob) = hit_mob {
                let impact_pt = mob.position + DVec3::Y * (mob.height * 0.5);
                match proj.kind {
                    GadgetKind::ImpactWeb => {
                        // Knock back & pin to nearest wall/floor behind mob!
                        let pin_anchor = raycast_blocks(world, impact_pt, DVec3::new(dir.x, -0.25, dir.z).normalize_or(dir), 5.5, false)
                            .map(|h| h.position)
                            .or_else(|| raycast_blocks(world, impact_pt, DVec3::NEG_Y, 4.0, false).map(|h| h.position));
                        impulses.push(MobImpulseCommand {
                            entity_id: mob.id,
                            delta_velocity: [
                                dir.x as f32 * 0.92,
                                0.24,
                                dir.z as f32 * 0.92,
                            ],
                        });
                        self.apply_web_restraint(mob.id, pin_anchor, 105);
                        events.push(GadgetEvent::EntityWebbed {
                            entity_id: mob.id,
                            at: impact_pt,
                        });
                        continue;
                    }
                    GadgetKind::RicochetWeb => {
                        proj.hit_ids.push(mob.id);
                        self.apply_web_restraint(mob.id, None, 85);
                        impulses.push(MobImpulseCommand {
                            entity_id: mob.id,
                            delta_velocity: [dir.x as f32 * 0.35, 0.16, dir.z as f32 * 0.35],
                        });
                        events.push(GadgetEvent::EntityWebbed {
                            entity_id: mob.id,
                            at: impact_pt,
                        });
                        if proj.bounces_left > 0 {
                            proj.bounces_left -= 1;
                            proj.pos = impact_pt;
                            // Chain toward next nearest unhit mob within 11 blocks!
                            if let Some(next_target) = targets
                                .iter()
                                .filter(|m| m.can_restrain && !m.is_player && !proj.hit_ids.contains(&m.id))
                                .min_by(|a, b| {
                                    a.position
                                        .distance(impact_pt)
                                        .total_cmp(&b.position.distance(impact_pt))
                                })
                                && next_target.position.distance(impact_pt) <= 11.0
                            {
                                let next_dir = ((next_target.position + DVec3::Y * 0.8) - impact_pt)
                                    .normalize_or(dir);
                                proj.vel = next_dir * step_len;
                                next_projectiles.push(proj);
                            }
                        }
                        continue;
                    }
                    GadgetKind::WebBomb => {
                        self.trigger_web_bomb_burst(impact_pt, targets, impulses, events);
                        continue;
                    }
                    GadgetKind::WebGrabber => {
                        self.spawn_deployed(DeployedGadget::WebGrabberCore {
                            pos: impact_pt,
                            ticks_left: 70,
                        });
                        events.push(GadgetEvent::Burst {
                            kind: GadgetKind::WebGrabber,
                            at: impact_pt,
                        });
                        continue;
                    }
                    GadgetKind::TripMine | GadgetKind::WebTrap => {
                        self.apply_web_restraint(mob.id, Some(mob.position), 100);
                        events.push(GadgetEvent::EntityWebbed {
                            entity_id: mob.id,
                            at: impact_pt,
                        });
                        continue;
                    }
                    GadgetKind::RemoteMine => {
                        self.spawn_deployed(DeployedGadget::RemoteMine {
                            pos: impact_pt,
                            normal: DVec3::Y,
                            ticks_left: 20 * 30,
                        });
                        continue;
                    }
                    GadgetKind::WebLine => continue,
                }
            }

            if let Some(hit) = block_hit {
                let surface_pt = hit.position + hit.normal * 0.06;
                match proj.kind {
                    GadgetKind::RicochetWeb if proj.bounces_left > 0 => {
                        proj.bounces_left -= 1;
                        proj.pos = surface_pt;
                        let reflected = (dir - hit.normal * (2.0 * dir.dot(hit.normal))).normalize_or(-dir);
                        // Subtle smart assist steering reflected web toward nearby mob within 28 deg cone!
                        let steered = targets
                            .iter()
                            .filter(|m| m.can_restrain && !m.is_player && !proj.hit_ids.contains(&m.id))
                            .filter_map(|m| {
                                let to_m = ((m.position + DVec3::Y * 0.8) - surface_pt).normalize_or_zero();
                                let align = to_m.dot(reflected);
                                (align > 0.86 && m.position.distance(surface_pt) < 13.0)
                                    .then_some((align, to_m))
                            })
                            .max_by(|a, b| a.0.total_cmp(&b.0))
                            .map(|(_, d)| d)
                            .unwrap_or(reflected);
                        proj.vel = steered * (step_len * 0.96);
                        events.push(GadgetEvent::RicochetBounce { at: surface_pt });
                        next_projectiles.push(proj);
                    }
                    GadgetKind::WebBomb => {
                        self.trigger_web_bomb_burst(surface_pt, targets, impulses, events);
                    }
                    GadgetKind::WebGrabber => {
                        self.spawn_deployed(DeployedGadget::WebGrabberCore {
                            pos: surface_pt,
                            ticks_left: 72,
                        });
                        events.push(GadgetEvent::Burst {
                            kind: GadgetKind::WebGrabber,
                            at: surface_pt,
                        });
                    }
                    GadgetKind::TripMine => {
                        self.spawn_deployed(DeployedGadget::TripMine {
                            pos: surface_pt,
                            normal: hit.normal,
                            beam_len: 7.0,
                            ticks_left: 20 * 45,
                        });
                    }
                    GadgetKind::RemoteMine => {
                        self.spawn_deployed(DeployedGadget::RemoteMine {
                            pos: surface_pt,
                            normal: hit.normal,
                            ticks_left: 20 * 60,
                        });
                    }
                    GadgetKind::WebTrap => {
                        self.spawn_deployed(DeployedGadget::WebTrap {
                            pos: surface_pt,
                            normal: hit.normal,
                            arm_delay: 8,
                            ticks_left: 20 * 50,
                        });
                    }
                    _ => {}
                }
                continue;
            }

            proj.pos += proj.vel;
            next_projectiles.push(proj);
        }
        self.projectiles = next_projectiles;

        // 3. Step deployed world devices (Web Grabber vortex, Trip Mine laser, Remote Mine, Web Trap)
        let mut kept_deployed = Vec::with_capacity(self.deployed.len());
        let old_deployed = std::mem::take(&mut self.deployed);
        for dev in old_deployed {
            match dev {
                DeployedGadget::WebGrabberCore { pos, ticks_left } => {
                    if ticks_left <= 1 {
                        continue;
                    }
                    // Pull up to 6 nearby mobs smoothly toward vortex focal point without slam-killing them!
                    let mut pulled = 0_usize;
                    for mob in targets {
                        if !mob.can_restrain || mob.is_player || pulled >= 6 {
                            continue;
                        }
                        let delta = pos - mob.position;
                        let dist = delta.length();
                        if (0.7..=8.5).contains(&dist) {
                            let pull = (delta / dist) * (dist * 0.085).clamp(0.10, 0.42);
                            impulses.push(MobImpulseCommand {
                                entity_id: mob.id,
                                delta_velocity: [
                                    pull.x as f32,
                                    (pull.y as f32 + 0.04).clamp(-0.15, 0.24),
                                    pull.z as f32,
                                ],
                            });
                            self.apply_web_restraint(mob.id, Some(pos), 35);
                            pulled += 1;
                        }
                    }
                    kept_deployed.push(DeployedGadget::WebGrabberCore {
                        pos,
                        ticks_left: ticks_left - 1,
                    });
                }

                DeployedGadget::TripMine {
                    pos,
                    normal,
                    beam_len,
                    ticks_left,
                } => {
                    if ticks_left <= 1 {
                        continue;
                    }
                    let actual_len = raycast_blocks(world, pos + normal * 0.1, normal, beam_len, false)
                        .map(|h| h.distance + 0.1)
                        .unwrap_or(beam_len);
                    let mut triggered = false;
                    for mob in targets {
                        if !mob.can_restrain || mob.is_player {
                            continue;
                        }
                        let mob_mid = mob.position + DVec3::Y * (mob.height * 0.5);
                        let along = (mob_mid - pos).dot(normal).clamp(0.0, actual_len);
                        let closest = pos + normal * along;
                        if closest.distance(mob_mid) <= 0.85 {
                            // Snap mob back onto Trip Mine wall anchor!
                            let pull = (pos - mob_mid).normalize_or(-normal) * 0.78;
                            impulses.push(MobImpulseCommand {
                                entity_id: mob.id,
                                delta_velocity: [
                                    pull.x as f32,
                                    (pull.y as f32 + 0.12).clamp(-0.35, 0.45),
                                    pull.z as f32,
                                ],
                            });
                            self.apply_web_restraint(mob.id, Some(pos), 115);
                            events.push(GadgetEvent::Burst {
                                kind: GadgetKind::TripMine,
                                at: mob_mid,
                            });
                            events.push(GadgetEvent::EntityWebbed {
                                entity_id: mob.id,
                                at: mob_mid,
                            });
                            triggered = true;
                            break;
                        }
                    }
                    if !triggered {
                        kept_deployed.push(DeployedGadget::TripMine {
                            pos,
                            normal,
                            beam_len: actual_len,
                            ticks_left: ticks_left - 1,
                        });
                    }
                }

                DeployedGadget::RemoteMine {
                    pos,
                    normal,
                    ticks_left,
                } => {
                    if ticks_left > 1 {
                        kept_deployed.push(DeployedGadget::RemoteMine {
                            pos,
                            normal,
                            ticks_left: ticks_left - 1,
                        });
                    }
                }

                DeployedGadget::WebTrap {
                    pos,
                    normal,
                    arm_delay,
                    ticks_left,
                } => {
                    if ticks_left <= 1 {
                        continue;
                    }
                    if arm_delay > 0 {
                        kept_deployed.push(DeployedGadget::WebTrap {
                            pos,
                            normal,
                            arm_delay: arm_delay - 1,
                            ticks_left: ticks_left - 1,
                        });
                        continue;
                    }
                    let mut sprung = false;
                    for mob in targets {
                        if !mob.can_restrain || mob.is_player {
                            continue;
                        }
                        if mob.position.distance(pos) <= 2.75 {
                            self.apply_web_restraint(mob.id, Some(pos), 110);
                            events.push(GadgetEvent::Burst {
                                kind: GadgetKind::WebTrap,
                                at: pos,
                            });
                            events.push(GadgetEvent::EntityWebbed {
                                entity_id: mob.id,
                                at: mob.position,
                            });
                            sprung = true;
                        }
                    }
                    if !sprung {
                        kept_deployed.push(DeployedGadget::WebTrap {
                            pos,
                            normal,
                            arm_delay: 0,
                            ticks_left: ticks_left - 1,
                        });
                    }
                }
            }
        }
        self.deployed = kept_deployed;

        // 4. Decay WebLines
        for line in &mut self.web_lines {
            line.ticks_left = line.ticks_left.saturating_sub(1);
        }
        self.web_lines.retain(|l| l.ticks_left > 0);

        // 5. Hold webbed/pinned mobs gently in place & maintain non-lethal fall-arrest grace timers!
        for state in self.webbed_entities.values_mut() {
            if state.web_ticks_left > 0 {
                state.web_ticks_left -= 1;
                if let Some(mob) = targets.iter().find(|m| m.id == state.entity_id) {
                    if let Some(pin) = state.pin_anchor {
                        let delta = pin - mob.position;
                        let dist = delta.length();
                        if dist > 0.65 && dist < 9.0 {
                            let tether_pull = (delta / dist) * (dist * 0.12).clamp(0.05, 0.30);
                            impulses.push(MobImpulseCommand {
                                entity_id: mob.id,
                                delta_velocity: [
                                    tether_pull.x as f32,
                                    (tether_pull.y as f32 + 0.055).clamp(-0.08, 0.22),
                                    tether_pull.z as f32,
                                ],
                            });
                        }
                    }
                }
            } else if state.fall_arrest_grace_ticks > 0 {
                state.fall_arrest_grace_ticks -= 1;
            }
        }
        self.webbed_entities
            .retain(|_, s| s.web_ticks_left > 0 || s.fall_arrest_grace_ticks > 0);
    }

    fn trigger_web_bomb_burst(
        &mut self,
        center: DVec3,
        targets: &[TargetEntitySnapshot],
        impulses: &mut Vec<MobImpulseCommand>,
        events: &mut Vec<GadgetEvent>,
    ) {
        events.push(GadgetEvent::Burst {
            kind: GadgetKind::WebBomb,
            at: center,
        });
        for mob in targets {
            if !mob.can_restrain || mob.is_player {
                continue;
            }
            let dist = mob.position.distance(center);
            if dist <= 5.6 {
                let floor_anchor = Some(mob.position);
                impulses.push(MobImpulseCommand {
                    entity_id: mob.id,
                    delta_velocity: [0.0, 0.08, 0.0],
                });
                self.apply_web_restraint(mob.id, floor_anchor, 95);
                events.push(GadgetEvent::EntityWebbed {
                    entity_id: mob.id,
                    at: mob.position + DVec3::Y * 0.8,
                });
            }
        }
    }

    pub fn apply_web_restraint(
        &mut self,
        entity_id: u32,
        pin_anchor: Option<DVec3>,
        duration_ticks: u16,
    ) {
        let entry = self
            .webbed_entities
            .entry(entity_id)
            .or_insert(WebbedEntityState {
                entity_id,
                pin_anchor,
                web_ticks_left: 0,
                fall_arrest_grace_ticks: 80,
            });
        entry.web_ticks_left = entry.web_ticks_left.max(duration_ticks);
        entry.fall_arrest_grace_ticks = 80;
        if pin_anchor.is_some() {
            entry.pin_anchor = pin_anchor;
        }
    }

    pub fn is_entity_visually_webbed(&self, entity_id: u32) -> bool {
        self.webbed_entities
            .get(&entity_id)
            .is_some_and(|s| s.web_ticks_left > 0)
    }

    /// True if incoming environmental/fall damage on `entity_id` should be negated
    /// to preserve Spider-Man's non-lethal gadget behavior.
    pub fn should_protect_entity_from_fall(&self, entity_id: u32, damage_type: &str) -> bool {
        let is_fall_like = damage_type.contains("fall")
            || damage_type.contains("fly_into_wall")
            || damage_type.contains("stalagmite");
        is_fall_like && self.webbed_entities.contains_key(&entity_id)
    }

    pub fn export_sync_entries(&self) -> Vec<GadgetSyncEntry> {
        self.webbed_entities
            .values()
            .filter(|s| s.web_ticks_left > 0)
            .map(|s| GadgetSyncEntry {
                entity_id: s.entity_id,
                anchor: s.pin_anchor.map(|v| v.to_array()),
                ticks_left: s.web_ticks_left,
            })
            .collect()
    }

    pub fn apply_sync_entries(&mut self, entries: Vec<GadgetSyncEntry>) {
        self.webbed_entities.clear();
        for e in entries {
            self.webbed_entities.insert(
                e.entity_id,
                WebbedEntityState {
                    entity_id: e.entity_id,
                    pin_anchor: e.anchor.map(DVec3::from_array),
                    web_ticks_left: e.ticks_left,
                    fall_arrest_grace_ticks: 40,
                },
            );
        }
    }

    /// Emits 3D lines for flying gadget bolts, Trip Mine lasers, Remote Mine beacons,
    /// Web Grabber radial tethers, traversable WebLines, and pinned-mob web strands!
    pub fn build_render_segments(
        &self,
        partial_tick: f32,
        web_color_argb: u32,
        targets: &[TargetEntitySnapshot],
        out: &mut Vec<RenderSegment3D>,
    ) {
        let t = partial_tick.clamp(0.0, 1.0) as f64;

        // 1. Flying gadget bolts
        for p in &self.projectiles {
            let cur = p.prev_pos.lerp(p.pos, t);
            let tail = cur - p.vel.normalize_or(DVec3::Z) * 0.55;
            let color = match p.kind {
                GadgetKind::TripMine => 0xEE48_D8FF,
                GadgetKind::RemoteMine => 0xEEFF_A838,
                GadgetKind::RicochetWeb => 0xF5B8_E8FF,
                _ => web_color_argb,
            };
            out.push(RenderSegment3D {
                from: tail.to_array(),
                to: cur.to_array(),
                color_argb: color,
                width: 3.0,
            });
        }

        // 2. Deployed gadgets
        for d in &self.deployed {
            match *d {
                DeployedGadget::WebGrabberCore { pos, ticks_left } => {
                    let pulse = ((ticks_left as f64 * 0.45).sin() * 0.08 + 0.22).max(0.12);
                    out.push(RenderSegment3D {
                        from: (pos - DVec3::Y * pulse).to_array(),
                        to: (pos + DVec3::Y * pulse).to_array(),
                        color_argb: 0xEE66_E0FF,
                        width: 4.0,
                    });
                    for mob in targets {
                        if !mob.is_player && mob.position.distance(pos) <= 8.5 {
                            out.push(RenderSegment3D {
                                from: pos.to_array(),
                                to: (mob.position + DVec3::Y * (mob.height * 0.5)).to_array(),
                                color_argb: scale_alpha(web_color_argb, 0.85),
                                width: 2.0,
                            });
                        }
                    }
                }
                DeployedGadget::TripMine {
                    pos,
                    normal,
                    beam_len,
                    ..
                } => {
                    let beam_end = pos + normal * beam_len;
                    out.push(RenderSegment3D {
                        from: pos.to_array(),
                        to: (pos + normal * 0.14).to_array(),
                        color_argb: 0xFF28_3448,
                        width: 4.5,
                    });
                    out.push(RenderSegment3D {
                        from: (pos + normal * 0.14).to_array(),
                        to: beam_end.to_array(),
                        color_argb: 0xB838_E2FF,
                        width: 1.6,
                    });
                }
                DeployedGadget::RemoteMine {
                    pos,
                    normal,
                    ticks_left,
                } => {
                    let blink = if (ticks_left / 6) % 2 == 0 {
                        0xFFFF_9C28
                    } else {
                        0xCCFF_4422
                    };
                    out.push(RenderSegment3D {
                        from: pos.to_array(),
                        to: (pos + normal * 0.16).to_array(),
                        color_argb: blink,
                        width: 4.8,
                    });
                }
                DeployedGadget::WebTrap { pos, normal, .. } => {
                    let center = pos + normal * 0.04;
                    let ref_u = if normal.y.abs() > 0.9 { DVec3::X } else { DVec3::Y };
                    let t1 = normal.cross(ref_u).normalize_or(DVec3::X);
                    let t2 = normal.cross(t1).normalize_or(DVec3::Z);
                    for i in 0..6 {
                        let a0 = (i as f64) * (std::f64::consts::TAU / 6.0);
                        let a1 = ((i + 1) as f64) * (std::f64::consts::TAU / 6.0);
                        let p0 = center + (t1 * a0.cos() + t2 * a0.sin()) * 0.42;
                        let p1 = center + (t1 * a1.cos() + t2 * a1.sin()) * 0.42;
                        out.push(RenderSegment3D {
                            from: p0.to_array(),
                            to: p1.to_array(),
                            color_argb: 0xCC98_ECFF,
                            width: 1.8,
                        });
                    }
                }
            }
        }

        // 3. Structural traversable WebLines (double braided silk cable!)
        for line in &self.web_lines {
            out.push(RenderSegment3D {
                from: line.start.to_array(),
                to: line.end.to_array(),
                color_argb: scale_alpha(web_color_argb, 0.45),
                width: 4.4,
            });
            out.push(RenderSegment3D {
                from: line.start.to_array(),
                to: line.end.to_array(),
                color_argb: web_color_argb,
                width: 2.5,
            });
        }

        // 4. Pinned-mob silk tethers
        for state in self.webbed_entities.values() {
            if state.web_ticks_left == 0 {
                continue;
            }
            if let Some(pin) = state.pin_anchor
                && let Some(mob) = targets.iter().find(|m| m.id == state.entity_id)
            {
                let chest = mob.position + DVec3::Y * (mob.height * 0.55);
                out.push(RenderSegment3D {
                    from: pin.to_array(),
                    to: chest.to_array(),
                    color_argb: web_color_argb,
                    width: 2.2,
                });
            }
        }
    }
}
