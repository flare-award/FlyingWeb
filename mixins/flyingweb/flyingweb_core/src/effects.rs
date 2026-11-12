use glam::DVec3;

use crate::{
    config::FlyingWebConfig,
    gadgets::{GadgetEvent, GadgetKind},
    movement::TraversalEvent,
    swinging::SwingEvent,
};

/// One original layered local sound cue scheduled via `Game::native_play_local_sound`.
#[derive(Clone, Debug)]
pub struct SoundCue {
    pub sound: String,
    pub at: [f64; 3],
    pub volume: f32,
    pub pitch: f32,
    pub delay_ticks: i32,
}

/// Particle family mapped onto Horizon's `ParticleOptions` (`native_add_particle`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleFlavor {
    SilkMist,
    AnchorSpark,
    WindStreak,
    GadgetBurst,
}

#[derive(Clone, Copy, Debug)]
pub struct ParticleCue {
    pub flavor: ParticleFlavor,
    pub at: [f64; 3],
    pub velocity: [f64; 3],
}

/// Translates simulation events and high-speed wind/glide states into original multi-layered
/// acoustic sound cues and lightweight particle cues.
#[derive(Clone, Debug, Default)]
pub struct EffectsSynthesizer {
    wind_tick_accum: u32,
}

impl EffectsSynthesizer {
    pub fn reset(&mut self) {
        self.wind_tick_accum = 0;
    }

    pub fn emit_swing_event(
        &self,
        ev: SwingEvent,
        player_pos: DVec3,
        cfg: &FlyingWebConfig,
        sounds: &mut Vec<SoundCue>,
        particles: &mut Vec<ParticleCue>,
    ) {
        let vol = cfg.sound_volume;
        match ev {
            SwingEvent::ShotLaunched { origin, anchor, .. } => {
                if vol > 0.01 {
                    // Original 3-voice "Thwip!" signature: snappy high-tension cord + pneumatic hiss + latch click!
                    sounds.push(SoundCue {
                        sound: "minecraft:entity.fishing_bobber.throw".to_owned(),
                        at: origin.to_array(),
                        volume: (0.62 * vol).clamp(0.0, 2.0),
                        pitch: 1.88,
                        delay_ticks: 0,
                    });
                    sounds.push(SoundCue {
                        sound: "minecraft:item.crossbow.shoot".to_owned(),
                        at: origin.to_array(),
                        volume: (0.26 * vol).clamp(0.0, 2.0),
                        pitch: 1.96,
                        delay_ticks: 0,
                    });
                    sounds.push(SoundCue {
                        sound: "minecraft:block.tripwire.attach".to_owned(),
                        at: origin.to_array(),
                        volume: (0.38 * vol).clamp(0.0, 2.0),
                        pitch: 1.72,
                        delay_ticks: 1,
                    });
                }
                if cfg.particle_density > 0.05 {
                    let dir = (anchor - origin).normalize_or(DVec3::Y) * 0.45;
                    particles.push(ParticleCue {
                        flavor: ParticleFlavor::SilkMist,
                        at: origin.to_array(),
                        velocity: dir.to_array(),
                    });
                }
            }
            SwingEvent::AnchorAttached { anchor, normal, .. } => {
                if vol > 0.01 {
                    sounds.push(SoundCue {
                        sound: "minecraft:entity.leash_knot.place".to_owned(),
                        at: anchor.to_array(),
                        volume: (0.68 * vol).clamp(0.0, 2.0),
                        pitch: 1.44,
                        delay_ticks: 0,
                    });
                }
                if cfg.particle_density > 0.05 {
                    let splash = normal * 0.14;
                    for i in 0..3 {
                        let off = ((i as f64) - 1.0) * 0.08;
                        particles.push(ParticleCue {
                            flavor: ParticleFlavor::AnchorSpark,
                            at: (anchor + normal * 0.04).to_array(),
                            velocity: [splash.x + off, splash.y + 0.06, splash.z - off],
                        });
                    }
                }
            }
            SwingEvent::Released {
                vault_jump,
                release_speed,
            } => {
                if vol > 0.01 {
                    sounds.push(SoundCue {
                        sound: "minecraft:entity.leash_knot.break".to_owned(),
                        at: player_pos.to_array(),
                        volume: (0.45 * vol).clamp(0.0, 2.0),
                        pitch: if vault_jump { 1.62 } else { 1.38 },
                        delay_ticks: 0,
                    });
                    if vault_jump {
                        sounds.push(SoundCue {
                            sound: "minecraft:entity.player.attack.sweep".to_owned(),
                            at: player_pos.to_array(),
                            volume: (0.35 * vol * (release_speed as f32).clamp(0.5, 1.4))
                                .clamp(0.0, 2.0),
                            pitch: 1.52,
                            delay_ticks: 1,
                        });
                    }
                }
            }
            SwingEvent::MissedShot { .. } => {
                if vol > 0.01 {
                    sounds.push(SoundCue {
                        sound: "minecraft:block.dispenser.fail".to_owned(),
                        at: player_pos.to_array(),
                        volume: (0.28 * vol).clamp(0.0, 2.0),
                        pitch: 1.85,
                        delay_ticks: 0,
                    });
                }
            }
        }
    }

