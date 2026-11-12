#!/usr/bin/env python3
"""Generate original 64x64 standard Minecraft-skin-layout Spider-Hero suit PNGs.

All generated textures are 100% original pixel art using standard 64x64 Java
Edition player skin UV coordinates (base skin + 0.25-texel second-skin layer)
so users can edit them in Blockbench / Aseprite or drop any standard 64x64
player skin PNG into `<horizon_data>/mods/flyingweb/suits/` without touching code.
"""

from __future__ import annotations

import struct
import zlib
from pathlib import Path

WIDTH = 64
HEIGHT = 64


def write_png(path: Path, pixels: list[list[tuple[int, int, int, int]]]) -> None:
    raw = bytearray()
    for row in pixels:
        raw.append(0)  # Filter type 0 (None)
        for r, g, b, a in row:
            raw.extend((r & 0xFF, g & 0xFF, b & 0xFF, a & 0xFF))

    def chunk(tag: bytes, data: bytes) -> bytes:
        crc = zlib.crc32(tag + data) & 0xFFFFFFFF
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", crc)

    ihdr = struct.pack(">IIBBBBB", WIDTH, HEIGHT, 8, 6, 0, 0, 0)
    png_bytes = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", ihdr)
        + chunk(b"IDAT", zlib.compress(bytes(raw), level=9))
        + chunk(b"IEND", b"")
    )
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(png_bytes)


def blank_canvas() -> list[list[tuple[int, int, int, int]]]:
    return [[(0, 0, 0, 0) for _ in range(WIDTH)] for _ in range(HEIGHT)]


def fill_rect(
    px: list[list[tuple[int, int, int, int]]],
    x0: int,
    y0: int,
    w: int,
    h: int,
    color: tuple[int, int, int, int],
) -> None:
    for y in range(y0, y0 + h):
        if 0 <= y < HEIGHT:
            for x in range(x0, x0 + w):
                if 0 <= x < WIDTH:
                    px[y][x] = color


def shade(color: tuple[int, int, int, int], delta: int) -> tuple[int, int, int, int]:
    r, g, b, a = color
    return (
        max(0, min(255, r + delta)),
        max(0, min(255, g + delta)),
        max(0, min(255, b + delta)),
        a,
    )


def paint_box_uv(
    px: list[list[tuple[int, int, int, int]]],
    u: int,
    v: int,
    w: int,
    h: int,
    d: int,
    color: tuple[int, int, int, int],
) -> None:
    # Standard Minecraft box UV layout:
    # Top:    (u + d,         v)     size w x d
    # Bottom: (u + d + w,     v)     size w x d
    # Right:  (u,             v + d) size d x h
    # Front:  (u + d,         v + d) size w x h
    # Left:   (u + d + w,     v + d) size d x h
    # Back:   (u + 2*d + w,   v + d) size w x h
    fill_rect(px, u + d, v, w, d, shade(color, 12))
    fill_rect(px, u + d + w, v, w, d, shade(color, -18))
    fill_rect(px, u, v + d, d, h, shade(color, -10))
    fill_rect(px, u + d, v + d, w, h, color)
    fill_rect(px, u + d + w, v + d, d, h, shade(color, -6))
    fill_rect(px, u + 2 * d + w, v + d, w, h, shade(color, -12))


