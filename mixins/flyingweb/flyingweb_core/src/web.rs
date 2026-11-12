use glam::DVec3;

use crate::input::WristSide;

/// Unique handle for an active or fading web strand.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StrandId(pub u64);

/// Lifecycle state of a rendered 3D web strand.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StrandState {
    /// Travelling from wrist toward anchor (`progress` in `0.0..=1.0`).
    Shooting { progress: f32 },
    /// Latched onto solid world geometry (`tension` in `0.0..=1.0`).
    Attached { tension: f32, rest_length: f64 },
    /// Recently released silk line drifting downward and fading cleanly over `ticks_left` (`1..=7`).
    Detaching { ticks_left: u8, max_ticks: u8 },
}

/// Role of a strand so multi-strand visuals (dual Point-Launch, Slingshot, Swing) coexist cleanly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrandRole {
    Swing,
    ZipOrLaunch,
    Slingshot,
}

/// One simulated & rendered web silk strand.
#[derive(Clone, Debug)]
pub struct WebStrand {
    pub id: StrandId,
    pub role: StrandRole,
    pub wrist: Option<WristSide>,
    pub prev_origin: DVec3,
    pub origin: DVec3,
    pub anchor: DVec3,
    pub anchor_normal: DVec3,
    pub state: StrandState,
    pub color_argb: u32,
    pub age_ticks: u32,
    /// Ticks left for the impact star splat decal on the block face (`0` = faded).
    pub impact_flash_ticks: u8,
}

/// A single 3D line primitive emitted for Horizon's `WorldPrimitives::line`.
#[derive(Clone, Copy, Debug)]
pub struct RenderSegment3D {
    pub from: [f64; 3],
    pub to: [f64; 3],
    pub color_argb: u32,
    pub width: f32,
}

/// Manages all active, dual-launch, and fading web strands with hard capacity bounds
/// and guaranteed zero-orphan cleanup.
#[derive(Clone, Debug)]
pub struct WebStrandManager {
    next_id: u64,
    strands: Vec<WebStrand>,
}

const MAX_ACTIVE_STRANDS: usize = 24;

impl Default for WebStrandManager {
    fn default() -> Self {
        Self {
            next_id: 1,
            strands: Vec::new(),
        }
    }
}

impl WebStrandManager {
    pub fn clear(&mut self) {
        self.strands.clear();
    }

    pub fn active_count(&self) -> usize {
        self.strands.len()
    }

    pub fn strands(&self) -> &[WebStrand] {
        &self.strands
    }