    pub fn emit_traversal_event(
        &self,
        ev: TraversalEvent,
        player_pos: DVec3,
        step_sound_hint: &str,
        cfg: &FlyingWebConfig,
        sounds: &mut Vec<SoundCue>,
        particles: &mut Vec<ParticleCue>,
    ) {
        let vol = cfg.sound_volume;
        match ev {
            TraversalEvent::WebZipFired { dir } => {
                if vol > 0.01 {
                    sounds.push(SoundCue {
                        sound: "minecraft:entity.fishing_bobber.retrieve".to_owned(),
                        at: player_pos.to_array(),
                        volume: (0.72 * vol).clamp(0.0, 2.0),
                        pitch: 1.75,
                        delay_ticks: 0,
                    });
                    sounds.push(SoundCue {
                        sound: "minecraft:entity.player.attack.sweep".to_owned(),
                        at: player_pos.to_array(),
                        volume: (0.40 * vol).clamp(0.0, 2.0),
                        pitch: 1.60,
                        delay_ticks: 1,
                    });
                }
                if cfg.particle_density > 0.05 {
                    particles.push(ParticleCue {
                        flavor: ParticleFlavor::SilkMist,
                        at: (player_pos + DVec3::Y * 1.1).to_array(),
                        velocity: (-dir * 0.25).to_array(),
                    });
                }
            }
            TraversalEvent::PointLaunchStarted { perch } => {
                if vol > 0.01 {
                    sounds.push(SoundCue {
                        sound: "minecraft:entity.fishing_bobber.throw".to_owned(),
                        at: player_pos.to_array(),
                        volume: (0.68 * vol).clamp(0.0, 2.0),
                        pitch: 1.92,
                        delay_ticks: 0,
                    });
                    sounds.push(SoundCue {
                        sound: "minecraft:block.tripwire.attach".to_owned(),
                        at: perch.to_array(),
                        volume: (0.55 * vol).clamp(0.0, 2.0),
                        pitch: 1.62,
                        delay_ticks: 1,
                    });
                }
            }
            TraversalEvent::PointLaunchVaulted { .. } => {
                if vol > 0.01 {
                    sounds.push(SoundCue {
                        sound: "minecraft:entity.firework_rocket.launch".to_owned(),
                        at: player_pos.to_array(),
                        volume: (0.36 * vol).clamp(0.0, 2.0),
                        pitch: 1.72,
                        delay_ticks: 0,
                    });
                }
            }
            TraversalEvent::SlingshotStarted => {
                if vol > 0.01 {
                    sounds.push(SoundCue {
                        sound: "minecraft:item.crossbow.loading_middle".to_owned(),
                        at: player_pos.to_array(),
                        volume: (0.72 * vol).clamp(0.0, 2.0),
                        pitch: 1.35,
                        delay_ticks: 0,
                    });
                }
            }
            TraversalEvent::SlingshotReleased { charge, .. } => {
                if vol > 0.01 {
                    sounds.push(SoundCue {
                        sound: "minecraft:item.crossbow.shoot".to_owned(),
                        at: player_pos.to_array(),
                        volume: (0.85 * vol).clamp(0.0, 2.0),
                        pitch: 1.25 + charge * 0.45,
                        delay_ticks: 0,
                    });
                    sounds.push(SoundCue {
                        sound: "minecraft:entity.ender_dragon.flap".to_owned(),
                        at: player_pos.to_array(),
                        volume: (0.50 * vol).clamp(0.0, 2.0),
                        pitch: 1.65,
                        delay_ticks: 1,
                    });
                }
            }
            TraversalEvent::WebWingsDeployed => {
                if vol > 0.01 {
                    sounds.push(SoundCue {
                        sound: "minecraft:item.armor.equip_elytra".to_owned(),
                        at: player_pos.to_array(),
                        volume: (0.82 * vol).clamp(0.0, 2.0),
                        pitch: 1.42,
                        delay_ticks: 0,
                    });
                }
            }
            TraversalEvent::WebWingsStowed => {
                if vol > 0.01 {
                    sounds.push(SoundCue {
                        sound: "minecraft:item.armor.equip_leather".to_owned(),
                        at: player_pos.to_array(),
                        volume: (0.55 * vol).clamp(0.0, 2.0),
                        pitch: 1.35,
                        delay_ticks: 0,
                    });
                }
            }
            TraversalEvent::AirTrickCompleted { combo, .. } => {
                if vol > 0.01 {
                    let p = (1.15 + (combo.min(8) as f32) * 0.09).clamp(1.0, 1.95);
                    sounds.push(SoundCue {
                        sound: "minecraft:block.amethyst_block.chime".to_owned(),
                        at: player_pos.to_array(),
                        volume: (0.55 * vol).clamp(0.0, 2.0),
                        pitch: p,
                        delay_ticks: 0,
                    });
                }
            }
            TraversalEvent::WallRunStep { pos, .. } => {
                if vol > 0.01 {
                    let ev_name = if step_sound_hint.is_empty() {
                        "minecraft:block.stone.step"
                    } else {
                        step_sound_hint
                    };
                    sounds.push(SoundCue {
                        sound: ev_name.to_owned(),
                        at: pos.to_array(),
                        volume: (0.58 * vol).clamp(0.0, 2.0),
                        pitch: 1.28,
                        delay_ticks: 0,
                    });
                }
            }
            TraversalEvent::WallKickOff { .. } => {
                if vol > 0.01 {
                    sounds.push(SoundCue {
                        sound: "minecraft:entity.player.attack.nodamage".to_owned(),
                        at: player_pos.to_array(),
                        volume: (0.62 * vol).clamp(0.0, 2.0),
                        pitch: 1.48,
                        delay_ticks: 0,
                    });
                }
            }
            TraversalEvent::SuperheroLanding { impact_speed } => {
                if vol > 0.01 {
                    sounds.push(SoundCue {
                        sound: "minecraft:block.deepslate.fall".to_owned(),
                        at: player_pos.to_array(),
                        volume: (0.85 * vol * (impact_speed as f32).clamp(0.7, 1.4))
                            .clamp(0.0, 2.0),
                        pitch: 0.92,
                        delay_ticks: 0,
                    });
                }
                if cfg.particle_density > 0.05 {
                    for i in 0..8 {
                        let a = (i as f64) * (std::f64::consts::TAU / 8.0);
                        particles.push(ParticleCue {
                            flavor: ParticleFlavor::SilkMist,
                            at: (player_pos + DVec3::Y * 0.1).to_array(),
                            velocity: [a.cos() * 0.22, 0.05, a.sin() * 0.22],
                        });
                    }
                }
            }
        }
    }

