use crate::{
    config::{FlyingWebConfig, TuningPreset},
    gadgets::{GadgetKind, GadgetManager},
    input::{InputRouter, SpiderAction},
    movement::{AirTrickKind, TraversalController, TraversalState},
    suit::SuitAtlasManager,
    swinging::SwingController,
};

/// Tabs in the in-game FlyingWeb Settings Screen (`Screen::Mod` for `"flyingweb"`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SettingsTab {
    #[default]
    Swinging,
    Camera,
    Traversal,
    Gadgets,
    SuitAndFx,
    Controls,
}

impl SettingsTab {
    pub const ALL: [Self; 6] = [
        Self::Swinging,
        Self::Camera,
        Self::Traversal,
        Self::Gadgets,
        Self::SuitAndFx,
        Self::Controls,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Swinging => "Swinging",
            Self::Camera => "Spider-Cam",
            Self::Traversal => "Traversal",
            Self::Gadgets => "Gadgets",
            Self::SuitAndFx => "Suit & FX",
            Self::Controls => "Controls",
        }
    }
}

/// Abstract 2D draw commands rendered cleanly onto Horizon's `HudBatch` in `glue.rs`.
#[derive(Clone, Debug)]
pub enum UiDrawCmd {
    FillRect {
        rect: [f32; 4],
        color_argb: u32,
    },
    ModSprite {
        src: [f32; 4],
        dst: [f32; 4],
        color_argb: u32,
    },
    Text {
        text: String,
        at: [f32; 2],
        color_argb: u32,
    },
}

/// Interactive row descriptor on the Settings Screen.
#[derive(Clone, Debug)]
enum ControlWidget {
    Toggle {
        label: &'static str,
        value: bool,
        id: &'static str,
    },
    Slider {
        label: &'static str,
        value: f32,
        min: f32,
        max: f32,
        display: String,
        id: &'static str,
    },
    CycleButton {
        label: &'static str,
        value_text: String,
        id: &'static str,
    },
    KeybindRow {
        action: SpiderAction,
        bound_key: String,
        listening: bool,
    },
}

#[derive(Clone, Debug, Default)]
pub struct FlyingWebUiState {
    pub active_tab: SettingsTab,
    pub status_banner: Option<(String, u8)>,
}

/// Side-effect actions returned when clicking inside the Settings Screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiClickOutcome {
    None,
    ConfigChanged,
    ReloadSuitTexture,
    CloseScreen,
}

impl FlyingWebUiState {
    pub fn tick(&mut self) {
        if let Some((_, ticks)) = &mut self.status_banner {
            if *ticks <= 1 {
                self.status_banner = None;
            } else {
                *ticks -= 1;
            }
        }
    }

    pub fn set_banner(&mut self, msg: impl Into<String>) {
        self.status_banner = Some((msg.into(), 60));
    }

