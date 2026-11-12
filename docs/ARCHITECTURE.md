# FlyingWeb Architecture & Physics Design

This document details how **FlyingWeb** integrates into **Horizon 26.3** (`fdc87aecdbd116bf1e08adcd062fcbaea8c2e2ca`, `NATIVE_HOOKS_API = 411`) and how its core simulation, camera, gadget, rendering, and network systems are structured.

---

## 1. Why a Native Horizon Mixin (`[[crate]]` + `[[module]]` + `[[injection]]`)?

Horizon supports two mod tiers:
- **Content/WASM plugins** for declarative items/blocks or coarse script callbacks.
- **Native Rust Mixins (`.hnmod`)** compiled directly into `horizon-client` via `horizon-mixins`.

Authentic pendulum web-swinging, sub-frame camera choreography (`native_camera_pose`, `native_camera_distance`, `native_compute_fov`, `native_camera_shake`), 3D world-space web strand rendering (`native_world_primitives`), custom 3D player/mob quad submission (`native_player_submit`, `native_mob_layer`), and 256×256 sprite atlas HUD rendering (`MOD_IMAGE_SPRITE`) require frame-accurate access to `horizon_client::game::Game<'_>` hooks.

FlyingWeb isolates **92% of its logic** inside a pure, engine-independent Rust crate (`mixins/flyingweb/flyingweb_core`) and keeps the Horizon bridge (`mixins/flyingweb/glue.rs` + `mixins/flyingweb/hooks/*.rs`) thin, defensive, and free of `unsafe` code:

```text
mixins/flyingweb/
├── horizon-mixin.toml        # Declares crate `flyingweb_core`, module `flyingweb_glue`, and 34 HEAD hooks
├── glue.rs                   # Thin Horizon 26.3 adapter (mounted as `crate::game::flyingweb_glue`)
├── hooks/*.rs                # Single-function injection shims forwarding into `flyingweb_glue`
└── flyingweb_core/           # Pure-Rust simulation, raycaster, camera, gadgets, animations, suit & UI
    ├── Cargo.toml            # Allowlisted dependencies only: bitflags, glam, png, rustc-hash, serde, serde_json
    ├── data/suits/*.png      # 4 original 64x64 RGBA Spider-Suit textures
    └── src/
        ├── config.rs         # Versioned JSON config + 4 curated presets (Cinematic/Comfort/Arcade/Realistic)
        ├── input.rs          # Edge-triggered actions, hold durations, coyote & jump-buffer timers
        ├── raycast.rs        # 3D Amanatides-Woo DDA voxel traversal + sub-block AABB + cone anchor scorer
        ├── physics.rs        # 4-substep XPBD unilateral rope solver, drag, tangential steering & release boost
        ├── swinging.rs       # Dual-hand web shooter state machine, anchor lifecycle & fall-damage cushion
        ├── web.rs            # Tapered 3D silk quad geometry, tip shoot-out, sag, wind & anchor splats
        ├── camera.rs         # Frame-rate-independent Spider-Cam + 5-ray obstacle frustum feeler
        ├── movement.rs       # Unified traversal FSM (Swinging, WebZip, PointLaunch, Perch, WallRun/Climb, Dive)
        ├── gadgets.rs        # 8 non-lethal Spider-Gadgets, deterministic client/server simulation & net codec
        ├── animations.rs     # 6-DOF limb rig poses & first-person web-shooter hand gestures
        ├── suit.rs           # 64x64 PNG validator/loader + 256x256 RGBA runtime HUD/suit texture atlas
        ├── effects.rs        # Layered vanilla soundscapes, particle emitters & camera shake envelopes
        ├── ui.rs             # Minimal curved-arc HUD, 8-sector radial Gadget Wheel & 6-tab config GUI
        └── tests.rs          # Deterministic unit & integration test suite
```

---

## 2. Sub-Stepped Pendulum Physics (`physics.rs` & `swinging.rs`)

Horizon steps `PlayerMotion` at **20 Hz** (`dt = 0.05s`) inside `native_movement_input`. Solving a stiff rope constraint once per 50 ms tick introduces artificial energy drift and chord tunneling. FlyingWeb divides every tick into **`N = 4` sub-steps** (`h = 0.0125s`):

1. **External Forces**: Applies gravity $\mathbf{g} = (0, -28.0, 0)\,\text{m/s}^2$, quadratic aerodynamic drag, and subtle camera-relative horizontal air-steering decomposed into the plane perpendicular to the rope unit vector $\hat{\mathbf{n}} = (\mathbf{p}_{\text{shoulder}} - \mathbf{a}) / \|\mathbf{p}_{\text{shoulder}} - \mathbf{a}\|$.
2. **Semi-Implicit Trial Step**: $\mathbf{p}^* = \mathbf{p} + \mathbf{v} h$.
3. **Unilateral XPBD Rope Projection**:
   - If $\|\mathbf{p}^*_{\text{shoulder}} - \mathbf{a}\| \le L$, the web line is **slack** — the player arcs freely under gravity without rigid rod pushback.
   - If $\|\mathbf{p}^*_{\text{shoulder}} - \mathbf{a}\| > L$, position is projected onto the sphere of radius $L$ with mild silk compliance, and any outward radial velocity $(\mathbf{v} \cdot \hat{\mathbf{n}} > 0)$ is removed cleanly while preserving tangential speed $\mathbf{v}_t = \mathbf{v} - (\mathbf{v} \cdot \hat{\mathbf{n}})\hat{\mathbf{n}}$.
