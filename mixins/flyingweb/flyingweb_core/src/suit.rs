use std::{fs, io::Cursor, path::Path};

use glam::{Mat4, Quat, Vec3};

use crate::{
    animations::SpiderRigPose,
    config::{FlyingWebConfig, SuitRenderMode, SuitVariant},
    gadgets::GadgetKind,
};

pub const ATLAS_WIDTH: u32 = 256;
pub const ATLAS_HEIGHT: u32 = 256;

const CLASSIC_PNG: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/suits/classic_suit.png"));
const ADVANCED_PNG: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/suits/advanced_suit.png"));
const MILES_PNG: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/suits/miles_suit.png"));
const SYMBIOTE_PNG: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/suits/symbiote_suit.png"));

/// One textured 3D quad in `BlockEntityDraw` local coordinates (`1.0` unit = `1` block)
/// with `uv_16` mapped onto `MOD_IMAGE_SPRITE` (`0.0..=16.0` across the 256x256 atlas).
#[derive(Clone, Copy, Debug)]
pub struct SuitQuad {
    pub corners: [[f32; 3]; 4],
    pub uv_16: [f32; 4],
    pub tint_rgb: u32,
}

/// Manages the 256x256 RGBA `MOD_IMAGE_SPRITE` atlas containing:
/// - `[0..64, 0..64]`: Active 64x64 Minecraft-skin-layout Spider-Suit PNG
/// - `[64..128, 0..64]`: Translucent axillary Web-Wings airfoil silk texture
/// - `[128..192, 0..64]`: 3D Mob Web-Cocoon / Restraint wrap texture
/// - `[0..144, 64..80]`: Crisp 16x16 Spider-HUD & Gadget icons
#[derive(Clone, Debug)]
pub struct SuitAtlasManager {
    pub pixels: Vec<u8>,
    pub dirty: bool,
    loaded_variant: Option<(SuitVariant, String)>,
}

impl Default for SuitAtlasManager {
    fn default() -> Self {
        let mut mgr = Self {
            pixels: vec![0_u8; (ATLAS_WIDTH * ATLAS_HEIGHT * 4) as usize],
            dirty: true,
            loaded_variant: None,
        };
        mgr.paint_static_regions();
        mgr.load_builtin_suit(SuitVariant::Classic);
        mgr
    }
}

impl SuitAtlasManager {
    /// Seeds built-in editable 64x64 suit PNG templates into `<data_dir>/suits/` if absent
    /// so players can inspect, edit, or add `custom_suit.png` without touching Rust code.
    pub fn ensure_disk_templates(data_dir: &Path) {
        let suits_dir = data_dir.join("suits");
        if fs::create_dir_all(&suits_dir).is_err() {
            return;
        }
        let templates = [
            ("classic_suit.png", CLASSIC_PNG),
            ("advanced_suit.png", ADVANCED_PNG),
            ("miles_suit.png", MILES_PNG),
            ("symbiote_suit.png", SYMBIOTE_PNG),
            ("custom_suit.png", CLASSIC_PNG),
        ];
        for (name, bytes) in templates {
            let p = suits_dir.join(name);
            if !p.exists() {
                let _ = fs::write(p, bytes);
            }
        }
    }

    pub fn sync_with_config(&mut self, cfg: &FlyingWebConfig, data_dir: Option<&Path>) {
        let key = (cfg.suit_variant, cfg.custom_suit_filename.clone());
        if self.loaded_variant.as_ref() == Some(&key) {
            return;
        }
        self.reload_from_config(cfg, data_dir);
    }

    pub fn reload_from_config(&mut self, cfg: &FlyingWebConfig, data_dir: Option<&Path>) {
        let filename = cfg.suit_variant.filename(&cfg.custom_suit_filename);
        // Always check `<data_dir>/suits/<filename>` first so user edits to ANY suit PNG
        // on disk take effect immediately upon reload without modifying code!
        if let Some(dir) = data_dir {
            let disk_path = dir.join("suits").join(filename);
            if let Ok(bytes) = fs::read(&disk_path)
                && self.blit_64x64_png(&bytes)
            {
                self.loaded_variant = Some((cfg.suit_variant, cfg.custom_suit_filename.clone()));
                self.dirty = true;
                return;
            }
        }
        self.load_builtin_suit(cfg.suit_variant);
        self.loaded_variant = Some((cfg.suit_variant, cfg.custom_suit_filename.clone()));
    }

