#!/usr/bin/env python3
"""Generates the macOS app icon.

A macOS icon is not a plain square image. The shape must be a superellipse
(squircle) of 824x824 inside a 1024x1024 canvas, with a 100 px transparent
margin on each side and a soft shadow. A full-canvas image looks larger than
system icons in the Dock and has the wrong corner shape.

The script draws the shape and the glyph (a graph of three nodes) from scratch
and builds `src-tauri/icons/icon.icns`. It does not touch icons for other
platforms: they need a full-canvas image.

Run: `python3 scripts/gen-macos-icon.py` (needs Pillow, numpy and iconutil).
"""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

# --- canvas geometry (Apple macOS template) -------------------------------

CANVAS = 1024
BODY = 824  # shape side; margin = (CANVAS - BODY) / 2 = 100
SQUIRCLE_N = 5.0  # superellipse exponent; matched to system icons
SS = 4  # supersampling factor for rasterization

# --- colors --------------------------------------------------------------

FILL_TOP = (94, 110, 226)
FILL_BOTTOM = (58, 74, 197)
GLYPH_COLOR = (255, 255, 255)
EDGE_OPACITY = 0.6  # graph edges are lighter than nodes, as in the original icon

# --- shadow --------------------------------------------------------------

SHADOW_OFFSET = 10
SHADOW_BLUR = 12
SHADOW_OPACITY = 0.22

# --- glyph: graph of three nodes -----------------------------------------
# Coordinates come from the previous icon (in a 512x512 coordinate system).

NODES = [
    ((149.5, 163.0), 62.0),
    ((368.5, 153.5), 42.0),
    ((358.0, 348.0), 65.5),
]
EDGES = [(0, 1), (0, 2)]
EDGE_WIDTH = 17.0
GLYPH_SOURCE_BOX = (87.5, 101.0, 423.5, 413.5)  # glyph bounds in the same coordinates
GLYPH_WIDTH_RATIO = 0.66  # part of the shape width that the glyph takes

ICNS_SIZES = [
    ("icon_16x16.png", 16),
    ("icon_16x16@2x.png", 32),
    ("icon_32x32.png", 32),
    ("icon_32x32@2x.png", 64),
    ("icon_128x128.png", 128),
    ("icon_128x128@2x.png", 256),
    ("icon_256x256.png", 256),
    ("icon_256x256@2x.png", 512),
    ("icon_512x512.png", 512),
    ("icon_512x512@2x.png", 1024),
]


def squircle_mask() -> Image.Image:
    """Shape mask: superellipse |x/a|^n + |y/a|^n <= 1, antialiased."""
    size = CANVAS * SS
    half = BODY * SS / 2.0
    axis = np.arange(size, dtype=np.float64) + 0.5 - size / 2.0
    norm = np.abs(axis / half) ** SQUIRCLE_N
    inside = (norm[None, :] + norm[:, None]) <= 1.0
    mask = Image.fromarray((inside * 255).astype("uint8"), mode="L")
    # Box averaging gives the true pixel coverage.
    return mask.resize((CANVAS, CANVAS), Image.BOX)


def gradient_fill() -> Image.Image:
    t = np.linspace(0.0, 1.0, CANVAS)[:, None]
    top = np.array(FILL_TOP, dtype=np.float64)[None, :]
    bottom = np.array(FILL_BOTTOM, dtype=np.float64)[None, :]
    column = top * (1.0 - t) + bottom * t
    plane = np.repeat(column[:, None, :], CANVAS, axis=1)
    return Image.fromarray(plane.round().astype("uint8"), mode="RGB")


def glyph_alpha() -> Image.Image:
    """Glyph alpha: edges are semi-transparent, nodes are opaque."""
    x0, y0, x1, y1 = GLYPH_SOURCE_BOX
    scale = (BODY * GLYPH_WIDTH_RATIO) / (x1 - x0)
    cx, cy = (x0 + x1) / 2.0, (y0 + y1) / 2.0

    def place(point: tuple[float, float]) -> tuple[float, float]:
        px, py = point
        return (
            ((px - cx) * scale + CANVAS / 2.0) * SS,
            ((py - cy) * scale + CANVAS / 2.0) * SS,
        )

    size = CANVAS * SS
    edges = Image.new("L", (size, size), 0)
    nodes = Image.new("L", (size, size), 0)
    edge_draw = ImageDraw.Draw(edges)
    node_draw = ImageDraw.Draw(nodes)

    for a, b in EDGES:
        edge_draw.line(
            [place(NODES[a][0]), place(NODES[b][0])],
            fill=255,
            width=round(EDGE_WIDTH * scale * SS),
        )
    for center, radius in NODES:
        px, py = place(center)
        r = radius * scale * SS
        node_draw.ellipse([px - r, py - r, px + r, py + r], fill=255)

    merged = np.maximum(
        np.asarray(edges, dtype=np.float64) * EDGE_OPACITY,
        np.asarray(nodes, dtype=np.float64),
    )
    glyph = Image.fromarray(merged.round().astype("uint8"), mode="L")
    return glyph.resize((CANVAS, CANVAS), Image.BOX)


def render() -> Image.Image:
    body_mask = squircle_mask()

    icon = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    icon.paste(gradient_fill(), (0, 0), body_mask)

    glyph = Image.new("RGBA", (CANVAS, CANVAS), GLYPH_COLOR + (0,))
    glyph.putalpha(glyph_alpha())
    icon = Image.alpha_composite(icon, glyph)

    # The shadow goes under the shape and must fit in the transparent margin.
    shadow = Image.new("L", (CANVAS, CANVAS), 0)
    shadow.paste(body_mask, (0, SHADOW_OFFSET))
    shadow = shadow.filter(ImageFilter.GaussianBlur(SHADOW_BLUR))
    shadow = shadow.point(lambda v: round(v * SHADOW_OPACITY))
    layer = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    layer.putalpha(shadow)

    return Image.alpha_composite(layer, icon)


def main() -> int:
    if shutil.which("iconutil") is None:
        print("iconutil (macOS) is required", file=sys.stderr)
        return 1

    root = Path(__file__).resolve().parent.parent
    target = root / "src-tauri" / "icons" / "icon.icns"
    icon = render()

    with tempfile.TemporaryDirectory() as tmp:
        iconset = Path(tmp) / "icon.iconset"
        iconset.mkdir()
        for name, size in ICNS_SIZES:
            icon.resize((size, size), Image.LANCZOS).save(iconset / name)
        subprocess.run(
            ["iconutil", "-c", "icns", str(iconset), "-o", str(target)],
            check=True,
        )
        icon.save(root / "src-tauri" / "icons" / "icon-macos-1024.png")

    print(f"done: {target}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