    /// Builds crisp Spider-HUD draw commands (Smart Anchor Reticle, Speedometer, Air-Trick banner,
    /// Slingshot Charge bar, and Gadget Card with ammo pips).
    #[allow(clippy::too_many_arguments)]
    pub fn build_hud(
        &self,
        screen_w: f32,
        screen_h: f32,
        cfg: &FlyingWebConfig,
        swing: &SwingController,
        traversal: &TraversalController,
        gadgets: &GadgetManager,
        speed_blocks_per_tick: f64,
        out: &mut Vec<UiDrawCmd>,
    ) {
        if !cfg.abilities_active() || cfg.hud_style == crate::config::HudStyle::Hidden {
            return;
        }

        let cx = screen_w * 0.5;
        let cy = screen_h * 0.5;

        // 1. Smart Anchor / Perch Reticle around center crosshair
        if swing.phase.is_attached() {
            // Taut swing bracket around crosshair
            out.push(UiDrawCmd::FillRect {
                rect: [cx - 8.0, cy - 1.0, 4.0, 2.0],
                color_argb: 0xDC70_E8FF,
            });
            out.push(UiDrawCmd::FillRect {
                rect: [cx + 4.0, cy - 1.0, 4.0, 2.0],
                color_argb: 0xDC70_E8FF,
            });
        } else if traversal.hovered_perch.is_some() {
            // Point-Launch Perch Ready double-chevron indicator!
            out.push(UiDrawCmd::FillRect {
                rect: [cx - 10.0, cy - 10.0, 20.0, 2.0],
                color_argb: 0xEE58_FFB4,
            });
            out.push(UiDrawCmd::FillRect {
                rect: [cx - 10.0, cy + 8.0, 20.0, 2.0],
                color_argb: 0xEE58_FFB4,
            });
            if cfg.hud_style == crate::config::HudStyle::Full {
                out.push(UiDrawCmd::Text {
                    text: format!("[{}] Point Launch", cfg.keys.web_zip),
                    at: [cx + 14.0, cy - 4.0],
                    color_argb: 0xEE98_FFD2,
                });
            }
        } else if let Some(cand) = swing.hovered_anchor {
            let color = if cand.direct_aim {
                0xEEFF_FFFF
            } else {
                0xCCFF_4458
            };
            out.push(UiDrawCmd::FillRect {
                rect: [cx - 7.0, cy - 7.0, 4.0, 2.0],
                color_argb: color,
            });
            out.push(UiDrawCmd::FillRect {
                rect: [cx + 3.0, cy - 7.0, 4.0, 2.0],
                color_argb: color,
            });
            out.push(UiDrawCmd::FillRect {
                rect: [cx - 7.0, cy + 5.0, 4.0, 2.0],
                color_argb: color,
            });
            out.push(UiDrawCmd::FillRect {
                rect: [cx + 3.0, cy + 5.0, 4.0, 2.0],
                color_argb: color,
            });
        }

        // Slingshot Charge Meter right below crosshair!
        if let TraversalState::SlingshotCharging { charge, .. } = traversal.state {
            let bw = 84.0;
            let bx = cx - bw * 0.5;
            let by = cy + 18.0;
            out.push(UiDrawCmd::FillRect {
                rect: [bx - 1.0, by - 1.0, bw + 2.0, 7.0],
                color_argb: 0xC810_141E,
            });
            out.push(UiDrawCmd::FillRect {
                rect: [bx, by, bw * charge.clamp(0.0, 1.0), 5.0],
                color_argb: 0xFFFF_4858,
            });
            out.push(UiDrawCmd::Text {
                text: "SLINGSHOT CHARGE".to_owned(),
                at: [bx - 4.0, by + 9.0],
                color_argb: 0xFFFF_D6DC,
            });
        }

        if cfg.hud_style == crate::config::HudStyle::Minimal {
            return;
        }

        // 2. Compact Spider-Status & Gadget Card in bottom-left corner (avoids vanilla hotbar!)
        let card_x = 10.0;
        let card_y = (screen_h - 56.0).max(12.0);
        let card_w = 148.0;
        let card_h = 44.0;

        out.push(UiDrawCmd::FillRect {
            rect: [card_x, card_y, card_w, card_h],
            color_argb: 0xB410_1522,
        });
        // Crimson top accent stripe
        out.push(UiDrawCmd::FillRect {
            rect: [card_x, card_y, card_w, 2.0],
            color_argb: 0xEEEC_2C3E,
        });

        // Spider emblem + Speed in m/s
        out.push(UiDrawCmd::ModSprite {
            src: SuitAtlasManager::hud_spider_icon_src(),
            dst: [card_x + 5.0, card_y + 5.0, 14.0, 14.0],
            color_argb: 0xFFFF_FFFF,
        });
        let speed_mps = speed_blocks_per_tick * 20.0;
        out.push(UiDrawCmd::Text {
            text: format!("{speed_mps:4.1} m/s  Zip:{}", traversal.air_zip_charges),
            at: [card_x + 23.0, card_y + 8.0],
            color_argb: 0xFFE8_F2FF,
        });

        // Active Spider-Gadget row + cartridge pips
        if cfg.gadgets_enabled {
            let g = gadgets.selected;
            let (cur_ammo, max_ammo, recharge) = gadgets.ammo_of(g);
            out.push(UiDrawCmd::ModSprite {
                src: SuitAtlasManager::hud_gadget_icon_src(g),
                dst: [card_x + 5.0, card_y + 23.0, 14.0, 14.0],
                color_argb: 0xFFFF_FFFF,
            });
            let remote_tag = if g == GadgetKind::RemoteMine && gadgets.has_armed_remote_mines() {
                " [ARMED]"
            } else {
                ""
            };
            out.push(UiDrawCmd::Text {
                text: format!("{}{}", g.label(), remote_tag),
                at: [card_x + 23.0, card_y + 23.0],
                color_argb: 0xFFB8_DCFF,
            });

            let pip_y = card_y + 35.0;
            for p in 0..max_ammo {
                let px = card_x + 23.0 + (p as f32) * 16.0;
                let bg = 0xAA26_3044;
                out.push(UiDrawCmd::FillRect {
                    rect: [px, pip_y, 13.0, 4.0],
                    color_argb: bg,
                });
                if p < cur_ammo {
                    out.push(UiDrawCmd::FillRect {
                        rect: [px, pip_y, 13.0, 4.0],
                        color_argb: 0xFF56_DCFF,
                    });
                } else if p == cur_ammo {
                    out.push(UiDrawCmd::FillRect {
                        rect: [px, pip_y, 13.0 * recharge, 4.0],
                        color_argb: 0xCC3C_98CC,
                    });
                }
            }
        }

        // Crawl latch & Air-Trick style callout above card
        if traversal.crawl_latch_enabled {
            out.push(UiDrawCmd::Text {
                text: "WALL/CEILING CRAWL ON".to_owned(),
                at: [card_x + 2.0, card_y - 12.0],
                color_argb: 0xEE72_FFB8,
            });
        }
        if let Some((trick, _)) = traversal.active_trick {
            let banner_txt = Self::trick_banner(trick, traversal.trick_combo.max(1));
            out.push(UiDrawCmd::Text {
                text: banner_txt,
                at: [card_x + 2.0, card_y - 24.0],
                color_argb: 0xFFFF_DA68,
            });
        }
    }

