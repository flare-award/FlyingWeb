use serde::{Deserialize, Serialize};

/// How the Spider-Hero suit renders on the player character.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuitRenderMode {
    /// Sleek FiskHeroes-style 64x64 skin-layer suit + 0.25-texel second-skin overlay
    /// with full articulated Spider-Hero animations (replaces bulky armor rendering).
    #[default]
    SkinSuit,
    /// Keeps the player's vanilla skin and overlays slim wrist web-shooters + Web Wings.
    WebShootersOnly,
    /// Hides suit cosmetics completely while keeping abilities active if enabled.
    Hidden,
}

impl SuitRenderMode {
    pub fn next(self) -> Self {
        match self {
            Self::SkinSuit => Self::WebShootersOnly,
            Self::WebShootersOnly => Self::Hidden,
            Self::Hidden => Self::SkinSuit,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::SkinSuit => "Skin-Layer Suit (Fisk-Style)",
            Self::WebShootersOnly => "Wrist Shooters Overlay",
            Self::Hidden => "Hidden",
        }
    }
}

/// Built-in original 64x64 suit textures plus user-replaceable external PNG option.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuitVariant {
    #[default]
    Classic,
    Advanced,
    Miles,
    Symbiote,
    CustomFile,
}

impl SuitVariant {
    pub fn next(self) -> Self {
        match self {
            Self::Classic => Self::Advanced,
            Self::Advanced => Self::Miles,
            Self::Miles => Self::Symbiote,
            Self::Symbiote => Self::CustomFile,
            Self::CustomFile => Self::Classic,
        }
    }

    pub fn filename(self, custom: &str) -> &str {
        match self {
            Self::Classic => "classic_suit.png",
            Self::Advanced => "advanced_suit.png",
            Self::Miles => "miles_suit.png",
            Self::Symbiote => "symbiote_suit.png",
            Self::CustomFile => {
                if custom.trim().is_empty() {
                    "custom_suit.png"
                } else {
                    custom.trim()
                }
            }
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Classic => "Classic Crimson-Cobalt",
            Self::Advanced => "Advanced White-Spider",
            Self::Miles => "Crimson-Noir",
            Self::Symbiote => "Obsidian Symbiote",
            Self::CustomFile => "Custom PNG (suits/custom_suit.png)",
        }
    }

    /// Web strand primary ARGB tint matched to the suit aesthetic.
    pub fn web_tint_argb(self) -> u32 {
        match self {
            Self::Symbiote => 0xF024_2634,
            _ => 0xF2F5_F9FF,
        }
    }
}

/// HUD presentation density.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudStyle {
    #[default]
    Full,
    Minimal,
    Hidden,
}

impl HudStyle {
    pub fn next(self) -> Self {
        match self {
            Self::Full => Self::Minimal,
            Self::Minimal => Self::Hidden,
            Self::Hidden => Self::Full,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Full => "Full Spider-HUD",
            Self::Minimal => "Minimal Reticle",
            Self::Hidden => "Hidden",
        }
    }
}

/// Quick presets for physics + Spider-Cam tuning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TuningPreset {
    InsomniacCinematic,
    ClassicAthletic,
    ComfortLowMotion,
}

impl TuningPreset {
    pub fn label(self) -> &'static str {
        match self {
            Self::InsomniacCinematic => "Insomniac Cinematic",
            Self::ClassicAthletic => "Classic Athletic",
            Self::ComfortLowMotion => "Comfort (Low Motion)",
        }
    }
}

/// Rebindable key names matching Horizon's `native_key` / `native_mod_key` identifiers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeybindConfig {
    pub shoot_web: String,
    pub release_web: String,
    pub web_zip: String,
    pub web_wings: String,
    pub air_trick: String,
    pub wall_crawl_toggle: String,
    pub use_gadget: String,
    pub cycle_gadget: String,
    pub open_settings: String,
}

