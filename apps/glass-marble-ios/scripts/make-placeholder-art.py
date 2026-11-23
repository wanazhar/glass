#!/usr/bin/env python3
"""Generate the placeholder app icon and launch images for Glass Marble.

Draws a navy field, a soft sphere ("marble") and an "M" glyph using PIL only —
no downloads, no external assets. Outputs:

  GlassMarble/Assets.xcassets/AppIcon.appiconset/AppIcon-1024.png  (opaque 1024)
  Config/Launch/MarbleLaunch@2x.png                               (860x1864)
  Config/Launch/MarbleLaunch@3x.png                               (1290x2796)
"""

import os
import sys

from PIL import Image, ImageDraw, ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
APP_DIR = os.path.dirname(HERE)
ICON_OUT = os.path.join(
    APP_DIR, "GlassMarble/Assets.xcassets/AppIcon.appiconset/AppIcon-1024.png"
)
LAUNCH_DIR = os.path.join(APP_DIR, "Config/Launch")

NAVY = (10, 18, 31)
SPHERE = (214, 226, 240)
SHADE = (150, 168, 196)
GLINT = (244, 248, 253)
INK = (10, 18, 31)


def font(size: int, bold: bool = True) -> ImageFont.FreeTypeFont:
    candidates = [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
    ]
    for path in candidates:
        if os.path.exists(path):
            return ImageFont.truetype(path, size)
    return ImageFont.load_default(size)


def draw_sphere(draw: ImageDraw.ImageDraw, cx: int, cy: int, r: int) -> None:
    for i in range(r, 0, -1):
        t = i / r
        colour = tuple(
            int(SHADE[c] + (SPHERE[c] - SHADE[c]) * (1 - t) ** 1.6) for c in range(3)
        )
        draw.ellipse([cx - i, cy - i, cx + i, cy + i], fill=colour)
    gr = int(r * 0.34)
    gx, gy = cx - int(r * 0.36), cy - int(r * 0.40)
    draw.ellipse([gx - gr, gy - gr, gx + gr, gy + gr], fill=GLINT)


def make_icon() -> None:
    size = 1024
    image = Image.new("RGB", (size, size), NAVY)
    draw = ImageDraw.Draw(image)
    cx, cy, r = size // 2, int(size * 0.42), int(size * 0.30)
    draw_sphere(draw, cx, cy, r)
    glyph = font(int(size * 0.34))
    text = "M"
    box = draw.textbbox((0, 0), text, font=glyph)
    tw, th = box[2] - box[0], box[3] - box[1]
    draw.text(
        (cx - tw / 2 - box[0], int(size * 0.66) - th / 2 - box[1]),
        text,
        font=glyph,
        fill=SPHERE,
    )
    os.makedirs(os.path.dirname(ICON_OUT), exist_ok=True)
    image.save(ICON_OUT)


def make_launch(scale: int) -> None:
    # 430x932 pt canvas at @2x/@3x, "Marble" centred in light semibold.
    width, height = 430 * scale, 932 * scale
    image = Image.new("RGB", (width, height), NAVY)
    draw = ImageDraw.Draw(image)
    glyph = font(int(44 * scale))
    text = "Marble"
    box = draw.textbbox((0, 0), text, font=glyph)
    tw, th = box[2] - box[0], box[3] - box[1]
    draw.text(
        ((width - tw) / 2 - box[0], (height - th) / 2 - box[1]),
        text,
        font=glyph,
        fill=SPHERE,
    )
    os.makedirs(LAUNCH_DIR, exist_ok=True)
    image.save(os.path.join(LAUNCH_DIR, f"MarbleLaunch@{scale}x.png"))


def main() -> int:
    make_icon()
    make_launch(2)
    make_launch(3)
    print(f"wrote {ICON_OUT}")
    print(f"wrote {LAUNCH_DIR}/MarbleLaunch@2x.png, MarbleLaunch@3x.png")
    return 0


if __name__ == "__main__":
    sys.exit(main())