    fn trick_banner(kind: AirTrickKind, combo: u32) -> String {
        format!("x{combo} {}", kind.label())
    }

    /// Builds the interactive tabbed FlyingWeb Settings Screen (`Screen::Mod` for `"flyingweb"`).
    pub fn build_settings_screen(
        &self,
        screen_rect: [f32; 4],
        mouse: [f32; 2],
        cfg: &FlyingWebConfig,
        router: &InputRouter,
        out: &mut Vec<UiDrawCmd>,
    ) {
        let [sx, sy, sw, sh] = screen_rect;
        // Translucent dark backdrop
        out.push(UiDrawCmd::FillRect {
            rect: [sx, sy, sw, sh],
            color_argb: 0xC408_0B14,
        });

        let panel_w = (sw - 36.0).clamp(280.0, 440.0);
        let panel_h = (sh - 28.0).clamp(200.0, 290.0);
        let px = sx + (sw - panel_w) * 0.5;
        let py = sy + (sh - panel_h) * 0.5;

        // Main panel frame + crimson header bar
        out.push(UiDrawCmd::FillRect {
            rect: [px, py, panel_w, panel_h],
            color_argb: 0xEE12_1726,
        });
        out.push(UiDrawCmd::FillRect {
            rect: [px, py, panel_w, 22.0],
            color_argb: 0xFFB8_1E2E,
        });
        out.push(UiDrawCmd::Text {
            text: "FLYINGWEB  —  SPIDER-HERO SETTINGS".to_owned(),
            at: [px + 10.0, py + 7.0],
            color_argb: 0xFFFF_FFFF,
        });

        // Close `[X]` button top-right
        let close_rect = [px + panel_w - 22.0, py + 3.0, 18.0, 16.0];
        let over_close = point_in_rect(mouse, close_rect);
        out.push(UiDrawCmd::FillRect {
            rect: close_rect,
            color_argb: if over_close { 0xFFFF_5262 } else { 0xAA22_0A10 },
        });
        out.push(UiDrawCmd::Text {
            text: "X".to_owned(),
            at: [close_rect[0] + 6.0, close_rect[1] + 4.0],
            color_argb: 0xFFFF_FFFF,
        });

        // 6 Category Tabs
        let tab_y = py + 25.0;
        let tab_w = (panel_w - 16.0) / (SettingsTab::ALL.len() as f32);
        for (idx, tab) in SettingsTab::ALL.iter().copied().enumerate() {
            let tx = px + 8.0 + (idx as f32) * tab_w;
            let trect = [tx + 1.0, tab_y, tab_w - 2.0, 16.0];
            let selected = self.active_tab == tab;
            let hovered = point_in_rect(mouse, trect);
            let bg = if selected {
                0xFF28_56A8
            } else if hovered {
                0xFF22_2E48
            } else {
                0xFF18_2034
            };
            out.push(UiDrawCmd::FillRect {
                rect: trect,
                color_argb: bg,
            });
            out.push(UiDrawCmd::Text {
                text: tab.label().to_owned(),
                at: [trect[0] + 4.0, trect[1] + 4.0],
                color_argb: if selected { 0xFFFF_FFFF } else { 0xFFB8_C8E4 },
            });
        }

        // Widget rows for the active tab (2-column grid for clean readability!)
        let widgets = Self::widgets_for_tab(self.active_tab, cfg, router);
        let start_y = tab_y + 22.0;
        let col_w = (panel_w - 24.0) * 0.5;
        let row_h = 20.0;

        for (i, w) in widgets.iter().enumerate() {
            let col = i % 2;
            let row = i / 2;
            let wx = px + 8.0 + (col as f32) * (col_w + 8.0);
            let wy = start_y + (row as f32) * (row_h + 4.0);
            if wy + row_h > py + panel_h - 26.0 {
                break;
            }
            let wrect = [wx, wy, col_w, row_h];
            let hovered = point_in_rect(mouse, wrect);

            match w {
                ControlWidget::Toggle { label, value, .. } => {
                    out.push(UiDrawCmd::FillRect {
                        rect: wrect,
                        color_argb: if hovered { 0xFF24_304C } else { 0xFF1A_2236 },
                    });
                    let pill_color = if *value { 0xFF2E_B872 } else { 0xFF68_323C };
                    out.push(UiDrawCmd::FillRect {
                        rect: [wx + col_w - 32.0, wy + 3.0, 28.0, row_h - 6.0],
                        color_argb: pill_color,
                    });
                    out.push(UiDrawCmd::Text {
                        text: (*label).to_owned(),
                        at: [wx + 5.0, wy + 6.0],
                        color_argb: 0xFFE6_EEFF,
                    });
                    out.push(UiDrawCmd::Text {
                        text: if *value { "ON" } else { "OFF" }.to_owned(),
                        at: [wx + col_w - 27.0, wy + 6.0],
                        color_argb: 0xFFFF_FFFF,
                    });
                }
                ControlWidget::Slider {
                    label,
                    value,
                    min,
                    max,
                    display,
                    ..
                } => {
                    out.push(UiDrawCmd::FillRect {
                        rect: wrect,
                        color_argb: if hovered { 0xFF24_304C } else { 0xFF1A_2236 },
                    });
                    let frac = ((*value - *min) / (*max - *min).max(1e-5)).clamp(0.0, 1.0);
                    out.push(UiDrawCmd::FillRect {
                        rect: [wx + 1.0, wy + row_h - 4.0, (col_w - 2.0) * frac, 3.0],
                        color_argb: 0xFF42_A8FF,
                    });
                    out.push(UiDrawCmd::Text {
                        text: format!("{label}: {display}"),
                        at: [wx + 5.0, wy + 5.0],
                        color_argb: 0xFFE6_EEFF,
                    });
                }
                ControlWidget::CycleButton {
                    label, value_text, ..
                } => {
                    out.push(UiDrawCmd::FillRect {
                        rect: wrect,
                        color_argb: if hovered { 0xFF2C_3C60 } else { 0xFF1E_2840 },
                    });
                    out.push(UiDrawCmd::Text {
                        text: format!("{label}: {value_text}"),
                        at: [wx + 5.0, wy + 6.0],
                        color_argb: 0xFFFF_EAA8,
                    });
                }
                ControlWidget::KeybindRow {
                    action,
                    bound_key,
                    listening,
                } => {
                    out.push(UiDrawCmd::FillRect {
                        rect: wrect,
                        color_argb: if *listening {
                            0xFF6A_3A18
                        } else if hovered {
                            0xFF24_304C
                        } else {
                            0xFF1A_2236
                        },
                    });
                    let rhs = if *listening {
                        "> PRESS KEY <"
                    } else {
                        bound_key.as_str()
                    };
                    out.push(UiDrawCmd::Text {
                        text: format!("{}: [{}]", action.label(), rhs),
                        at: [wx + 5.0, wy + 6.0],
                        color_argb: 0xFFE8_F2FF,
                    });
                }
            }
        }

        // Footer bar: Quick Presets + Reload Custom Suit PNG + Reset Defaults
        let fy = py + panel_h - 22.0;
        let footer_btns = [
            ("Preset: Cinematic", [px + 8.0, fy, 102.0, 17.0]),
            ("Preset: Comfort", [px + 114.0, fy, 94.0, 17.0]),
            ("Reload Suit PNG", [px + 212.0, fy, 96.0, 17.0]),
            ("Reset Defaults", [px + 312.0, fy, (panel_w - 320.0).max(76.0), 17.0]),
        ];
        for (txt, rect) in footer_btns {
            if rect[0] + rect[2] <= px + panel_w - 4.0 {
                let hov = point_in_rect(mouse, rect);
                out.push(UiDrawCmd::FillRect {
                    rect,
                    color_argb: if hov { 0xFF34_4870 } else { 0xFF20_2A42 },
                });
                out.push(UiDrawCmd::Text {
                    text: txt.to_owned(),
                    at: [rect[0] + 5.0, rect[1] + 5.0],
                    color_argb: 0xFFD8_E8FF,
                });
            }
        }

        if let Some((banner, _)) = &self.status_banner {
            out.push(UiDrawCmd::Text {
                text: banner.clone(),
                at: [px + 185.0, py + 7.0],
                color_argb: 0xFF98_FFBE,
            });
        }
    }