impl Default for KeybindConfig {
    fn default() -> Self {
        Self {
            shoot_web: "KeyR".to_owned(),
            release_web: "KeyX".to_owned(),
            web_zip: "KeyF".to_owned(),
            web_wings: "KeyG".to_owned(),
            air_trick: "KeyC".to_owned(),
            wall_crawl_toggle: "KeyV".to_owned(),
            use_gadget: "KeyB".to_owned(),
            cycle_gadget: "KeyN".to_owned(),
            open_settings: "KeyK".to_owned(),
        }
    }
}

/// Persistent configuration for FlyingWeb (`flyingweb.json`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FlyingWebConfig {
    // --- Master & Suit Gating ---
    pub mod_enabled: bool,
    pub require_suit_for_abilities: bool,

    // --- Web Swinging & Pendulum Physics ---
    pub hold_to_swing: bool,
    pub max_web_range: f64,
    pub min_web_range: f64,
    pub cone_search_degrees: f32,
    pub swing_gravity_scale: f64,
    pub web_stiffness: f64,
    pub swing_pump_accel: f64,
    pub air_steer_accel: f64,
    pub reel_speed: f64,
    pub release_jump_boost: f64,
    pub max_swing_speed: f64,
    pub aerodynamic_drag: f64,
    pub auto_ground_clearance: bool,

    // --- Third-Person Dynamic Spider-Cam (Critical Priority) ---
    pub camera_enabled: bool,
    pub auto_third_person: bool,
    pub keep_third_person_on_ground: bool,
    pub camera_base_distance: f32,
    pub camera_speed_distance_gain: f32,
    pub camera_height_offset: f32,
    pub camera_shoulder_offset: f32,
    pub camera_sensitivity_scale: f32,
    pub dynamic_fov_strength: f32,
    pub camera_follow_speed: f32,
    pub camera_arc_pitch_tracking: f32,
    pub camera_tilt_strength: f32,
    pub camera_inertia: f32,
    pub camera_landing_settle: f32,
    pub camera_effect_intensity: f32,
    pub camera_wall_collision: bool,

    // --- Additional Traversal Mechanics ---
    pub enable_web_zip: bool,
    pub enable_point_launch: bool,
    pub enable_slingshot: bool,
    pub enable_web_wings: bool,
    pub enable_air_tricks: bool,
    pub enable_wall_run: bool,
    pub enable_wall_crawl: bool,
    pub enable_ceiling_crawl: bool,
    pub auto_wall_run_attach: bool,

    // --- Spider-Gadgets (Master + Individual Toggles) ---
    pub gadgets_enabled: bool,
    pub enable_web_grabber: bool,
    pub enable_impact_web: bool,
    pub enable_web_bomb: bool,
    pub enable_trip_mine: bool,
    pub enable_remote_mine: bool,
    pub enable_web_line: bool,
    pub enable_web_trap: bool,
    pub enable_ricochet_web: bool,
    pub non_lethal_fall_protection: bool,

    // --- Suit, Web Strand Visuals, Audio & HUD ---
    pub suit_mode: SuitRenderMode,
    pub suit_variant: SuitVariant,
    pub custom_suit_filename: String,
    pub show_second_skin_layer: bool,
    pub show_web_shooters: bool,
    pub show_web_wings_model: bool,
    pub web_strand_thickness: f32,
    pub web_sag_visuals: bool,
    pub arm_shoot_animation: bool,
    pub sound_volume: f32,
    pub particle_density: f32,
    pub hud_style: HudStyle,

    // --- Keybinds ---
    pub keys: KeybindConfig,
}