    pub fn emit_gadget_event(
        &self,
        ev: GadgetEvent,
        cfg: &FlyingWebConfig,
        sounds: &mut Vec<SoundCue>,
        particles: &mut Vec<ParticleCue>,
    ) {
        let vol = cfg.sound_volume;
        match ev {
            GadgetEvent::Fired { kind, at } => {
                if vol > 0.01 {
                    let pitch = match kind {
                        GadgetKind::ImpactWeb => 1.55,
                        GadgetKind::RicochetWeb => 1.82,
                        GadgetKind::TripMine | GadgetKind::RemoteMine => 1.32,
                        _ => 1.68,
                    };
                    sounds.push(SoundCue {
                        sound: "minecraft:entity.snowball.throw".to_owned(),
                        at: at.to_array(),
                        volume: (0.60 * vol).clamp(0.0, 2.0),
                        pitch,
                        delay_ticks: 0,
                    });
                }
            }
            GadgetEvent::Burst { at, .. } => {
                if vol > 0.01 {
                    sounds.push(SoundCue {
                        sound: "minecraft:entity.Cobweb.place".to_ascii_lowercase().replace("entity.cobweb", "block.cobweb"),
                        at: at.to_array(),
                        volume: (0.82 * vol).clamp(0.0, 2.0),
                        pitch: 1.25,
                        delay_ticks: 0,
                    });
                    sounds.push(SoundCue {
                        sound: "minecraft:entity.leash_knot.place".to_owned(),
                        at: at.to_array(),
                        volume: (0.72 * vol).clamp(0.0, 2.0),
                        pitch: 1.48,
                        delay_ticks: 1,
                    });
                }
                if cfg.particle_density > 0.05 {
                    for i in 0..8 {
                        let a = (i as f64) * (std::f64::consts::TAU / 8.0);
                        particles.push(ParticleCue {
                            flavor: ParticleFlavor::GadgetBurst,
                            at: at.to_array(),
                            velocity: [a.cos() * 0.28, 0.14, a.sin() * 0.28],
                        });
                    }
                }
            }
            GadgetEvent::RicochetBounce { at } => {
                if vol > 0.01 {
                    sounds.push(SoundCue {
                        sound: "minecraft:block.amethyst_block.hit".to_owned(),
                        at: at.to_array(),
                        volume: (0.62 * vol).clamp(0.0, 2.0),
                        pitch: 1.78,
                        delay_ticks: 0,
                    });
                }
            }
            GadgetEvent::EntityWebbed { at, .. } => {
                if vol > 0.01 {
                    sounds.push(SoundCue {
                        sound: "minecraft:entity.leash_knot.place".to_owned(),
                        at: at.to_array(),
                        volume: (0.65 * vol).clamp(0.0, 2.0),
                        pitch: 1.52,
                        delay_ticks: 0,
                    });
                }
                if cfg.particle_density > 0.05 {
                    particles.push(ParticleCue {
                        flavor: ParticleFlavor::SilkMist,
                        at: at.to_array(),
                        velocity: [0.0, 0.08, 0.0],
                    });
                }
            }
            GadgetEvent::WebLineAnchored { start, end } => {
                if vol > 0.01 {
                    sounds.push(SoundCue {
                        sound: "minecraft:item.crossbow.loading_end".to_owned(),
                        at: start.lerp(end, 0.5).to_array(),
                        volume: (0.75 * vol).clamp(0.0, 2.0),
                        pitch: 1.45,
                        delay_ticks: 0,
                    });
                }
            }
        }
    }