    /// Processes a left/right mouse click inside the Settings Screen and updates `cfg` in place.
    pub fn on_mouse_press(
        &mut self,
        screen_rect: [f32; 4],
        mouse: [f32; 2],
        button: i32,
        cfg: &mut FlyingWebConfig,
        router: &mut InputRouter,
    ) -> UiClickOutcome {
        let [sx, sy, sw, sh] = screen_rect;
        let panel_w = (sw - 36.0).clamp(280.0, 440.0);
        let panel_h = (sh - 28.0).clamp(200.0, 290.0);
        let px = sx + (sw - panel_w) * 0.5;
        let py = sy + (sh - panel_h) * 0.5;

        let close_rect = [px + panel_w - 22.0, py + 3.0, 18.0, 16.0];
        if point_in_rect(mouse, close_rect) {
            router.awaiting_rebind = None;
            return UiClickOutcome::CloseScreen;
        }

        let tab_y = py + 25.0;
        let tab_w = (panel_w - 16.0) / (SettingsTab::ALL.len() as f32);
        for (idx, tab) in SettingsTab::ALL.iter().copied().enumerate() {
            let tx = px + 8.0 + (idx as f32) * tab_w;
            let trect = [tx + 1.0, tab_y, tab_w - 2.0, 16.0];
            if point_in_rect(mouse, trect) {
                self.active_tab = tab;
                router.awaiting_rebind = None;
                return UiClickOutcome::None;
            }
        }

        // Footer buttons
        let fy = py + panel_h - 22.0;
        if point_in_rect(mouse, [px + 8.0, fy, 102.0, 17.0]) {
            cfg.apply_preset(TuningPreset::InsomniacCinematic);
            self.set_banner("Applied Cinematic Preset");
            return UiClickOutcome::ConfigChanged;
        }
        if point_in_rect(mouse, [px + 114.0, fy, 94.0, 17.0]) {
            cfg.apply_preset(TuningPreset::ComfortLowMotion);
            self.set_banner("Applied Comfort Preset");
            return UiClickOutcome::ConfigChanged;
        }
        if point_in_rect(mouse, [px + 212.0, fy, 96.0, 17.0]) {
            self.set_banner("Reloaded Suit Texture");
            return UiClickOutcome::ReloadSuitTexture;
        }
        if point_in_rect(mouse, [px + 312.0, fy, (panel_w - 320.0).max(76.0), 17.0]) {
            let saved_keys = cfg.keys.clone();
            *cfg = FlyingWebConfig::default();
            if button == 1 {
                cfg.keys = saved_keys;
            }
            self.set_banner("Restored Defaults");
            return UiClickOutcome::ConfigChanged;
        }

        // Active tab widgets
        let widgets = Self::widgets_for_tab(self.active_tab, cfg, router);
        let start_y = tab_y + 22.0;
        let col_w = (panel_w - 24.0) * 0.5;
        let row_h = 20.0;

        for (i, w) in widgets.iter().enumerate() {
            let col = i % 2;
            let row = i / 2;
            let wx = px + 8.0 + (col as f32) * (col_w + 8.0);
            let wy = start_y + (row as f32) * (row_h + 4.0);
            if wy + row_h > py + panel_h - 26.0 {
                break;
            }
            let wrect = [wx, wy, col_w, row_h];
            if !point_in_rect(mouse, wrect) {
                continue;
            }

            match w {
                ControlWidget::Toggle { id, .. } => {
                    Self::flip_toggle(cfg, id);
                    cfg.sanitize();
                    return UiClickOutcome::ConfigChanged;
                }
                ControlWidget::Slider {
                    id, min, max, ..
                } => {
                    let frac = ((mouse[0] - wx) / col_w.max(1.0)).clamp(0.0, 1.0);
                    let new_val = *min + (*max - *min) * frac;
                    Self::set_slider(cfg, id, new_val);
                    cfg.sanitize();
                    return UiClickOutcome::ConfigChanged;
                }
                ControlWidget::CycleButton { id, .. } => {
                    Self::cycle_option(cfg, id);
                    cfg.sanitize();
                    return UiClickOutcome::ConfigChanged;
                }
                ControlWidget::KeybindRow { action, .. } => {
                    if button == 1 {
                        // Right-click restores default key for this action!
                        let def = FlyingWebConfig::default();
                        action.set_binding(&mut cfg.keys, action.binding(&def.keys).to_owned());
                        router.awaiting_rebind = None;
                        return UiClickOutcome::ConfigChanged;
                    }
                    router.awaiting_rebind = Some(*action);
                    return UiClickOutcome::None;
                }
            }
        }

        UiClickOutcome::None
    }