impl Default for FlyingWebConfig {
    fn default() -> Self {
        Self {
            mod_enabled: true,
            require_suit_for_abilities: true,

            hold_to_swing: true,
            max_web_range: 54.0,
            min_web_range: 2.5,
            cone_search_degrees: 22.0,
            swing_gravity_scale: 1.0,
            web_stiffness: 0.94,
            swing_pump_accel: 0.050,
            air_steer_accel: 0.034,
            reel_speed: 0.45,
            release_jump_boost: 0.44,
            max_swing_speed: 2.35,
            aerodynamic_drag: 0.0085,
            auto_ground_clearance: true,

            camera_enabled: true,
            auto_third_person: true,
            keep_third_person_on_ground: false,
            camera_base_distance: 4.2,
            camera_speed_distance_gain: 2.6,
            camera_height_offset: 0.42,
            camera_shoulder_offset: 0.22,
            camera_sensitivity_scale: 1.0,
            dynamic_fov_strength: 1.0,
            camera_follow_speed: 12.0,
            camera_arc_pitch_tracking: 0.55,
            camera_tilt_strength: 1.0,
            camera_inertia: 0.65,
            camera_landing_settle: 0.85,
            camera_effect_intensity: 1.0,
            camera_wall_collision: true,

            enable_web_zip: true,
            enable_point_launch: true,
            enable_slingshot: true,
            enable_web_wings: true,
            enable_air_tricks: true,
            enable_wall_run: true,
            enable_wall_crawl: true,
            enable_ceiling_crawl: true,
            auto_wall_run_attach: true,

            gadgets_enabled: true,
            enable_web_grabber: true,
            enable_impact_web: true,
            enable_web_bomb: true,
            enable_trip_mine: true,
            enable_remote_mine: true,
            enable_web_line: true,
            enable_web_trap: true,
            enable_ricochet_web: true,
            non_lethal_fall_protection: true,

            suit_mode: SuitRenderMode::SkinSuit,
            suit_variant: SuitVariant::Classic,
            custom_suit_filename: "custom_suit.png".to_owned(),
            show_second_skin_layer: true,
            show_web_shooters: true,
            show_web_wings_model: true,
            web_strand_thickness: 2.2,
            web_sag_visuals: true,
            arm_shoot_animation: true,
            sound_volume: 0.90,
            particle_density: 1.0,
            hud_style: HudStyle::Full,

            keys: KeybindConfig::default(),
        }
    }
}

fn clean_f64(v: f64, default: f64, min: f64, max: f64) -> f64 {
    if v.is_finite() {
        v.clamp(min, max)
    } else {
        default
    }
}

fn clean_f32(v: f32, default: f32, min: f32, max: f32) -> f32 {
    if v.is_finite() {
        v.clamp(min, max)
    } else {
        default
    }
}

impl FlyingWebConfig {
    /// Whether Spider-Hero abilities are currently permitted by suit + master toggles.
    pub fn abilities_active(&self) -> bool {
        if !self.mod_enabled {
            return false;
        }
        if self.require_suit_for_abilities {
            self.suit_mode != SuitRenderMode::Hidden
        } else {
            true
        }
    }

