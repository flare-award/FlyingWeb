use crate::config::KeybindConfig;

/// Logical Spider-Hero control actions decoupled from physical key names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpiderAction {
    ShootWeb,
    ReleaseWeb,
    WebZip,
    WebWings,
    AirTrick,
    WallCrawlToggle,
    UseGadget,
    CycleGadget,
    OpenSettings,
}

impl SpiderAction {
    pub const ALL: [Self; 9] = [
        Self::ShootWeb,
        Self::ReleaseWeb,
        Self::WebZip,
        Self::WebWings,
        Self::AirTrick,
        Self::WallCrawlToggle,
        Self::UseGadget,
        Self::CycleGadget,
        Self::OpenSettings,
    ];

    pub fn id_slug(self) -> &'static str {
        match self {
            Self::ShootWeb => "shoot_web",
            Self::ReleaseWeb => "release_web",
            Self::WebZip => "web_zip",
            Self::WebWings => "web_wings",
            Self::AirTrick => "air_trick",
            Self::WallCrawlToggle => "wall_crawl_toggle",
            Self::UseGadget => "use_gadget",
            Self::CycleGadget => "cycle_gadget",
            Self::OpenSettings => "open_settings",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ShootWeb => "Shoot Web / Swing",
            Self::ReleaseWeb => "Release Web / Detach",
            Self::WebZip => "Web Zip / Point Launch",
            Self::WebWings => "Deploy Web Wings",
            Self::AirTrick => "Air Tricks / Slingshot Charge",
            Self::WallCrawlToggle => "Toggle Wall / Ceiling Crawl",
            Self::UseGadget => "Fire Active Gadget",
            Self::CycleGadget => "Cycle Spider-Gadget",
            Self::OpenSettings => "FlyingWeb Settings Menu",
        }
    }

    pub fn binding<'a>(self, keys: &'a KeybindConfig) -> &'a str {
        match self {
            Self::ShootWeb => &keys.shoot_web,
            Self::ReleaseWeb => &keys.release_web,
            Self::WebZip => &keys.web_zip,
            Self::WebWings => &keys.web_wings,
            Self::AirTrick => &keys.air_trick,
            Self::WallCrawlToggle => &keys.wall_crawl_toggle,
            Self::UseGadget => &keys.use_gadget,
            Self::CycleGadget => &keys.cycle_gadget,
            Self::OpenSettings => &keys.open_settings,
        }
    }

    pub fn set_binding(self, keys: &mut KeybindConfig, new_key: String) {
        match self {
            Self::ShootWeb => keys.shoot_web = new_key,
            Self::ReleaseWeb => keys.release_web = new_key,
            Self::WebZip => keys.web_zip = new_key,
            Self::WebWings => keys.web_wings = new_key,
            Self::AirTrick => keys.air_trick = new_key,
            Self::WallCrawlToggle => keys.wall_crawl_toggle = new_key,
            Self::UseGadget => keys.use_gadget = new_key,
            Self::CycleGadget => keys.cycle_gadget = new_key,
            Self::OpenSettings => keys.open_settings = new_key,
        }
    }
}

/// Which wrist fired the most recent web line, alternating naturally like Spider-Man.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WristSide {
    #[default]
    Right,
    Left,
}

impl WristSide {
    pub fn flip(self) -> Self {
        match self {
            Self::Right => Self::Left,
            Self::Left => Self::Right,
        }
    }

    /// Lateral shoulder offset sign (+1 right, -1 left in player right-vector space).
    pub fn sign(self) -> f64 {
        match self {
            Self::Right => 1.0,
            Self::Left => -1.0,
        }
    }
}

/// Per-tick snapshot of directional + edge-triggered Spider-Hero inputs.
#[derive(Clone, Copy, Debug, Default)]
pub struct TickInput {
    /// Forward/back movement axis (-1.0..=1.0, +1 is W/forward).
    pub forward: f32,
    /// Strafe axis (-1.0..=1.0, +1 is A/left in Horizon's `native_movement_input`).
    pub strafe: f32,
    pub jump_held: bool,
    pub jump_just_pressed: bool,
    pub sneak_held: bool,
    pub sneak_just_pressed: bool,

    pub shoot_held: bool,
    pub shoot_just_pressed: bool,
    pub shoot_just_released: bool,

    pub release_just_pressed: bool,
    pub zip_just_pressed: bool,
    pub wings_just_pressed: bool,

    pub trick_held: bool,
    pub trick_just_pressed: bool,

    pub crawl_toggle_just_pressed: bool,
    pub gadget_use_just_pressed: bool,
    pub gadget_cycle_just_pressed: bool,
    pub open_settings_just_pressed: bool,

    /// Accumulated scroll-wheel steps since the previous tick (+ up / reel-in, - down / pay-out).
    pub wheel_delta: f32,
}

/// Tracks live key holds and latches sub-tick key edges until consumed by the 20 Hz simulation tick.
#[derive(Clone, Debug, Default)]
pub struct InputRouter {
    held: [bool; 9],
    just_pressed: [bool; 9],
    just_released: [bool; 9],