4. **Pendulum Pump & Release Timing**:
   - Holding forward near the bottom half of the arc ($n_y < -0.35$) adds a bounded tangential acceleration along the swing direction.
   - Releasing `R` while swinging upward after the nadir ($-0.95 < n_y < -0.15$ and $v_y > -1.0$) awards a smooth forward + upward release boost proportional to tangential momentum.
5. **Ground Skimming**: Brushing blocks within `< 0.6m` while moving horizontally at high speed cushions downward velocity and shortens $L$ slightly rather than snapping the tether immediately.

---

## 3. Overhead Anchor Discovery (`raycast.rs`)

Mid-air sky-hooks are strictly forbidden (`find_best_swing_anchor` returns `None` over empty plains). When the player presses `R`:

1. A golden-angle spiral cone (33 candidate rays up to `config.max_web_range`, biased `18°–55°` above horizon and slightly toward the alternating left/right shooting hand) traverses loaded chunks via `WorldView` (`BlockCollisionKind`: `FullCube`, `BottomSlab`, `TopSlab`, `TallBlock`, `PostOrWall`, `FoilageAnchor`).
2. Each hit candidate at $\mathbf{a}$ is scored across five geometric terms:
   $$\text{Score}(\mathbf{a}) = w_{\text{align}} S_{\text{align}} + w_{\text{elev}} S_{\text{elev}} + w_{\text{dist}} S_{\text{dist}} + w_{\text{clear}} S_{\text{clear}} + w_{\text{side}} S_{\text{side}}$$
   where $S_{\text{elev}}$ rejects anchors below shoulder height unless emergency-recovering from a deep fall, $S_{\text{dist}}$ peaks in the golden pendulum band (`14m–32m`), and $S_{\text{clear}}$ probes below $\mathbf{a}$ to verify clean swing-under clearance.

---

## 4. Frame-Rate-Independent Dynamic Spider-Cam (`camera.rs`)

Horizon renders frames decoupled from the 20 Hz tick loop (`native_frame(dt)` -> `native_camera_distance`, `native_camera_pose`, `native_compute_fov`, `native_camera_shake`). Every camera channel uses exact exponential decay:
$$\alpha(k, \Delta t) = 1 - e^{-k \Delta t}$$
producing identical trajectories at 30 FPS, 60 FPS, 144 FPS, and 240 FPS.

- **Auto Third-Person**: Activates `CameraType::ThirdPersonBack` when entering `Swinging`, `WebZip`, `PointLaunch`, or `WallRun` and restores the user's prior `CameraType` after `0.65s` grounded settle time.
- **Asymmetric Obstacle Feeler**: Casts 5 rays from the player's eye pivot toward the desired boom camera position (center + 4 frustum corners). Obstructions pull the boom inward rapidly ($k_{\text{in}} = 28\,\text{s}^{-1}$) and ease back smoothly ($k_{\text{out}} = 6\,\text{s}^{-1}$).

---

## 5. Texture Atlas & Custom 3D Rendering (`suit.rs`, `web.rs`, `glue.rs`)

Horizon exposes a single dynamic RGBA sprite slot per native mod (`MOD_IMAGE_SPRITE`). FlyingWeb packs a **256×256 RGBA atlas** at runtime:
- **Top-Left `(0, 0, 64, 64)`**: Active Spider-Suit 64×64 Minecraft skin texture (either one of the 4 embedded PNGs or a user-supplied custom PNG from `mod_data/flyingweb/suits/`).
- **Top-Right `(64, 0, 192, 64)`**: Procedurally rasterized crisp HUD icons for all 8 Spider-Gadgets, web reticle markers, Spider-Crest emblem, and radial wheel wedges.
- **Bottom `(0, 64, 128, 64)`**: Woven white/silver silk Cocoon & Wall-Pin overlay tiles rendered onto ensnared mobs via `native_mob_layer`.

---

## 6. Network Protocol (`flyingweb:v1`)

Client-to-server and server-to-client synchronization uses a compact, versioned JSON wire frame on channel `"flyingweb:v1"` (`MAX_PACKET_BYTES = 1024`, all floats checked with `f32::is_finite()`):

- **`ClientPacket::FireGadget`**: Requests gadget activation from the server with sanitized aim vector and sequence ID; the server validates range (`<= 6.0m` origin tolerance), per-player cooldowns, and finite coordinates before applying non-lethal mob impulses (`native_push_mob`) and status states (`WebbedSlow`, `WebbedCocoon`, `WallPinned`, `Suspended`, `Shocked`).
- **`ClientPacket::TraversalSync`**: Synchronizes active swing/dive fall-cushioning (`fall_cushion_credit`) so `native_living_incoming_damage` prevents unfair kinetic fall damage after high-altitude aerial arcs without granting god-mode immunity to combat or lava.
- **`ServerPacket::WorldSync`**: Broadcasts active world snares, trip mines, drones, and webbed mob statuses (`entity_id`, `status`, `remaining_s`) so all nearby clients render silk cocoons and tether lines accurately.