    /// Clamps every numeric field to safe physical bounds and rejects NaN/Inf.
    pub fn sanitize(&mut self) {
        let d = Self::default();
        self.min_web_range = clean_f64(self.min_web_range, d.min_web_range, 1.5, 12.0);
        self.max_web_range = clean_f64(
            self.max_web_range,
            d.max_web_range,
            self.min_web_range + 6.0,
            96.0,
        );
        self.cone_search_degrees =
            clean_f32(self.cone_search_degrees, d.cone_search_degrees, 0.0, 42.0);
        self.swing_gravity_scale =
            clean_f64(self.swing_gravity_scale, d.swing_gravity_scale, 0.35, 2.2);
        self.web_stiffness = clean_f64(self.web_stiffness, d.web_stiffness, 0.45, 1.0);
        self.swing_pump_accel =
            clean_f64(self.swing_pump_accel, d.swing_pump_accel, 0.0, 0.12);
        self.air_steer_accel =
            clean_f64(self.air_steer_accel, d.air_steer_accel, 0.0, 0.09);
        self.reel_speed = clean_f64(self.reel_speed, d.reel_speed, 0.05, 1.5);
        self.release_jump_boost =
            clean_f64(self.release_jump_boost, d.release_jump_boost, 0.0, 1.0);
        self.max_swing_speed = clean_f64(self.max_swing_speed, d.max_swing_speed, 0.85, 3.60);
        self.aerodynamic_drag =
            clean_f64(self.aerodynamic_drag, d.aerodynamic_drag, 0.002, 0.05);

        self.camera_base_distance =
            clean_f32(self.camera_base_distance, d.camera_base_distance, 2.0, 9.0);
        self.camera_speed_distance_gain = clean_f32(
            self.camera_speed_distance_gain,
            d.camera_speed_distance_gain,
            0.0,
            6.0,
        );
        self.camera_height_offset = clean_f32(
            self.camera_height_offset,
            d.camera_height_offset,
            -0.4,
            1.6,
        );
        self.camera_shoulder_offset = clean_f32(
            self.camera_shoulder_offset,
            d.camera_shoulder_offset,
            -1.2,
            1.2,
        );
        self.camera_sensitivity_scale = clean_f32(
            self.camera_sensitivity_scale,
            d.camera_sensitivity_scale,
            0.25,
            2.5,
        );
        self.dynamic_fov_strength =
            clean_f32(self.dynamic_fov_strength, d.dynamic_fov_strength, 0.0, 2.0);
        self.camera_follow_speed =
            clean_f32(self.camera_follow_speed, d.camera_follow_speed, 3.0, 28.0);
        self.camera_arc_pitch_tracking = clean_f32(
            self.camera_arc_pitch_tracking,
            d.camera_arc_pitch_tracking,
            0.0,
            1.2,
        );
        self.camera_tilt_strength =
            clean_f32(self.camera_tilt_strength, d.camera_tilt_strength, 0.0, 2.0);
        self.camera_inertia = clean_f32(self.camera_inertia, d.camera_inertia, 0.0, 1.5);
        self.camera_landing_settle = clean_f32(
            self.camera_landing_settle,
            d.camera_landing_settle,
            0.0,
            2.0,
        );
        self.camera_effect_intensity = clean_f32(
            self.camera_effect_intensity,
            d.camera_effect_intensity,
            0.0,
            2.0,
        );

        self.web_strand_thickness =
            clean_f32(self.web_strand_thickness, d.web_strand_thickness, 0.8, 5.0);
        self.sound_volume = clean_f32(self.sound_volume, d.sound_volume, 0.0, 1.5);
        self.particle_density = clean_f32(self.particle_density, d.particle_density, 0.0, 2.0);

        if self.custom_suit_filename.trim().is_empty()
            || self.custom_suit_filename.contains("..")
            || self.custom_suit_filename.contains('/')
            || self.custom_suit_filename.contains('\\')
        {
            self.custom_suit_filename = d.custom_suit_filename;
        }
    }

    pub fn apply_preset(&mut self, preset: TuningPreset) {
        match preset {
            TuningPreset::InsomniacCinematic => {
                let d = Self::default();
                self.max_swing_speed = d.max_swing_speed;
                self.swing_pump_accel = d.swing_pump_accel;
                self.camera_base_distance = 4.2;
                self.camera_speed_distance_gain = 2.6;
                self.dynamic_fov_strength = 1.0;
                self.camera_tilt_strength = 1.0;
                self.camera_inertia = 0.65;
                self.camera_landing_settle = 0.85;
                self.camera_effect_intensity = 1.0;
            }
            TuningPreset::ClassicAthletic => {
                self.max_swing_speed = 2.60;
                self.swing_pump_accel = 0.056;
                self.camera_base_distance = 4.6;
                self.camera_speed_distance_gain = 2.0;
                self.dynamic_fov_strength = 0.85;
                self.camera_tilt_strength = 0.70;
                self.camera_inertia = 0.45;
                self.camera_landing_settle = 0.65;
                self.camera_effect_intensity = 0.85;
            }
            TuningPreset::ComfortLowMotion => {
                self.camera_base_distance = 4.5;
                self.camera_speed_distance_gain = 1.0;
                self.dynamic_fov_strength = 0.20;
                self.camera_tilt_strength = 0.0;
                self.camera_inertia = 0.20;
                self.camera_landing_settle = 0.25;
                self.camera_effect_intensity = 0.30;
                self.camera_arc_pitch_tracking = 0.20;
            }
        }
        self.sanitize();
    }

    pub fn from_json_lossless(json: &str) -> Self {
        let mut parsed: Self = serde_json::from_str(json).unwrap_or_default();
        parsed.sanitize();
        parsed
    }

    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_owned())
    }
}
