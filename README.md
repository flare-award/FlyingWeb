# FlyingWeb — Spider-Hero Traversal, Dynamic Camera & Gadget Suite for Horizon 26.3

**FlyingWeb** is a native Rust mod (`.hnmod`) for [**Horizon**](https://github.com/horizon-rust/horizon) (`26.3`), built from the ground up around **authentic, sub-stepped pendulum web-swinging**, a **frame-rate-independent dynamic Spider-Cam**, **acrobatic wall & rooftop parkour**, **8 non-lethal Spider-Gadgets**, and **swappable 64×64 Spider-Hero suits**.

Inspired by the traversal feel of *Marvel's Spider-Man* (2018 / 2023) and the customizable superhero gear philosophy of *Fisk's Superheroes*, FlyingWeb runs natively inside Horizon's Rust mixin runtime (`horizon-mixins`) with zero unsafe blocks, deterministic physics, and full singleplayer + Horizon-hosted multiplayer support.

---

## Core Highlights

1. **Sub-Stepped Pendulum Web Swinging (Pillar #1)**
   - **Real Overhead Anchor Discovery**: Every swing attaches to an actual loaded block surface in the world — never mid-air. A 3D Amanatides-Woo DDA raycaster + golden-angle forward/upward cone scorer evaluates candidate ledges, building corners, tree canopies, and bridges (including sub-block stairs, slabs, fences, walls, and leaves).
   - **4-Substep XPBD Rope Constraint Solver**: Unilateral taut/slack rope physics (`dist <= rest_length`) with velocity decomposition, subtle tangential steering pump, reel-in / let-out length control, timed bottom-of-arc release impulse, and natural ground skimming that converts low arcs into momentum instead of snapping off.
   - **3D Tapered Web Line Rendering**: Multi-strand silk lines with shoot-out tip extension, tension sag, subtle wind oscillation, and surface anchor splats rendered through `WorldPrimitives`.

2. **Cinematic Dynamic Spider-Cam (Pillar #2)**
   - **Automatic Third-Person Activation**: Seamlessly switches to `ThirdPersonBack` when swinging or wall-running and restores your previous camera perspective after landing.
   - **Velocity & Tension Tracking**: Speed-proportional pull-back (`4.0m` to `7.8m`), lateral banking roll (`±8.5°`), dive/soar pitch framing, look-ahead offset, apex float, and Dynamic FOV boost (`+16°` at terminal swing speed).
   - **5-Ray Anti-Clip Obstacle Feeler**: Asymmetric fast pull-in (`28 Hz`) and smooth recovery (`6 Hz`) prevent camera clipping inside narrow city alleys or forest canopies.
   - **Motion Sickness Friendly**: Roll, FOV boost, look-ahead, and camera shake can each be scaled independently or switched immediately to the **Comfort** preset.

3. **Full Traversal & Parkour Suite**
   - **Web Zip** (`C`): Quick aerial horizontal/upward burst toward a targeted surface or forward arc (up to 2 bursts per airtime).
   - **Point Launch + Perch Boost** (`X` -> `Space`): Zip directly onto ledges, lampposts, fences, or rooftops into a crouched Spider-Perch; press `Space` inside the timing window for a boosted super-vault.
   - **Wall Run & Wall Climb**: Hold `Sprint` against vertical walls to run horizontally across facades or ascend sheer towers, complete with angled **Wall Jump** (`Space`) and automatic **Corner Vault** over roof lips.
   - **Charged Super Jump** (`Sneak` + hold `Space`): Charge on the ground and launch up to ~12 blocks straight into swing height.
   - **Air Dive & Recovery** (`Left Ctrl`): Tuck into a steep high-speed dive to convert altitude into swing speed on last-second attach, with stylish superhero landing recovery and swing-earned fall-damage cushion.

4. **8 Non-Lethal Spider-Gadgets + Radial Wheel**
   - **Impact Web** — Instant single-target knockback + Webbed Cocoon snare (`4.5s`).
   - **Web Shooter Burst** — Rapid 3-shot web spray; 1 hit slows (`WebbedSlow`), 2+ hits cocoon (`WebbedCocoon`) or stick an already-webbed mob to an adjacent wall (`WallPinned`).
   - **Web Bomb** — Arcing proximity grenade bursting in a `4.5m` radius to web-snare nearby mobs.
   - **Suspension Matrix** — Lifts mobs in a `4.0m` radius into zero-gravity mid-air stasis (`3.5s`) before dropping them cushioned.
   - **Concussive Blast** — Directional `65°` sonic-air cone pushing enemies back without lethal damage or block griefing.
   - **Trip Mine** — Surface-mounted laser snare (`1.2s` arming delay) that pulls crossing enemies into a cocoon tether.
   - **Spider-Drone** — Hovering autonomous escort (`14s`) orbiting your shoulder and firing mini-webs at nearby threats.
   - **Electric Web** — Chain-stun web arcing across up to 4 nearby mobs (`4.5m` hop radius).

5. **Swappable Suits & Custom 64×64 PNG Support**
   - Ships with **4 original bundled Spider-Hero suits** (`Classic Red & Blue`, `Advanced White-Spider`, `Crimson & Obsidian`, `Midnight Symbiote`) plus **Vanilla Skin (Off)** mode.
   - Drop any standard **64×64 RGBA Minecraft skin PNG** into `<horizon_data_dir>/flyingweb/suits/<name>.png` and click **Reload Custom Suits** in the **Suits** tab — no recompilation required!

---

## Default Controls

All keybinds register natively with Horizon's Controls menu (`options.txt` persistence) and can also be rebound interactively inside FlyingWeb's **Controls** tab:

| Action | Default Key | Behavior |
| :--- | :--- | :--- |
| **Swing Shoot / Hold** | `R` (Hold) | Cast overhead web line; hold to swing, release to launch off arc |
| **Web Zip** | `C` | Quick aerial pull burst (up to 2 charges per airtime) |
| **Point Launch** | `X` | Precision zip to targeted ledge/post + timed `Space` Perch Boost |
| **Fire Active Gadget** | `G` | Fire currently selected Spider-Gadget (consumes Web Fluid) |
| **Gadget Wheel / Quick Cycle** | `V` (Hold / Tap) | Hold `V` + move mouse to pick from radial wheel; tap `V` to cycle next gadget; `Scroll Wheel` while holding `V` also cycles |
| **Cycle Spider-Suit** | `B` | Cycle `Classic` -> `Advanced` -> `Miles` -> `Symbiote` -> `Off` |
| **FlyingWeb Config Screen** | `K` | Open 6-tab interactive configuration & preset screen |
| **Reel In / Let Out Rope** | `Space` / `Left Shift` | Shorten or lengthen active web tether while swinging |
| **Air Dive** | `Left Ctrl` | Steep headfirst dive while airborne |

### Slash Commands

- `/flyingweb` (or `/fw`) — Open the 6-tab FlyingWeb configuration screen
- `/fw preset <cinematic|comfort|arcade|realistic>` — Apply a curated physics & camera preset immediately
- `/fw suit <classic|advanced|miles|symbiote|off>` — Switch active Spider-Suit
- `/fw reload` — Reload `config.json` and rescan custom 64×64 PNG suits on disk
- `/fw reset` — Reset all settings to balanced defaults

---

## Installation (Horizon 26.3)

1. Download `flyingweb-1.0.0.hnmod` from [GitHub Releases](../../releases) (or build from source using `python3 tools/horizon_ci.py`).
2. Place `flyingweb-1.0.0.hnmod` inside your Horizon profile's `mods/` directory (or import it via the Horizon Launcher **Mods** tab).
3. Launch **Horizon 26.3** (`fdc87aecdbd116bf1e08adcd062fcbaea8c2e2ca`). Horizon's native mod toolchain automatically verifies the `.hnmod` archive and compiles the mixin into the client.
4. Enter any world, look toward buildings, trees, or cliffs, jump, and hold **`R`** to swing! Press **`K`** at any time (or click **FlyingWeb** under `Mods`) to customize physics, camera feel, HUD, suits, or controls.

---

## Adding Custom Spider-Suit Skins (No Code Needed)

FlyingWeb uses the standard **64×64 RGBA Minecraft player skin UV layout** (head, hat/mask overlay, torso, arms, legs):

1. Launch Horizon once with FlyingWeb enabled so its data directory is created.
2. Locate `<horizon_profile>/mod_data/flyingweb/suits/` (or check the path printed by `/fw reload`).
3. Drop any `64×64` RGBA `.png` file into `suits/` (for example, `noir_spider.png` or `scarlet_spider.png`).
4. In-game, press **`K`** -> **Suits** tab -> **Reload Custom Suits** (or run `/fw reload`), then select your suit!

---

## Configuration & Presets

Settings persist automatically in `<horizon_config>/flyingweb/config.json` (mirrored to Horizon's mod KV store) with versioned schema migration and safe clamping. Four built-in presets are available with one click:

- **Cinematic** *(Default)* — Authentic pendulum weight, full Spider-Cam banking/pull-back/FOV, dynamic web lines, auto 3rd-person.
- **Comfort** — Zero camera roll, reduced FOV shift, minimal shake, steady framing for motion-sensitive players.
- **Arcade** — Forgiving wide anchor search cone, stronger swing pump & release boost, extra Web Zip charge, fast fluid regen.
- **Realistic** — Strict anchor geometry, pure gravity-driven conservation of momentum, zero artificial pump assist.

---

## Multiplayer Compatibility & Technical Notes

- **Singleplayer & Horizon Integrated Server**: Full traversal physics, custom Spider-Cam, suit rendering, HUD, and server-authoritative Spider-Gadget crowd control (`WebbedSlow`, `WebbedCocoon`, `WallPinned`, `Suspended`, `Shocked`, knockback, fall-damage cushioning) work out of the box over the `flyingweb:v1` custom payload channel.
- **Vanilla Dedicated Servers**: Client-side Spider-Cam, HUD, and local suit overlays remain non-crashing, while server-authoritative mob snare impulses and custom packet gadgets gracefully stay quiescent unless the server runs FlyingWeb's `flyingweb:v1` handler; conservative movement speeds stay bounded to avoid vanilla movement kicks.
- **Horizon Mixin Scope**: Horizon native mods compile against a pinned Horizon source revision (`compatibility.json`) for deterministic safety and zero-overhead `Game<'_>` hook integration. Note that Horizon's `native_sound` API plays built-in Minecraft sound events at modulated pitch/volume rather than arbitrary custom `.ogg` waveforms, so FlyingWeb synthesizes layered multi-pitch spatial soundscapes (`entity.fishing_bobber.throw`, `block.Cobweb.place`, `entity.breeze.wind_burst`, `item.trident.riptide_2`, etc.) paired with custom particle bursts.

---

## Building & Verifying Locally

### Quick Structural & Package Check (Python 3.10+ only)
```bash
python3 tools/verify_bundle.py check
python3 tools/verify_bundle.py package --out-dir dist
```

### Full Rust Suite & Horizon 26.3 Mixin Compilation
Requires Rust `1.95+` (Horizon Launcher uses `1.99.1`) and Linux audio/udev headers (`libasound2-dev libudev-dev pkg-config` on Ubuntu/Debian):
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python3 tools/horizon_ci.py --out-dir dist
```

---

## License

Released under the [MIT License](LICENSE). All code and bundled 64×64 pixel-art textures are original creations built specifically for FlyingWeb.