    prev_jump: bool,
    prev_sneak: bool,
    latest_forward: f32,
    latest_strafe: f32,
    latest_jump: bool,
    latest_sneak: bool,
    wheel_accum: f32,

    /// If `Some(action)`, the settings UI is listening for the next key press to rebind `action`.
    pub awaiting_rebind: Option<SpiderAction>,
}

impl InputRouter {
    fn index(action: SpiderAction) -> usize {
        match action {
            SpiderAction::ShootWeb => 0,
            SpiderAction::ReleaseWeb => 1,
            SpiderAction::WebZip => 2,
            SpiderAction::WebWings => 3,
            SpiderAction::AirTrick => 4,
            SpiderAction::WallCrawlToggle => 5,
            SpiderAction::UseGadget => 6,
            SpiderAction::CycleGadget => 7,
            SpiderAction::OpenSettings => 8,
        }
    }

    pub fn is_held(&self, action: SpiderAction) -> bool {
        self.held[Self::index(action)]
    }

    /// Feed a raw `native_key(key, pressed)` event. Returns `true` if consumed by a live rebind.
    pub fn on_key_event(&mut self, key: &str, pressed: bool, keys: &mut KeybindConfig) -> bool {
        if let Some(target_action) = self.awaiting_rebind {
            if pressed {
                if key != "Escape" {
                    target_action.set_binding(keys, key.to_owned());
                }
                self.awaiting_rebind = None;
            }
            return true;
        }

        for action in SpiderAction::ALL {
            if action.binding(keys).eq_ignore_ascii_case(key) {
                self.set_action_state(action, pressed);
            }
        }
        false
    }

    pub fn set_action_state(&mut self, action: SpiderAction, pressed: bool) {
        let idx = Self::index(action);
        if pressed && !self.held[idx] {
            self.just_pressed[idx] = true;
        } else if !pressed && self.held[idx] {
            self.just_released[idx] = true;
        }
        self.held[idx] = pressed;
    }

    /// Feed raw movement axes from `native_movement_input`.
    pub fn record_movement_axes(&mut self, forward: f32, strafe: f32, jump: bool, sneak: bool) {
        self.latest_forward = forward.clamp(-1.0, 1.0);
        self.latest_strafe = strafe.clamp(-1.0, 1.0);
        self.latest_jump = jump;
        self.latest_sneak = sneak;
    }

    /// Feed scroll-wheel steps (`+` scroll up, `-` scroll down).
    pub fn record_wheel(&mut self, steps: f32) {
        if steps.is_finite() {
            self.wheel_accum = (self.wheel_accum + steps).clamp(-12.0, 12.0);
        }
    }

    /// Consume latched edges into a `TickInput` for one 20 Hz simulation step.
    pub fn consume_tick(&mut self) -> TickInput {
        let jump_just_pressed = self.latest_jump && !self.prev_jump;
        let sneak_just_pressed = self.latest_sneak && !self.prev_sneak;
        self.prev_jump = self.latest_jump;
        self.prev_sneak = self.latest_sneak;

        let jp = std::mem::take(&mut self.just_pressed);
        let jr = std::mem::take(&mut self.just_released);
        let wheel = std::mem::replace(&mut self.wheel_accum, 0.0);

        TickInput {
            forward: self.latest_forward,
            strafe: self.latest_strafe,
            jump_held: self.latest_jump,
            jump_just_pressed,
            sneak_held: self.latest_sneak,
            sneak_just_pressed,

            shoot_held: self.held[Self::index(SpiderAction::ShootWeb)],
            shoot_just_pressed: jp[Self::index(SpiderAction::ShootWeb)],
            shoot_just_released: jr[Self::index(SpiderAction::ShootWeb)],

            release_just_pressed: jp[Self::index(SpiderAction::ReleaseWeb)],
            zip_just_pressed: jp[Self::index(SpiderAction::WebZip)],
            wings_just_pressed: jp[Self::index(SpiderAction::WebWings)],

            trick_held: self.held[Self::index(SpiderAction::AirTrick)],
            trick_just_pressed: jp[Self::index(SpiderAction::AirTrick)],

            crawl_toggle_just_pressed: jp[Self::index(SpiderAction::WallCrawlToggle)],
            gadget_use_just_pressed: jp[Self::index(SpiderAction::UseGadget)],
            gadget_cycle_just_pressed: jp[Self::index(SpiderAction::CycleGadget)],
            open_settings_just_pressed: jp[Self::index(SpiderAction::OpenSettings)],

            wheel_delta: wheel,
        }
    }

    /// Release all held actions and transient latches (on screen open, death, or world exit).
    pub fn reset_all(&mut self) {
        self.held = [false; 9];
        self.just_pressed = [false; 9];
        self.just_released = [false; 9];
        self.prev_jump = false;
        self.prev_sneak = false;
        self.latest_forward = 0.0;
        self.latest_strafe = 0.0;
        self.latest_jump = false;
        self.latest_sneak = false;
        self.wheel_accum = 0.0;
        self.awaiting_rebind = None;
    }
}