    fn widgets_for_tab(
        tab: SettingsTab,
        cfg: &FlyingWebConfig,
        router: &InputRouter,
    ) -> Vec<ControlWidget> {
        match tab {
            SettingsTab::Swinging => vec![
                ControlWidget::Toggle {
                    label: "Mod Enabled",
                    value: cfg.mod_enabled,
                    id: "mod_enabled",
                },
                ControlWidget::Toggle {
                    label: "Hold Key To Swing",
                    value: cfg.hold_to_swing,
                    id: "hold_to_swing",
                },
                ControlWidget::Slider {
                    label: "Max Web Range",
                    value: cfg.max_web_range as f32,
                    min: 20.0,
                    max: 88.0,
                    display: format!("{:.0}m", cfg.max_web_range),
                    id: "max_web_range",
                },
                ControlWidget::Slider {
                    label: "Cone Assist",
                    value: cfg.cone_search_degrees,
                    min: 0.0,
                    max: 36.0,
                    display: format!("{:.0}°", cfg.cone_search_degrees),
                    id: "cone_search_degrees",
                },
                ControlWidget::Slider {
                    label: "Max Swing Speed",
                    value: cfg.max_swing_speed as f32,
                    min: 1.0,
                    max: 3.4,
                    display: format!("{:.0} m/s", cfg.max_swing_speed * 20.0),
                    id: "max_swing_speed",
                },
                ControlWidget::Slider {
                    label: "Gravity Scale",
                    value: cfg.swing_gravity_scale as f32,
                    min: 0.5,
                    max: 1.8,
                    display: format!("{:.2}x", cfg.swing_gravity_scale),
                    id: "swing_gravity_scale",
                },
                ControlWidget::Slider {
                    label: "Swing Pump Force",
                    value: cfg.swing_pump_accel as f32,
                    min: 0.01,
                    max: 0.10,
                    display: format!("{:.3}", cfg.swing_pump_accel),
                    id: "swing_pump_accel",
                },
                ControlWidget::Slider {
                    label: "Jump Vault Boost",
                    value: cfg.release_jump_boost as f32,
                    min: 0.0,
                    max: 0.85,
                    display: format!("{:.2}", cfg.release_jump_boost),
                    id: "release_jump_boost",
                },
                ControlWidget::Toggle {
                    label: "Street-Skim Clearance",
                    value: cfg.auto_ground_clearance,
                    id: "auto_ground_clearance",
                },
                ControlWidget::Toggle {
                    label: "Web Slack Catenary",
                    value: cfg.web_sag_visuals,
                    id: "web_sag_visuals",
                },
            ],
            SettingsTab::Camera => vec![
                ControlWidget::Toggle {
                    label: "Dynamic Spider-Cam",
                    value: cfg.camera_enabled,
                    id: "camera_enabled",
                },
                ControlWidget::Toggle {
                    label: "Auto 3rd-Person",
                    value: cfg.auto_third_person,
                    id: "auto_third_person",
                },
                ControlWidget::Toggle {
                    label: "Keep 3rd-Person Ground",
                    value: cfg.keep_third_person_on_ground,
                    id: "keep_third_person_on_ground",
                },
                ControlWidget::Toggle {
                    label: "Wall Collision Guard",
                    value: cfg.camera_wall_collision,
                    id: "camera_wall_collision",
                },
                ControlWidget::Slider {
                    label: "Base Distance",
                    value: cfg.camera_base_distance,
                    min: 2.2,
                    max: 8.0,
                    display: format!("{:.1}m", cfg.camera_base_distance),
                    id: "camera_base_distance",
                },
                ControlWidget::Slider {
                    label: "Speed Pullback",
                    value: cfg.camera_speed_distance_gain,
                    min: 0.0,
                    max: 5.0,
                    display: format!("+{:.1}m", cfg.camera_speed_distance_gain),
                    id: "camera_speed_distance_gain",
                },
                ControlWidget::Slider {
                    label: "Camera Height",
                    value: cfg.camera_height_offset,
                    min: -0.2,
                    max: 1.2,
                    display: format!("{:+.2}m", cfg.camera_height_offset),
                    id: "camera_height_offset",
                },
                ControlWidget::Slider {
                    label: "Dynamic FOV",
                    value: cfg.dynamic_fov_strength,
                    min: 0.0,
                    max: 1.8,
                    display: format!("{:.0}%", cfg.dynamic_fov_strength * 100.0),
                    id: "dynamic_fov_strength",
                },
                ControlWidget::Slider {
                    label: "Follow Speed",
                    value: cfg.camera_follow_speed,
                    min: 4.0,
                    max: 24.0,
                    display: format!("{:.1}", cfg.camera_follow_speed),
                    id: "camera_follow_speed",
                },
                ControlWidget::Slider {
                    label: "Turn Roll Tilt",
                    value: cfg.camera_tilt_strength,
                    min: 0.0,
                    max: 1.8,
                    display: format!("{:.0}%", cfg.camera_tilt_strength * 100.0),
                    id: "camera_tilt_strength",
                },
                ControlWidget::Slider {
                    label: "Landing Settle",
                    value: cfg.camera_landing_settle,
                    min: 0.0,
                    max: 1.6,
                    display: format!("{:.0}%", cfg.camera_landing_settle * 100.0),
                    id: "camera_landing_settle",
                },
                ControlWidget::Slider {
                    label: "Effect Intensity",
                    value: cfg.camera_effect_intensity,
                    min: 0.0,
                    max: 1.6,
                    display: format!("{:.0}%", cfg.camera_effect_intensity * 100.0),
                    id: "camera_effect_intensity",
                },
            ],
            SettingsTab::Traversal => vec![
                ControlWidget::Toggle {
                    label: "Mid-Air Web Zip",
                    value: cfg.enable_web_zip,
                    id: "enable_web_zip",
                },
                ControlWidget::Toggle {
                    label: "Point Launch & Vault",
                    value: cfg.enable_point_launch,
                    id: "enable_point_launch",
                },
                ControlWidget::Toggle {
                    label: "Slingshot Launch",
                    value: cfg.enable_slingshot,
                    id: "enable_slingshot",
                },
                ControlWidget::Toggle {
                    label: "Web Wings Gliding",
                    value: cfg.enable_web_wings,
                    id: "enable_web_wings",
                },
                ControlWidget::Toggle {
                    label: "Air Tricks",
                    value: cfg.enable_air_tricks,
                    id: "enable_air_tricks",
                },
                ControlWidget::Toggle {
                    label: "Wall Running",
                    value: cfg.enable_wall_run,
                    id: "enable_wall_run",
                },
                ControlWidget::Toggle {
                    label: "Auto Wall-Run Latch",
                    value: cfg.auto_wall_run_attach,
                    id: "auto_wall_run_attach",
                },
                ControlWidget::Toggle {
                    label: "Wall Crawling",
                    value: cfg.enable_wall_crawl,
                    id: "enable_wall_crawl",
                },
                ControlWidget::Toggle {
                    label: "Ceiling Crawling",
                    value: cfg.enable_ceiling_crawl,
                    id: "enable_ceiling_crawl",
                },
            ],
            SettingsTab::Gadgets => vec![
                ControlWidget::Toggle {
                    label: "All Gadgets Enabled",
                    value: cfg.gadgets_enabled,
                    id: "gadgets_enabled",
                },
                ControlWidget::Toggle {
                    label: "Safe Fall Protection",
                    value: cfg.non_lethal_fall_protection,
                    id: "non_lethal_fall_protection",
                },
                ControlWidget::Toggle {
                    label: "Impact Web",
                    value: cfg.enable_impact_web,
                    id: "enable_impact_web",
                },
                ControlWidget::Toggle {
                    label: "Web Grabber",
                    value: cfg.enable_web_grabber,
                    id: "enable_web_grabber",
                },
                ControlWidget::Toggle {
                    label: "Web Bomb",
                    value: cfg.enable_web_bomb,
                    id: "enable_web_bomb",
                },
                ControlWidget::Toggle {
                    label: "Trip Mine",
                    value: cfg.enable_trip_mine,
                    id: "enable_trip_mine",
                },
                ControlWidget::Toggle {
                    label: "Remote Mine",
                    value: cfg.enable_remote_mine,
                    id: "enable_remote_mine",
                },
                ControlWidget::Toggle {
                    label: "Web Line (Tightrope)",
                    value: cfg.enable_web_line,
                    id: "enable_web_line",
                },
                ControlWidget::Toggle {
                    label: "Web Trap",
                    value: cfg.enable_web_trap,
                    id: "enable_web_trap",
                },
                ControlWidget::Toggle {
                    label: "Ricochet Web",
                    value: cfg.enable_ricochet_web,
                    id: "enable_ricochet_web",
                },
            ],
            SettingsTab::SuitAndFx => vec![
                ControlWidget::CycleButton {
                    label: "Suit Mode",
                    value_text: cfg.suit_mode.label().to_owned(),
                    id: "suit_mode",
                },
                ControlWidget::CycleButton {
                    label: "Suit Skin",
                    value_text: cfg.suit_variant.label().to_owned(),
                    id: "suit_variant",
                },
                ControlWidget::Toggle {
                    label: "Require Suit Equipped",
                    value: cfg.require_suit_for_abilities,
                    id: "require_suit_for_abilities",
                },
                ControlWidget::Toggle {
                    label: "2nd-Skin Overlay Layer",
                    value: cfg.show_second_skin_layer,
                    id: "show_second_skin_layer",
                },
                ControlWidget::Toggle {
                    label: "Show Wrist Shooters",
                    value: cfg.show_web_shooters,
                    id: "show_web_shooters",
                },
                ControlWidget::Toggle {
                    label: "Show 3D Web Wings",
                    value: cfg.show_web_wings_model,
                    id: "show_web_wings_model",
                },
                ControlWidget::CycleButton {
                    label: "HUD Style",
                    value_text: cfg.hud_style.label().to_owned(),
                    id: "hud_style",
                },
                ControlWidget::Slider {
                    label: "Strand Width",
                    value: cfg.web_strand_thickness,
                    min: 1.0,
                    max: 4.5,
                    display: format!("{:.1}px", cfg.web_strand_thickness),
                    id: "web_strand_thickness",
                },
                ControlWidget::Slider {
                    label: "Sound Volume",
                    value: cfg.sound_volume,
                    min: 0.0,
                    max: 1.5,
                    display: format!("{:.0}%", cfg.sound_volume * 100.0),
                    id: "sound_volume",
                },
                ControlWidget::Slider {
                    label: "Particle Density",
                    value: cfg.particle_density,
                    min: 0.0,
                    max: 1.8,
                    display: format!("{:.0}%", cfg.particle_density * 100.0),
                    id: "particle_density",
                },
            ],
            SettingsTab::Controls => SpiderAction::ALL
                .iter()
                .copied()
                .map(|action| ControlWidget::KeybindRow {
                    action,
                    bound_key: action.binding(&cfg.keys).to_owned(),
                    listening: router.awaiting_rebind == Some(action),
                })
                .collect(),
        }
    }