    pub fn load_builtin_suit(&mut self, variant: SuitVariant) {
        let bytes = match variant {
            SuitVariant::Classic | SuitVariant::CustomFile => CLASSIC_PNG,
            SuitVariant::Advanced => ADVANCED_PNG,
            SuitVariant::Miles => MILES_PNG,
            SuitVariant::Symbiote => SYMBIOTE_PNG,
        };
        let _ = self.blit_64x64_png(bytes);
        self.dirty = true;
    }

    /// Decodes a standard 64x64 RGBA or RGB PNG into the `[0..64, 0..64]` quadrant of the atlas.
    pub fn blit_64x64_png(&mut self, png_bytes: &[u8]) -> bool {
        let decoder = png::Decoder::new(Cursor::new(png_bytes));
        let Ok(mut reader) = decoder.read_info() else {
            return false;
        };
        let info = reader.info();
        if info.width != 64 || info.height != 64 {
            return false;
        }
        let mut buf = vec![0_u8; reader.output_buffer_size().unwrap_or(64 * 64 * 4)];
        let Ok(frame) = reader.next_frame(&mut buf) else {
            return false;
        };
        let data = &buf[..frame.buffer_size()];

        for y in 0..64_u32 {
            for x in 0..64_u32 {
                let rgba = match frame.color_type {
                    png::ColorType::Rgba => {
                        let i = ((y * 64 + x) * 4) as usize;
                        if i + 3 >= data.len() {
                            return false;
                        }
                        [data[i], data[i + 1], data[i + 2], data[i + 3]]
                    }
                    png::ColorType::Rgb => {
                        let i = ((y * 64 + x) * 3) as usize;
                        if i + 2 >= data.len() {
                            return false;
                        }
                        [data[i], data[i + 1], data[i + 2], 255]
                    }
                    _ => return false,
                };
                self.set_pixel(x, y, rgba);
            }
        }
        true
    }

    fn set_pixel(&mut self, x: u32, y: u32, rgba: [u8; 4]) {
        if x < ATLAS_WIDTH && y < ATLAS_HEIGHT {
            let idx = ((y * ATLAS_WIDTH + x) * 4) as usize;
            self.pixels[idx..idx + 4].copy_from_slice(&rgba);
        }
    }

    fn paint_static_regions(&mut self) {
        // 1. [64..128, 0..64]: Translucent glowing Web-Wings underarm silk airfoil
        for ly in 0..64_u32 {
            for lx in 0..64_u32 {
                let diag = (lx as i32 - ly as i32).unsigned_abs();
                let rib = lx % 9 == 0 || ly % 9 == 0 || diag % 11 == 0;
                let border = lx < 2 || ly < 2 || lx + ly > 61;
                if lx + ly <= 63 {
                    let px = if border {
                        [225, 244, 255, 235]
                    } else if rib {
                        [180, 228, 255, 205]
                    } else {
                        [135, 205, 248, 130]
                    };
                    self.set_pixel(64 + lx, ly, px);
                }
            }
        }

        // 2. [128..192, 0..64]: 3D Mob Web-Cocoon / Restraint silk wrap pattern
        for ly in 0..64_u32 {
            for lx in 0..64_u32 {
                let strand_a = (lx + ly * 2) % 7 <= 1;
                let strand_b = (lx * 2 + 64 - ly) % 8 <= 1;
                let ring = ly % 10 == 0;
                let px = if strand_a || strand_b || ring {
                    [244, 248, 255, 235]
                } else {
                    [225, 234, 245, 92]
                };
                self.set_pixel(128 + lx, ly, px);
            }
        }

        // 3. [0..144, 64..80]: Nine crisp 16x16 HUD icons (0 = Spider Emblem, 1..=8 = Gadgets)
        // Slot 0: Spider Emblem (0..16, 64..80)
        for y in 0..16_u32 {
            for x in 0..16_u32 {
                let dx = (x as i32 - 8).abs();
                let dy = (y as i32 - 8).abs();
                let body = dx <= 1 && dy <= 3;
                let legs = (dx == dy && (2..=6).contains(&dx)) || (dy == 1 && (2..=5).contains(&dx));
                if body || legs {
                    self.set_pixel(x, 64 + y, [238, 44, 58, 255]);
                }
            }
        }

        // Slots 1..=8: Gadget icons (`slot * 16 .. (slot + 1) * 16`, `64..80`)
        for (idx, kind) in GadgetKind::ALL.iter().copied().enumerate() {
            let ox = ((idx as u32) + 1) * 16;
            let oy = 64_u32;
            let accent = match kind {
                GadgetKind::ImpactWeb => [242, 248, 255, 255],
                GadgetKind::WebGrabber => [96, 224, 255, 255],
                GadgetKind::WebBomb => [255, 214, 92, 255],
                GadgetKind::TripMine => [72, 220, 255, 255],
                GadgetKind::RemoteMine => [255, 148, 56, 255],
                GadgetKind::WebLine => [196, 242, 255, 255],
                GadgetKind::WebTrap => [152, 236, 198, 255],
                GadgetKind::RicochetWeb => [224, 168, 255, 255],
            };
            for y in 2..14_u32 {
                for x in 2..14_u32 {
                    let dx = (x as i32 - 8).abs();
                    let dy = (y as i32 - 8).abs();
                    let on = match kind {
                        GadgetKind::ImpactWeb => dx + dy <= 5 && (dx <= 1 || dy <= 1),
                        GadgetKind::WebGrabber => dx * dx + dy * dy <= 26 && (dx == dy || dx == 0 || dy == 0),
                        GadgetKind::WebBomb => dx * dx + dy * dy <= 22,
                        GadgetKind::TripMine => dy <= 1 || (x <= 5 && dy <= 3),
                        GadgetKind::RemoteMine => (dx <= 4 && dy <= 4) && !(dx <= 1 && dy <= 1),
                        GadgetKind::WebLine => dy <= 1 || ((x == 3 || x == 12) && dy <= 4),
                        GadgetKind::WebTrap => dx + dy == 5 || dx + dy == 3,
                        GadgetKind::RicochetWeb => (x + y == 15) || (x == y && x >= 7),
                    };
                    if on {
                        self.set_pixel(ox + x, oy + y, accent);
                    }
                }
            }
        }
    }