def add_web_grid(
    px: list[list[tuple[int, int, int, int]]],
    x0: int,
    y0: int,
    w: int,
    h: int,
    web_color: tuple[int, int, int, int],
) -> None:
    for y in range(y0, y0 + h):
        for x in range(x0, x0 + w):
            lx = x - x0
            ly = y - y0
            if ly % 3 == 0 or (lx in (0, w - 1, w // 2) and ly % 2 == 1):
                px[y][x] = web_color


def build_suit(
    primary: tuple[int, int, int, int],
    secondary: tuple[int, int, int, int],
    web_line: tuple[int, int, int, int],
    emblem: tuple[int, int, int, int],
    lens_border: tuple[int, int, int, int],
    lens_fill: tuple[int, int, int, int],
    shooter_metal: tuple[int, int, int, int],
    accent_glow: tuple[int, int, int, int],
) -> list[list[tuple[int, int, int, int]]]:
    px = blank_canvas()

    # 1. Head base (8x8x8 at 0,0) + web pattern
    paint_box_uv(px, 0, 0, 8, 8, 8, primary)
    add_web_grid(px, 8, 8, 8, 8, web_line)
    add_web_grid(px, 24, 8, 8, 8, web_line)

    # Expressive Spider-Lens eyes on Head Front (8..16, 8..16) and Hat Front (40..48, 8..16)
    for ox in (8, 40):
        # Left lens
        fill_rect(px, ox + 1, 10, 2, 3, lens_border)
        px[11][ox + 1] = lens_fill
        px[11][ox + 2] = lens_fill
        px[10][ox + 2] = lens_fill
        # Right lens
        fill_rect(px, ox + 5, 10, 2, 3, lens_border)
        px[11][ox + 5] = lens_fill
        px[11][ox + 6] = lens_fill
        px[10][ox + 5] = lens_fill

    # 2. Body base (8x12x4 at 16,16): secondary side panels + primary chest/belt V
    paint_box_uv(px, 16, 16, 8, 12, 4, secondary)
    # Primary center chest panel + V-belt on Front (20..28, 20..32)
    fill_rect(px, 21, 20, 6, 7, primary)
    add_web_grid(px, 21, 20, 6, 7, web_line)
    fill_rect(px, 20, 30, 8, 2, primary)
    px[31][23] = primary
    px[31][24] = primary
    # Back upper panel + back spider emblem on Back (32..40, 20..32)
    fill_rect(px, 33, 21, 6, 6, emblem)
    fill_rect(px, 34, 22, 4, 4, primary)
    px[23][35] = emblem
    px[23][36] = emblem
    px[24][35] = emblem
    px[24][36] = emblem

    # Chest Spider Emblem on both base front (20,20) and jacket overlay front (20,36)
    for oy in (20, 36):
        # Central body of spider
        px[oy + 2][23] = emblem
        px[oy + 2][24] = emblem
        px[oy + 3][23] = emblem
        px[oy + 3][24] = emblem
        px[oy + 4][23] = emblem
        px[oy + 4][24] = emblem
        # Spider legs splaying diagonally
        px[oy + 1][22] = emblem
        px[oy + 1][25] = emblem
        px[oy + 3][21] = emblem
        px[oy + 3][26] = emblem
        px[oy + 5][22] = emblem
        px[oy + 5][25] = emblem

    # 3. Right Arm (40,16) & Left Arm (32,48): secondary inner bicep + primary shoulder/glove
    for u, v, ou, ov in ((40, 16, 40, 32), (32, 48, 48, 48)):
        paint_box_uv(px, u, v, 4, 12, 4, secondary)
        # Shoulder & outer stripe + glove
        fill_rect(px, u + 4, v + 4, 4, 4, primary)
        fill_rect(px, u + 4, v + 12, 4, 4, primary)
        add_web_grid(px, u + 4, v + 12, 4, 4, web_line)
        # Slim FiskHeroes-style wrist Web-Shooter cuff on 2nd-skin sleeve layer
        fill_rect(px, ou, ov + 13, 16, 2, shooter_metal)
        px[ov + 14][ou + 5] = accent_glow
        px[ov + 14][ou + 6] = accent_glow

    # 4. Right Leg (0,16) & Left Leg (16,48): secondary thighs + primary webbed boots
    for u, v, ou, ov in ((0, 16, 0, 32), (16, 48, 0, 48)):
        paint_box_uv(px, u, v, 4, 12, 4, secondary)
        # Boots (bottom 5 pixels of leg)
        fill_rect(px, u, v + 11, 16, 5, primary)
        add_web_grid(px, u + 4, v + 11, 4, 5, web_line)
        # Subtle boot cuff trim on 2nd-skin pant layer
        fill_rect(px, ou + 4, ov + 11, 4, 1, shade(primary, 20))

    return px


def main() -> None:
    out_dir = (
        Path(__file__).resolve().parents[1]
        / "mixins"
        / "flyingweb"
        / "flyingweb_core"
        / "data"
        / "suits"
    )

    suits = {
        "classic_suit.png": build_suit(
            primary=(214, 34, 46, 255),
            secondary=(26, 72, 176, 255),
            web_line=(38, 12, 18, 255),
            emblem=(20, 22, 30, 255),
            lens_border=(16, 18, 24, 255),
            lens_fill=(244, 248, 255, 255),
            shooter_metal=(180, 190, 205, 255),
            accent_glow=(90, 210, 255, 255),
        ),
        "advanced_suit.png": build_suit(
            primary=(222, 42, 38, 255),
            secondary=(22, 58, 148, 255),
            web_line=(48, 16, 18, 255),
            emblem=(245, 248, 252, 255),
            lens_border=(18, 22, 30, 255),
            lens_fill=(248, 250, 255, 255),
            shooter_metal=(235, 240, 248, 255),
            accent_glow=(110, 225, 255, 255),
        ),
        "miles_suit.png": build_suit(
            primary=(26, 28, 36, 255),
            secondary=(18, 20, 26, 255),
            web_line=(196, 28, 48, 255),
            emblem=(228, 32, 52, 255),
            lens_border=(210, 28, 46, 255),
            lens_fill=(245, 246, 252, 255),
            shooter_metal=(72, 24, 34, 255),
            accent_glow=(255, 196, 64, 255),
        ),
        "symbiote_suit.png": build_suit(
            primary=(20, 22, 30, 255),
            secondary=(14, 15, 22, 255),
            web_line=(36, 40, 56, 255),
            emblem=(242, 246, 255, 255),
            lens_border=(220, 228, 245, 255),
            lens_fill=(250, 252, 255, 255),
            shooter_metal=(48, 52, 68, 255),
            accent_glow=(210, 225, 255, 255),
        ),
    }

    for filename, canvas in suits.items():
        target = out_dir / filename
        write_png(target, canvas)
        print(f"Wrote {target} ({target.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