    fn flip_toggle(cfg: &mut FlyingWebConfig, id: &str) {
        match id {
            "mod_enabled" => cfg.mod_enabled = !cfg.mod_enabled,
            "hold_to_swing" => cfg.hold_to_swing = !cfg.hold_to_swing,
            "auto_ground_clearance" => cfg.auto_ground_clearance = !cfg.auto_ground_clearance,
            "web_sag_visuals" => cfg.web_sag_visuals = !cfg.web_sag_visuals,
            "camera_enabled" => cfg.camera_enabled = !cfg.camera_enabled,
            "auto_third_person" => cfg.auto_third_person = !cfg.auto_third_person,
            "keep_third_person_on_ground" => {
                cfg.keep_third_person_on_ground = !cfg.keep_third_person_on_ground
            }
            "camera_wall_collision" => cfg.camera_wall_collision = !cfg.camera_wall_collision,
            "enable_web_zip" => cfg.enable_web_zip = !cfg.enable_web_zip,
            "enable_point_launch" => cfg.enable_point_launch = !cfg.enable_point_launch,
            "enable_slingshot" => cfg.enable_slingshot = !cfg.enable_slingshot,
            "enable_web_wings" => cfg.enable_web_wings = !cfg.enable_web_wings,
            "enable_air_tricks" => cfg.enable_air_tricks = !cfg.enable_air_tricks,
            "enable_wall_run" => cfg.enable_wall_run = !cfg.enable_wall_run,
            "auto_wall_run_attach" => cfg.auto_wall_run_attach = !cfg.auto_wall_run_attach,
            "enable_wall_crawl" => cfg.enable_wall_crawl = !cfg.enable_wall_crawl,
            "enable_ceiling_crawl" => cfg.enable_ceiling_crawl = !cfg.enable_ceiling_crawl,
            "gadgets_enabled" => cfg.gadgets_enabled = !cfg.gadgets_enabled,
            "non_lethal_fall_protection" => {
                cfg.non_lethal_fall_protection = !cfg.non_lethal_fall_protection
            }
            "enable_impact_web" => cfg.enable_impact_web = !cfg.enable_impact_web,
            "enable_web_grabber" => cfg.enable_web_grabber = !cfg.enable_web_grabber,
            "enable_web_bomb" => cfg.enable_web_bomb = !cfg.enable_web_bomb,
            "enable_trip_mine" => cfg.enable_trip_mine = !cfg.enable_trip_mine,
            "enable_remote_mine" => cfg.enable_remote_mine = !cfg.enable_remote_mine,
            "enable_web_line" => cfg.enable_web_line = !cfg.enable_web_line,
            "enable_web_trap" => cfg.enable_web_trap = !cfg.enable_web_trap,
            "enable_ricochet_web" => cfg.enable_ricochet_web = !cfg.enable_ricochet_web,
            "require_suit_for_abilities" => {
                cfg.require_suit_for_abilities = !cfg.require_suit_for_abilities
            }
            "show_second_skin_layer" => cfg.show_second_skin_layer = !cfg.show_second_skin_layer,
            "show_web_shooters" => cfg.show_web_shooters = !cfg.show_web_shooters,
            "show_web_wings_model" => cfg.show_web_wings_model = !cfg.show_web_wings_model,
            _ => {}
        }
    }