    /// Source `[x, y, w, h]` in atlas pixels for `native_sprite(..., MOD_IMAGE_SPRITE, src, ...)`.
    pub fn hud_spider_icon_src() -> [f32; 4] {
        [0.0, 64.0, 16.0, 16.0]
    }

    pub fn hud_gadget_icon_src(kind: GadgetKind) -> [f32; 4] {
        let slot = GadgetKind::ALL
            .iter()
            .position(|&g| g == kind)
            .unwrap_or(0) as f32
            + 1.0;
        [slot * 16.0, 64.0, 16.0, 16.0]
    }
}

/// Converts pixel coordinates `(px0, py0, px1, py1)` inside the 256x256 `MOD_IMAGE_SPRITE`
/// into `native_draw_quad`'s `0.0..=16.0` UV range (`pixel / 256.0 * 16.0 == pixel / 16.0`).
#[inline]
fn atlas_uv16(px0: f32, py0: f32, px1: f32, py1: f32) -> [f32; 4] {
    [px0 / 16.0, py0 / 16.0, px1 / 16.0, py1 / 16.0]
}

fn euler_quat_deg(rot: [f32; 3]) -> Quat {
    Quat::from_rotation_z(rot[2].to_radians())
        * Quat::from_rotation_y(rot[1].to_radians())
        * Quat::from_rotation_x(rot[0].to_radians())
}