    /// Emits speed-reactive wind streaks & wingtip vortices when swinging/gliding fast!
    pub fn emit_ambient_motion_fx(
        &mut self,
        player_feet: DVec3,
        velocity: DVec3,
        gliding: bool,
        cfg: &FlyingWebConfig,
        sounds: &mut Vec<SoundCue>,
        particles: &mut Vec<ParticleCue>,
    ) {
        self.wind_tick_accum = self.wind_tick_accum.wrapping_add(1);
        let speed = velocity.length();
        if speed > 0.85 && cfg.particle_density > 0.15 && self.wind_tick_accum % 2 == 0 {
            let back = (-velocity).normalize_or(DVec3::NEG_Z) * 0.25;
            particles.push(ParticleCue {
                flavor: ParticleFlavor::WindStreak,
                at: (player_feet + DVec3::Y * 1.1).to_array(),
                velocity: back.to_array(),
            });
        }
        if gliding && speed > 0.55 && cfg.sound_volume > 0.05 && self.wind_tick_accum % 16 == 0 {
            sounds.push(SoundCue {
                sound: "minecraft:entity.phantom.flap".to_owned(),
                at: player_feet.to_array(),
                volume: (0.18 * cfg.sound_volume * (speed as f32).clamp(0.5, 1.5)).clamp(0.0, 1.0),
                pitch: 1.75,
                delay_ticks: 0,
            });
        }
    }
}