    fn set_slider(cfg: &mut FlyingWebConfig, id: &str, v: f32) {
        match id {
            "max_web_range" => cfg.max_web_range = v as f64,
            "cone_search_degrees" => cfg.cone_search_degrees = v,
            "max_swing_speed" => cfg.max_swing_speed = v as f64,
            "swing_gravity_scale" => cfg.swing_gravity_scale = v as f64,
            "swing_pump_accel" => cfg.swing_pump_accel = v as f64,
            "release_jump_boost" => cfg.release_jump_boost = v as f64,
            "camera_base_distance" => cfg.camera_base_distance = v,
            "camera_speed_distance_gain" => cfg.camera_speed_distance_gain = v,
            "camera_height_offset" => cfg.camera_height_offset = v,
            "dynamic_fov_strength" => cfg.dynamic_fov_strength = v,
            "camera_follow_speed" => cfg.camera_follow_speed = v,
            "camera_tilt_strength" => cfg.camera_tilt_strength = v,
            "camera_landing_settle" => cfg.camera_landing_settle = v,
            "camera_effect_intensity" => cfg.camera_effect_intensity = v,
            "web_strand_thickness" => cfg.web_strand_thickness = v,
            "sound_volume" => cfg.sound_volume = v,
            "particle_density" => cfg.particle_density = v,
            _ => {}
        }
    }

    fn cycle_option(cfg: &mut FlyingWebConfig, id: &str) {
        match id {
            "suit_mode" => cfg.suit_mode = cfg.suit_mode.next(),
            "suit_variant" => cfg.suit_variant = cfg.suit_variant.next(),
            "hud_style" => cfg.hud_style = cfg.hud_style.next(),
            _ => {}
        }
    }
}

#[inline]
pub fn point_in_rect(pt: [f32; 2], rect: [f32; 4]) -> bool {
    pt[0] >= rect[0]
        && pt[0] <= rect[0] + rect[2]
        && pt[1] >= rect[1]
        && pt[1] <= rect[1] + rect[3]
}