/// Emits the 6 textured faces of a standard 64x64 Minecraft player-skin cuboid
/// in `BlockEntityDraw` space (`+Z` faces forward/south, `+X` is model-left, `+Y` is up).
#[allow(clippy::too_many_arguments)]
fn emit_skin_box(
    transform: Mat4,
    min_texels: [f32; 3],
    size_texels: [f32; 3],
    inflate_texels: f32,
    uv_origin: [u32; 2],
    uv_whd: [u32; 3],
    tint_rgb: u32,
    out: &mut Vec<SuitQuad>,
) {
    let min = (Vec3::from_array(min_texels) - Vec3::splat(inflate_texels)) / 16.0;
    let max =
        (Vec3::from_array(min_texels) + Vec3::from_array(size_texels) + Vec3::splat(inflate_texels))
            / 16.0;

    let [u, v] = uv_origin.map(|c| c as f32);
    let [w, h, d] = uv_whd.map(|c| c as f32);

    let xform = |x: f32, y: f32, z: f32| -> [f32; 3] {
        transform.transform_point3(Vec3::new(x, y, z)).to_array()
    };

    // Note on winding: `BlockEntityDraw::quad` expects `[top_left, bottom_left, bottom_right, top_right]`
    // matching `[[u0, v0], [u0, v1], [u1, v1], [u1, v0]]` counter-clockwise seen from outside!

    // Front (+Z face): seen from +Z, left is +X, right is -X!
    out.push(SuitQuad {
        corners: [
            xform(max.x, max.y, max.z),
            xform(max.x, min.y, max.z),
            xform(min.x, min.y, max.z),
            xform(min.x, max.y, max.z),
        ],
        uv_16: atlas_uv16(u + d, v + d, u + d + w, v + d + h),
        tint_rgb,
    });

    // Back (-Z face): seen from -Z, left is -X, right is +X!
    out.push(SuitQuad {
        corners: [
            xform(min.x, max.y, min.z),
            xform(min.x, min.y, min.z),
            xform(max.x, min.y, min.z),
            xform(max.x, max.y, min.z),
        ],
        uv_16: atlas_uv16(u + 2.0 * d + w, v + d, u + 2.0 * d + 2.0 * w, v + d + h),
        tint_rgb,
    });

    // Model-Right (-X face): seen from -X, left is +Z, right is -Z!
    out.push(SuitQuad {
        corners: [
            xform(min.x, max.y, max.z),
            xform(min.x, min.y, max.z),
            xform(min.x, min.y, min.z),
            xform(min.x, max.y, min.z),
        ],
        uv_16: atlas_uv16(u, v + d, u + d, v + d + h),
        tint_rgb,
    });

    // Model-Left (+X face): seen from +X, left is -Z, right is +Z!
    out.push(SuitQuad {
        corners: [
            xform(max.x, max.y, min.z),
            xform(max.x, min.y, min.z),
            xform(max.x, min.y, max.z),
            xform(max.x, max.y, max.z),
        ],
        uv_16: atlas_uv16(u + d + w, v + d, u + 2.0 * d + w, v + d + h),
        tint_rgb,
    });

    // Top (+Y face)
    out.push(SuitQuad {
        corners: [
            xform(max.x, max.y, min.z),
            xform(max.x, max.y, max.z),
            xform(min.x, max.y, max.z),
            xform(min.x, max.y, min.z),
        ],
        uv_16: atlas_uv16(u + d, v, u + d + w, v + d),
        tint_rgb,
    });

    // Bottom (-Y face)
    out.push(SuitQuad {
        corners: [
            xform(max.x, min.y, max.z),
            xform(max.x, min.y, min.z),
            xform(min.x, min.y, min.z),
            xform(min.x, min.y, max.z),
        ],
        uv_16: atlas_uv16(u + d + w, v, u + d + 2.0 * w, v + d),
        tint_rgb,
    });
}