    fn alloc_id(&mut self) -> StrandId {
        let id = StrandId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1).max(1);
        id
    }

    fn enforce_cap(&mut self) {
        while self.strands.len() >= MAX_ACTIVE_STRANDS {
            // Evict oldest detaching strand first, or oldest strand overall.
            if let Some(idx) = self
                .strands
                .iter()
                .position(|s| matches!(s.state, StrandState::Detaching { .. }))
            {
                self.strands.remove(idx);
            } else {
                self.strands.remove(0);
            }
        }
    }

    pub fn spawn_swing_strand(
        &mut self,
        wrist: WristSide,
        origin: DVec3,
        anchor: DVec3,
        anchor_normal: DVec3,
        color_argb: u32,
    ) -> StrandId {
        self.spawn_custom_strand(
            StrandRole::Swing,
            Some(wrist),
            origin,
            anchor,
            anchor_normal,
            StrandState::Shooting { progress: 0.15 },
            color_argb,
        )
    }

    pub fn spawn_custom_strand(
        &mut self,
        role: StrandRole,
        wrist: Option<WristSide>,
        origin: DVec3,
        anchor: DVec3,
        anchor_normal: DVec3,
        state: StrandState,
        color_argb: u32,
    ) -> StrandId {
        self.enforce_cap();
        let id = self.alloc_id();
        let impact_flash_ticks = if matches!(state, StrandState::Attached { .. }) {
            10
        } else {
            0
        };
        self.strands.push(WebStrand {
            id,
            role,
            wrist,
            prev_origin: origin,
            origin,
            anchor,
            anchor_normal,
            state,
            color_argb,
            age_ticks: 0,
            impact_flash_ticks,
        });
        id
    }

    pub fn update_shooting_strand(&mut self, id: StrandId, origin: DVec3, progress: f32) {
        if let Some(s) = self.strands.iter_mut().find(|s| s.id == id) {
            s.prev_origin = s.origin;
            s.origin = origin;
            s.state = StrandState::Shooting {
                progress: progress.clamp(0.0, 1.0),
            };
        }
    }

    pub fn mark_attached(&mut self, id: StrandId, initial_tension: f32) {
        if let Some(s) = self.strands.iter_mut().find(|s| s.id == id) {
            let dist = s.origin.distance(s.anchor);
            s.state = StrandState::Attached {
                tension: initial_tension.clamp(0.0, 1.0),
                rest_length: dist,
            };
            s.impact_flash_ticks = 12;
        }
    }

    pub fn update_attached_strand(
        &mut self,
        id: StrandId,
        origin: DVec3,
        tension: f32,
        rest_length: f64,
    ) {
        if let Some(s) = self.strands.iter_mut().find(|s| s.id == id) {
            s.prev_origin = s.origin;
            s.origin = origin;
            s.state = StrandState::Attached {
                tension: tension.clamp(0.0, 1.0),
                rest_length,
            };
        }
    }

    pub fn detach_strand(&mut self, id: StrandId) {
        if let Some(s) = self.strands.iter_mut().find(|s| s.id == id) {
            s.wrist = None;
            s.state = StrandState::Detaching {
                ticks_left: 6,
                max_ticks: 6,
            };
        }
    }

    pub fn detach_by_role(&mut self, role: StrandRole) {
        for s in &mut self.strands {
            if s.role == role && !matches!(s.state, StrandState::Detaching { .. }) {
                s.wrist = None;
                s.state = StrandState::Detaching {
                    ticks_left: 5,
                    max_ticks: 5,
                };
            }
        }
    }

    /// Advances strand timers, drifting released silk strands under light gravity and removing expired ones.
    pub fn tick(&mut self) {
        for s in &mut self.strands {
            s.age_ticks = s.age_ticks.saturating_add(1);
            if s.impact_flash_ticks > 0 {
                s.impact_flash_ticks -= 1;
            }
            if let StrandState::Detaching { ticks_left, .. } = &mut s.state {
                s.prev_origin = s.origin;
                // Released end falls gently toward anchor so there is zero harsh pop
                let toward_anchor = (s.anchor - s.origin) * 0.22;
                s.origin += toward_anchor + DVec3::new(0.0, -0.18, 0.0);
                *ticks_left = ticks_left.saturating_sub(1);
            }
        }
        self.strands.retain(|s| {
            !matches!(s.state, StrandState::Detaching { ticks_left: 0, .. })
                && s.origin.is_finite()
                && s.anchor.is_finite()
        });
    }

    /// Generates sub-frame interpolated 3D silk strand + anchor web-splat geometry
    /// for `WorldPrimitives` at `partial_tick` (`0.0..=1.0`).
    ///
    /// `live_wrist_fn` supplies the exact sub-frame interpolated 3D wrist position for
    /// attached strands so the web NEVER lags behind the player's hand even at 240 FPS!
    pub fn build_render_segments(
        &self,
        partial_tick: f32,
        base_width: f32,
        enable_sag: bool,
        live_wrist_fn: impl Fn(WristSide) -> DVec3,
        out: &mut Vec<RenderSegment3D>,
    ) {
        let alpha_t = (partial_tick.clamp(0.0, 1.0)) as f64;

        for s in &self.strands {
            let start = if let Some(w) = s.wrist {
                live_wrist_fn(w)
            } else {
                s.prev_origin.lerp(s.origin, alpha_t)
            };
            let anchor = s.anchor;
            if !start.is_finite() || !anchor.is_finite() {
                continue;
            }

            match s.state {
                StrandState::Shooting { progress } => {
                    let p = (progress as f64).clamp(0.05, 1.0);
                    let tip = start.lerp(anchor, p);
                    let span = tip - start;
                    let len = span.length();
                    if len < 0.05 {
                        continue;
                    }
                    // Organic Spider-Verse / Insomniac whip wave along flying silk strand!
                    let side = span.cross(DVec3::Y).normalize_or(DVec3::X);
                    let steps = 8_usize;
                    let phase = (s.age_ticks as f64 + alpha_t) * 1.9;
                    let amp = ((1.0 - p) * 0.24).clamp(0.02, 0.24);

                    let mut prev_pt = start;
                    for i in 1..=steps {
                        let u = (i as f64) / (steps as f64);
                        let envelope = (u * std::f64::consts::PI).sin();
                        let wave = (u * std::f64::consts::PI * 3.0 - phase).sin() * amp * envelope;
                        let pt = start.lerp(tip, u) + side * wave;
                        out.push(RenderSegment3D {
                            from: prev_pt.to_array(),
                            to: pt.to_array(),
                            color_argb: s.color_argb,
                            width: base_width * 1.05,
                        });
                        prev_pt = pt;
                    }
                }
                StrandState::Attached {
                    tension,
                    rest_length,
                } => {
                    let dist = start.distance(anchor);
                    let slack = (rest_length - dist).max(0.0);
                    let sag_amount = if enable_sag {
                        let tension_relax = (1.0 - tension as f64).clamp(0.0, 1.0);
                        (slack * 0.35 + tension_relax * (dist * 0.035)).clamp(0.0, 1.65)
                    } else {
                        0.0
                    };

                    if sag_amount < 0.025 {
                        // Razor-clean taut primary silk strand + subtle inner bright core
                        out.push(RenderSegment3D {
                            from: start.to_array(),
                            to: anchor.to_array(),
                            color_argb: scale_alpha(s.color_argb, 0.42),
                            width: base_width * 1.65,
                        });
                        out.push(RenderSegment3D {
                            from: start.to_array(),
                            to: anchor.to_array(),
                            color_argb: s.color_argb,
                            width: base_width,
                        });
                    } else {
                        // Smooth 7-segment parabolic catenary curve when slack
                        let steps = 7_usize;
                        let mut prev_pt = start;
                        for i in 1..=steps {
                            let u = (i as f64) / (steps as f64);
                            let parabola = 4.0 * u * (1.0 - u);
                            let pt = start.lerp(anchor, u) - DVec3::Y * (sag_amount * parabola);
                            out.push(RenderSegment3D {
                                from: prev_pt.to_array(),
                                to: pt.to_array(),
                                color_argb: s.color_argb,
                                width: base_width * 0.95,
                            });
                            prev_pt = pt;
                        }
                    }

                    // Anchor radial web-splat star decal flush against the hit surface normal!
                    Self::emit_anchor_web_decal(
                        anchor,
                        s.anchor_normal,
                        s.color_argb,
                        s.impact_flash_ticks,
                        base_width,
                        out,
                    );
                }
                StrandState::Detaching {
                    ticks_left,
                    max_ticks,
                } => {
                    let fade = ((ticks_left as f32 - partial_tick * 0.8).max(0.05)
                        / (max_ticks.max(1) as f32))
                        .clamp(0.0, 0.75);
                    let faded_color = scale_alpha(s.color_argb, fade);
                    let sag = 0.45 * (1.0 - fade as f64);
                    let mid = start.lerp(anchor, 0.5) - DVec3::Y * sag;
                    out.push(RenderSegment3D {
                        from: start.to_array(),
                        to: mid.to_array(),
                        color_argb: faded_color,
                        width: (base_width * 0.75).max(1.0),
                    });
                    out.push(RenderSegment3D {
                        from: mid.to_array(),
                        to: anchor.to_array(),
                        color_argb: faded_color,
                        width: (base_width * 0.65).max(1.0),
                    });
                }
            }
        }
    }

    /// Draws a clean 6-spoke spider-web anchor splat flush on the hit block face (`normal * 0.015` offset).
    fn emit_anchor_web_decal(
        anchor: DVec3,
        normal: DVec3,
        color_argb: u32,
        flash_ticks: u8,
        base_width: f32,
        out: &mut Vec<RenderSegment3D>,
    ) {
        let n = normal.normalize_or(DVec3::Y);
        let ref_up = if n.y.abs() > 0.9 { DVec3::X } else { DVec3::Y };
        let t1 = n.cross(ref_up).normalize_or(DVec3::X);
        let t2 = n.cross(t1).normalize_or(DVec3::Z);

        let center = anchor + n * 0.018;
        let radius = 0.22 + (flash_ticks as f64) * 0.010;
        let decal_color = scale_alpha(color_argb, 0.82);

        let mut ring_pts = [DVec3::ZERO; 6];
        for (i, slot) in ring_pts.iter_mut().enumerate() {
            let angle = (i as f64) * (std::f64::consts::TAU / 6.0);
            let radial = t1 * angle.cos() + t2 * angle.sin();
            let spoke_end = center + radial * radius;
            *slot = spoke_end * 0.72 + center * 0.28;
            out.push(RenderSegment3D {
                from: center.to_array(),
                to: spoke_end.to_array(),
                color_argb: decal_color,
                width: (base_width * 0.80).max(1.1),
            });
        }

        // Hexagonal inner silk ring connecting the 6 spokes!
        for i in 0..6 {
            let next = (i + 1) % 6;
            out.push(RenderSegment3D {
                from: ring_pts[i].to_array(),
                to: ring_pts[next].to_array(),
                color_argb: scale_alpha(color_argb, 0.60),
                width: (base_width * 0.65).max(1.0),
            });
        }
    }
}

pub fn scale_alpha(argb: u32, factor: f32) -> u32 {
    let a = ((argb >> 24) & 0xFF) as f32;
    let new_a = (a * factor.clamp(0.0, 1.0)).round().clamp(0.0, 255.0) as u32;
    (argb & 0x00FF_FFFF) | (new_a << 24)
}
