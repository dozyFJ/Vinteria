"""Draws the Winteria launcher icon: a yin-yang of the Pulse night and day colours with a W that flips colour
across the curve. Writes android/res/mipmap-*/ic_launcher.png and ic_launcher_round.png plus assets/winteria_icon.png.

Run from the project root:  python tools/make_android_icon.py
"""
from pathlib import Path

from PIL import Image, ImageDraw

DARK = (0x1A, 0x1E, 0x23)
LIGHT = (0xDC, 0xE5, 0xEE)
RIM = (0x55, 0x5F, 0x69)
SS = 4  # supersampling for smooth edges
BASE = 1024
SIZES = {"mdpi": 48, "hdpi": 72, "xhdpi": 96, "xxhdpi": 144, "xxxhdpi": 192}


def yin_yang(size, a, b):
    img = Image.new("RGB", (size, size), a)
    d = ImageDraw.Draw(img)
    c, r = size / 2, size / 2
    d.pieslice([0, 0, size - 1, size - 1], 90, 270, fill=b)  # left half
    d.ellipse([c - r / 2, 0, c + r / 2, r], fill=b)  # top bulge
    d.ellipse([c - r / 2, r, c + r / 2, size], fill=a)  # bottom bulge
    return img


def w_mask(size):
    m = Image.new("L", (size, size), 0)
    d = ImageDraw.Draw(m)
    c, r = size / 2, size / 2
    pts = [(-0.60, -0.36), (-0.31, 0.40), (0.0, -0.10), (0.31, 0.40), (0.60, -0.36)]
    pts = [(c + x * r, c + y * r) for x, y in pts]
    w = int(r * 0.17)
    d.line(pts, fill=255, width=w, joint="curve")
    for x, y in pts:
        d.ellipse([x - w / 2, y - w / 2, x + w / 2, y + w / 2], fill=255)
    return m


def icon(px):
    s = px * SS
    inner = int(s * 0.90)
    yy = yin_yang(inner, DARK, LIGHT)
    yy_inv = yin_yang(inner, LIGHT, DARK)
    face = Image.composite(yy_inv, yy, w_mask(inner))
    out = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(out)
    pad = (s - inner) // 2
    ring = max(SS, int(s * 0.018))
    d.ellipse([pad - ring, pad - ring, pad + inner + ring - 1, pad + inner + ring - 1], fill=RIM)
    circle = Image.new("L", (inner, inner), 0)
    ImageDraw.Draw(circle).ellipse([0, 0, inner - 1, inner - 1], fill=255)
    out.paste(face, (pad, pad), circle)
    return out.resize((px, px), Image.LANCZOS)


def main():
    root = Path(__file__).resolve().parent.parent
    for name, px in SIZES.items():
        folder = root / "android" / "res" / f"mipmap-{name}"
        folder.mkdir(parents=True, exist_ok=True)
        img = icon(px)
        img.save(folder / "ic_launcher.png")
        img.save(folder / "ic_launcher_round.png")
    icon(BASE // 2).save(root / "assets" / "winteria_icon.png")


if __name__ == "__main__":
    main()