/// Builds the full articulated FiskHeroes-style skin-layer Spider-Suit mesh (plus optional
/// 0.25-texel second-skin layer and deployable underarm Web-Wings airfoil membranes!).
pub fn build_spider_suit_quads(
    pose: &SpiderRigPose,
    cfg: &FlyingWebConfig,
    hurt_flash: bool,
    out: &mut Vec<SuitQuad>,
) {
    if cfg.suit_mode == SuitRenderMode::Hidden {
        return;
    }

    let tint = if hurt_flash { 0xFF8888 } else { 0xFFFFFF };
    let chest_pivot = Vec3::new(0.0, 0.90 + pose.root_offset_y, 0.0);
    let root_m = Mat4::from_translation(chest_pivot)
        * Mat4::from_quat(euler_quat_deg(pose.body_rot))
        * Mat4::from_translation(-chest_pivot);

    if cfg.suit_mode == SuitRenderMode::SkinSuit {
        // 1. Torso (`8x12x4` texels from y=12..24 texels = 0.75..1.50 blocks)
        let torso_m = root_m * Mat4::from_translation(Vec3::new(0.0, pose.root_offset_y, 0.0));
        emit_skin_box(torso_m, [-4.0, 12.0, -2.0], [8.0, 12.0, 4.0], 0.0, [16, 16], [8, 12, 4], tint, out);
        if cfg.show_second_skin_layer {
            emit_skin_box(torso_m, [-4.0, 12.0, -2.0], [8.0, 12.0, 4.0], 0.25, [16, 32], [8, 12, 4], tint, out);
        }

        // 2. Head (`8x8x8` texels pivoted at neck `(0, 1.50, 0)`)
        let neck = Vec3::new(0.0, 1.50, 0.0);
        let head_m = torso_m
            * Mat4::from_translation(neck)
            * Mat4::from_quat(euler_quat_deg([
                pose.head_rot[0],
                -pose.head_rot[1],
                pose.head_rot[2],
            ]))
            * Mat4::from_translation(-neck);
        emit_skin_box(head_m, [-4.0, 24.0, -4.0], [8.0, 8.0, 8.0], 0.0, [0, 0], [8, 8, 8], tint, out);
        if cfg.show_second_skin_layer {
            emit_skin_box(head_m, [-4.0, 24.0, -4.0], [8.0, 8.0, 8.0], 0.35, [32, 0], [8, 8, 8], tint, out);
        }

        // 3. Right Arm (`-X` side in model space, shoulder pivot at `(-5/16, 22/16, 0)`)
        let r_shoulder = Vec3::new(-5.0 / 16.0, 22.0 / 16.0, 0.0);
        let r_arm_m = torso_m
            * Mat4::from_translation(r_shoulder)
            * Mat4::from_quat(euler_quat_deg(pose.right_arm_rot))
            * Mat4::from_translation(-r_shoulder);
        emit_skin_box(r_arm_m, [-8.0, 12.0, -2.0], [4.0, 12.0, 4.0], 0.0, [40, 16], [4, 12, 4], tint, out);
        if cfg.show_second_skin_layer || cfg.show_web_shooters {
            emit_skin_box(r_arm_m, [-8.0, 12.0, -2.0], [4.0, 12.0, 4.0], 0.25, [40, 32], [4, 12, 4], tint, out);
        }

        // 4. Left Arm (`+X` side in model space, shoulder pivot at `(+5/16, 22/16, 0)`)
        let l_shoulder = Vec3::new(5.0 / 16.0, 22.0 / 16.0, 0.0);
        let l_arm_m = torso_m
            * Mat4::from_translation(l_shoulder)
            * Mat4::from_quat(euler_quat_deg(pose.left_arm_rot))
            * Mat4::from_translation(-l_shoulder);
        emit_skin_box(l_arm_m, [4.0, 12.0, -2.0], [4.0, 12.0, 4.0], 0.0, [32, 48], [4, 12, 4], tint, out);
        if cfg.show_second_skin_layer || cfg.show_web_shooters {
            emit_skin_box(l_arm_m, [4.0, 12.0, -2.0], [4.0, 12.0, 4.0], 0.25, [48, 48], [4, 12, 4], tint, out);
        }

        // 5. Right Leg (`-X` hip at `(-2/16, 12/16, 0)`)
        let r_hip = Vec3::new(-2.0 / 16.0, 12.0 / 16.0, 0.0);
        let r_leg_m = torso_m
            * Mat4::from_translation(r_hip)
            * Mat4::from_quat(euler_quat_deg(pose.right_leg_rot))
            * Mat4::from_translation(-r_hip);
        emit_skin_box(r_leg_m, [-4.0, 0.0, -2.0], [4.0, 12.0, 4.0], 0.0, [0, 16], [4, 12, 4], tint, out);
        if cfg.show_second_skin_layer {
            emit_skin_box(r_leg_m, [-4.0, 0.0, -2.0], [4.0, 12.0, 4.0], 0.25, [0, 32], [4, 12, 4], tint, out);
        }

        // 6. Left Leg (`+X` hip at `(+2/16, 12/16, 0)`)
        let l_hip = Vec3::new(2.0 / 16.0, 12.0 / 16.0, 0.0);
        let l_leg_m = torso_m
            * Mat4::from_translation(l_hip)
            * Mat4::from_quat(euler_quat_deg(pose.left_leg_rot))
            * Mat4::from_translation(-l_hip);
        emit_skin_box(l_leg_m, [0.0, 0.0, -2.0], [4.0, 12.0, 4.0], 0.0, [16, 48], [4, 12, 4], tint, out);
        if cfg.show_second_skin_layer {
            emit_skin_box(l_leg_m, [0.0, 0.0, -2.0], [4.0, 12.0, 4.0], 0.25, [0, 48], [4, 12, 4], tint, out);
        }

        // 7. Deployable Underarm Web-Wings Airfoil Membranes (double-sided quads!)
        if cfg.show_web_wings_model && pose.web_wings_deploy > 0.04 {
            emit_web_wings(torso_m, r_arm_m, l_arm_m, pose.web_wings_deploy, tint, out);
        }
    } else if cfg.suit_mode == SuitRenderMode::WebShootersOnly {
        // Overlay slim wrist Web-Shooter cuffs + Web Wings on vanilla player model (`native_mob_layer`)!
        let torso_m = Mat4::IDENTITY;
        if cfg.show_web_shooters {
            emit_skin_box(torso_m, [-8.0, 12.5, -2.0], [4.0, 2.0, 4.0], 0.30, [40, 32], [4, 12, 4], tint, out);
            emit_skin_box(torso_m, [4.0, 12.5, -2.0], [4.0, 2.0, 4.0], 0.30, [48, 48], [4, 12, 4], tint, out);
        }
        if cfg.show_web_wings_model && pose.web_wings_deploy > 0.04 {
            let r_shoulder = Vec3::new(-5.0 / 16.0, 22.0 / 16.0, 0.0);
            let l_shoulder = Vec3::new(5.0 / 16.0, 22.0 / 16.0, 0.0);
            let r_arm_m = Mat4::from_translation(r_shoulder)
                * Mat4::from_quat(euler_quat_deg([0.0, 0.0, 74.0]))
                * Mat4::from_translation(-r_shoulder);
            let l_arm_m = Mat4::from_translation(l_shoulder)
                * Mat4::from_quat(euler_quat_deg([0.0, 0.0, -74.0]))
                * Mat4::from_translation(-l_shoulder);
            emit_web_wings(torso_m, r_arm_m, l_arm_m, pose.web_wings_deploy, tint, out);
        }
    }
}

fn emit_web_wings(
    torso_m: Mat4,
    r_arm_m: Mat4,
    l_arm_m: Mat4,
    deploy: f32,
    tint_rgb: u32,
    out: &mut Vec<SuitQuad>,
) {
    let wing_uv = atlas_uv16(64.0, 0.0, 120.0, 56.0);

    let r_armpit = torso_m.transform_point3(Vec3::new(-4.0 / 16.0, 21.0 / 16.0, 0.0));
    let r_waist = torso_m.transform_point3(Vec3::new(
        -4.0 / 16.0,
        (21.0 - 9.5 * deploy) / 16.0,
        -0.5 / 16.0,
    ));
    let r_wrist = r_arm_m.transform_point3(Vec3::new(
        (-6.0 - 2.0 * deploy) / 16.0,
        (21.0 - 8.5 * deploy) / 16.0,
        0.0,
    ));
    let r_elbow = r_arm_m.transform_point3(Vec3::new(-6.0 / 16.0, 18.0 / 16.0, 0.0));

    // Double-sided right wing membrane
    out.push(SuitQuad {
        corners: [
            r_armpit.to_array(),
            r_waist.to_array(),
            r_wrist.to_array(),
            r_elbow.to_array(),
        ],
        uv_16: wing_uv,
        tint_rgb,
    });
    out.push(SuitQuad {
        corners: [
            r_elbow.to_array(),
            r_wrist.to_array(),
            r_waist.to_array(),
            r_armpit.to_array(),
        ],
        uv_16: wing_uv,
        tint_rgb,
    });

    let l_armpit = torso_m.transform_point3(Vec3::new(4.0 / 16.0, 21.0 / 16.0, 0.0));
    let l_waist = torso_m.transform_point3(Vec3::new(
        4.0 / 16.0,
        (21.0 - 9.5 * deploy) / 16.0,
        -0.5 / 16.0,
    ));
    let l_wrist = l_arm_m.transform_point3(Vec3::new(
        (6.0 + 2.0 * deploy) / 16.0,
        (21.0 - 8.5 * deploy) / 16.0,
        0.0,
    ));
    let l_elbow = l_arm_m.transform_point3(Vec3::new(6.0 / 16.0, 18.0 / 16.0, 0.0));

    out.push(SuitQuad {
        corners: [
            l_armpit.to_array(),
            l_waist.to_array(),
            l_wrist.to_array(),
            l_elbow.to_array(),
        ],
        uv_16: wing_uv,
        tint_rgb,
    });
    out.push(SuitQuad {
        corners: [
            l_elbow.to_array(),
            l_wrist.to_array(),
            l_waist.to_array(),
            l_armpit.to_array(),
        ],
        uv_16: wing_uv,
        tint_rgb,
    });
}

/// Emits a 3D spider-silk cocoon wrap around a webbed mob in `native_mob_layer`!
pub fn build_mob_web_cocoon_quads(out: &mut Vec<SuitQuad>) {
    emit_skin_box(
        Mat4::IDENTITY,
        [-5.2, 2.0, -5.2],
        [10.4, 15.0, 10.4],
        0.0,
        [128, 0],
        [10, 15, 10],
        0xFFFFFF,
        out,
    );
}
