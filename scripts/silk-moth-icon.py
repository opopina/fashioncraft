#!/usr/bin/env python3
"""Draw FashionCraft's first app icon, kept for the record (the vicuña replaced it): a silk moth
(Bombyx mori, the animal that gives silk) as a line engraving, ivory and black ink on an amber
rounded square.

The moth is drawn as a field of tones (wings with veins and cross lines, a furry body,
feathery antennae), then engraved: parallel lines whose width follows the tone, running out
from each wing's root and across the body, with a second crossing set in the darkest parts.
Everything is computed here; no image is read.

    python3 scripts/silk-moth-icon.py [OUTPUT.png] [SIZE] [RRGGBB]

Needs numpy, scipy and Pillow. Writes a SIZE x SIZE PNG (1024 by default) with transparent
corners outside the tile.
"""
import math
import sys

import numpy as np
from PIL import Image, ImageDraw
from scipy import ndimage

AMBER = (0xF2, 0xA1, 0x3A)
IVORY = (0xF4, 0xEF, 0xE2)
INK = (0x16, 0x13, 0x10)
# The drawing is made this many times larger than the output, then reduced.
SUPER = 2


def bezier(points, steps=48):
    """Points along a chain of cubic Bézier segments [p0, c1, c2, p1, c1, c2, p2, ...]."""
    out = []
    for k in range(0, len(points) - 1, 3):
        p0, c1, c2, p1 = (np.array(p, float) for p in points[k : k + 4])
        for t in np.linspace(0, 1, steps, endpoint=False):
            u = 1 - t
            out.append(u**3 * p0 + 3 * u * u * t * c1 + 3 * u * t * t * c2 + t**3 * p1)
    out.append(np.array(points[-1], float))
    return out


class Canvas:
    """Normalised coordinates: x right and y up, both from -1 to 1 across the tile."""

    def __init__(self, size):
        self.n = size
        j, i = np.meshgrid(np.arange(size), np.arange(size))
        self.x = (j + 0.5) / size * 2 - 1
        self.y = 1 - (i + 0.5) / size * 2
        self.px = size / 2  # pixels per unit

    def pixel(self, p):
        return ((p[0] + 1) / 2 * self.n, (1 - p[1]) / 2 * self.n)

    def polygon(self, points):
        img = Image.new("L", (self.n, self.n), 0)
        ImageDraw.Draw(img).polygon([self.pixel(p) for p in points], fill=255)
        return np.asarray(img, float) / 255

    def lines(self, segments, width):
        img = Image.new("L", (self.n, self.n), 0)
        d = ImageDraw.Draw(img)
        for a, b, w in segments:
            d.line([self.pixel(a), self.pixel(b)], fill=255, width=max(1, round(w * width)))
        return np.asarray(img, float) / 255


def mirrored(points):
    return [(-x, y) for x, y in points]


def gauss(v, width):
    return np.exp(-((v / width) ** 2))


def wobble(c, amount, seed):
    """A smooth hand tremor for the engraved lines."""
    rng = np.random.default_rng(seed)
    out = np.zeros_like(c.x)
    for _ in range(6):
        fx, fy = rng.uniform(2, 9, 2)
        ph = rng.uniform(0, 2 * math.pi)
        out += np.sin(fx * c.x * math.pi + fy * c.y * math.pi + ph)
    return out * amount / 6


def engrave(phase, width):
    """Ink where `phase` is within `width`/2 of a whole number: lines one unit of phase apart."""
    dist = np.abs((phase + 0.5) % 1.0 - 0.5) * 2
    return (dist < np.clip(width, 0, 0.96)).astype(float)


def draw(size):
    c = Canvas(size * SUPER)
    x, y = c.x, c.y
    # The moth, a little larger than the tile so its wing tips are cut by the edges.
    scale, lift = 1.13, -0.17
    x, y = x / scale, (y - lift) / scale

    # --- shapes (right side; the left mirrors them) ---------------------------------------
    fore_r = bezier(
        [
            (0.07, 0.31),
            (0.34, 0.53), (0.70, 0.68), (0.99, 0.63),
            (1.03, 0.59), (0.99, 0.52), (0.93, 0.50),
            (0.88, 0.36), (0.80, 0.12), (0.66, 0.02),
            (0.46, -0.02), (0.22, 0.06), (0.09, 0.17),
            (0.06, 0.22), (0.06, 0.27), (0.07, 0.31),
        ]
    )
    hind_r = bezier(
        [
            (0.07, 0.19),
            (0.33, 0.22), (0.62, 0.17), (0.75, 0.05),
            (0.87, -0.12), (0.72, -0.37), (0.50, -0.42),
            (0.30, -0.46), (0.12, -0.34), (0.08, -0.17),
            (0.06, -0.05), (0.06, 0.10), (0.07, 0.19),
        ]
    )
    scaled = lambda pts: [((px * scale), (py * scale + lift)) for px, py in pts]  # noqa: E731
    fore = np.maximum(c.polygon(scaled(fore_r)), c.polygon(scaled(mirrored(fore_r))))
    hind = np.maximum(c.polygon(scaled(hind_r)), c.polygon(scaled(mirrored(hind_r))))
    hind = hind * (1 - fore)

    # Body: head, thorax and a tapering abdomen.
    head = ((x / 0.072) ** 2 + ((y - 0.435) / 0.068) ** 2 <= 1).astype(float)
    thorax = ((x / 0.118) ** 2 + ((y - 0.28) / 0.135) ** 2 <= 1).astype(float)
    t = np.clip((0.17 - y) / 0.62, 0, 1)
    half = 0.108 * np.sin(math.pi * (0.12 + 0.88 * t)) ** 0.55 * (1 - 0.5 * t)
    abdomen = ((np.abs(x) <= half) & (y <= 0.19) & (y >= -0.45)).astype(float)
    body = np.clip(head + thorax + abdomen, 0, 1)
    fore = fore * (1 - body)
    hind = hind * (1 - body)

    # Antennae: feathery combs from the head, curving up and out.
    segments = []
    for side in (1, -1):
        shaft = bezier([(0.03, 0.48), (0.07, 0.60), (0.14, 0.74), (0.30, 0.84)], steps=60)
        for k in range(len(shaft) - 1):
            a, b = shaft[k], shaft[k + 1]
            segments.append(((side * a[0] * scale, a[1] * scale + lift),
                             (side * b[0] * scale, b[1] * scale + lift), 1.0))
        for k in range(4, len(shaft) - 2, 2):
            p = shaft[k]
            d = shaft[k + 1] - shaft[k - 1]
            d = d / np.linalg.norm(d)
            n = np.array([-d[1], d[0]])
            length = 0.06 * (1 - k / len(shaft)) + 0.016
            for s in (1, -1):
                tip = p + (n * s * 0.85 + d * 0.5) * length
                segments.append(((side * p[0] * scale, p[1] * scale + lift),
                                 (side * tip[0] * scale, tip[1] * scale + lift), 0.55))
    antennae = c.lines(segments, width=0.0095 * c.px)

    moth = np.clip(fore + hind + body, 0, 1)

    # --- tones: 0 paper, 1 ink -----------------------------------------------------------
    ax = np.abs(x)
    # Each wing's root, where its lines start.
    root = (0.08, 0.26)
    rx, ry = ax - root[0], y - root[1]
    r = np.hypot(rx, ry)
    theta = np.arctan2(ry, rx)

    # Distance in from each part's edge, for margins and outlines.
    def inset(part):
        return ndimage.distance_transform_edt(part > 0.5) / c.px / scale

    fore_in, hind_in = inset(fore), inset(hind)

    wing = np.zeros_like(x)
    wing += 0.08
    wing += 0.5 * np.exp(-r / 0.12)  # furry roots
    # Light falls on the leading edge; the trailing part of each wing is in shade.
    wing += fore * 0.22 * np.clip((0.35 - theta) / 0.9, 0, 1)
    # Cross lines of the forewing (antemedial, postmedial), slightly waved, and a
    # submarginal line along each wing's outer edge.
    wave = 0.018 * np.sin(9 * theta)
    wing += fore * 0.55 * gauss(r - 0.34 - wave, 0.013)
    wing += fore * 0.45 * gauss(r - 0.66 - wave, 0.012)
    wing += fore * 0.35 * gauss(fore_in - 0.045, 0.01) * (r > 0.5)
    wing += hind * 0.35 * gauss(hind_in - 0.05, 0.012) * (r > 0.35)
    wing += 0.25 * np.exp(-np.minimum(np.where(fore > 0.5, fore_in, 9), np.where(hind > 0.5, hind_in, 9)) / 0.018)
    # The hooked tip, shaded underneath.
    wing += fore * 0.45 * np.exp(-np.hypot(ax - 0.86, y - 0.56) / 0.07)
    # Discal lunule: a thin crescent opening outward.
    ring = np.hypot(ax - 0.55, y - 0.39)
    wing += fore * 0.65 * gauss(ring - 0.032, 0.006) * (ax < 0.545)
    # Veins: faint rays out from the root.
    veins = np.zeros_like(x)
    for angle in np.radians([38, 30, 22, 14, 6, -4, -16, -30, -48, -64]):
        dist = np.abs(-math.sin(angle) * rx + math.cos(angle) * ry)
        along = math.cos(angle) * rx + math.sin(angle) * ry
        veins += gauss(dist, 0.0045) * (along > 0.14)
    wing += 0.3 * np.clip(veins, 0, 1)
    # Under the forewing the hindwing is in shade, deeper toward its lower edge.
    shade = ndimage.gaussian_filter(np.roll(fore, (int(0.03 * c.px), int(0.01 * c.px)), (0, 1)), 0.02 * c.px)
    wing += hind * (0.2 + 0.45 * shade + 0.25 * np.clip(-y - 0.1, 0, 1))

    tone = wing * (fore + hind)

    # Body: a white, furry thorax; a rounded, banded abdomen; a small head with its eyes.
    xn = np.clip(ax / np.maximum(half, 1e-3), 0, 1)
    body_tone = 0.1 + 0.55 * xn**2
    bands = (0.5 + 0.5 * np.cos(2 * math.pi * (y - 0.16) / 0.082)) ** 6
    body_tone += abdomen * (0.35 * bands + 0.15 * np.clip(-y, 0, 1))
    thorax_r = (x / 0.118) ** 2 + ((y - 0.28) / 0.135) ** 2
    body_tone = np.where(thorax > 0, 0.14 + 0.75 * thorax_r**1.5, body_tone)
    body_tone = np.where(head > 0, 0.3 + 0.3 * ((x / 0.072) ** 2), body_tone)
    eyes = ((ax - 0.058) ** 2 + (y - 0.445) ** 2 <= 0.019**2).astype(float)
    tone += body * body_tone
    tone = np.clip(tone, 0, 1)

    # --- engraving ------------------------------------------------------------------------
    spacing = 0.017
    tremor = wobble(c, 0.18, 7)
    rng = np.random.default_rng(3)

    def streaks(u, v, density, seed):
        """Short strokes: noise drawn out along u, thresholded by `density` (0 none, 1 all)."""
        grid = np.random.default_rng(seed).random((256, 256))
        field = ndimage.map_coordinates(grid, [v % 256, u % 256], order=1, mode="wrap")
        return (field < density * 0.9).astype(float)

    # Wings: lines out from the root and, where dark, arcs round it; fur at the roots.
    radial = theta * 0.6 / spacing + tremor
    arcs = r / spacing + tremor
    root_fur = np.clip((0.17 - r) / 0.06, 0, 1)
    # Highlights stay clean paper; lines start in the light half-tones.
    wing_ink = engrave(radial, (tone - 0.14) * 1.35) * (1 - root_fur)
    wing_ink = np.maximum(wing_ink, engrave(arcs, (tone - 0.42) * 1.7) * (1 - root_fur))
    fur = streaks(r * 18 / 0.1, theta * 70, tone * 0.9, 11)
    wing_ink = np.maximum(wing_ink, fur * root_fur)
    # Body: lines across, bowed like rings, and lines along where dark; the thorax is fur.
    across = (y + 0.05 * (x / 0.11) ** 2) / (spacing * 0.8) + tremor
    along = x / (spacing * 0.8) + tremor
    body_ink = engrave(across, (tone - 0.1) * 1.3)
    body_ink = np.maximum(body_ink, engrave(along, (tone - 0.45) * 1.6))
    tuft = np.hypot(x, (y - 0.26) * 0.9)
    tuft_angle = np.arctan2(y - 0.26, x)
    thorax_fur = streaks(tuft * 160, tuft_angle * 260, tone * 1.15, 5)
    body_ink = np.where(thorax > 0, thorax_fur, body_ink)

    ink = wing_ink * (fore + hind) + body_ink * body
    ink = np.maximum(ink, eyes)
    ink = np.maximum(ink, antennae)
    # A firm outline round every part.
    for part in (fore, hind, body):
        inner = ndimage.binary_erosion(part > 0.5, iterations=max(1, int(0.0085 * c.px)))
        ink = np.maximum(ink, (part > 0.5) & ~inner)
    ink = np.clip(ink, 0, 1)

    # --- composite on the amber tile ------------------------------------------------------
    n = c.n
    rgb = np.empty((n, n, 3), float)
    for k in range(3):
        paper = np.where(moth > 0.5, IVORY[k], AMBER[k])
        rgb[..., k] = paper * (1 - ink) + INK[k] * ink
    tile = Image.new("L", (n, n), 0)
    ImageDraw.Draw(tile).rounded_rectangle([0, 0, n - 1, n - 1], radius=0.22 * n, fill=255)
    alpha = np.asarray(tile, float)
    image = Image.fromarray(np.dstack([rgb, alpha]).astype(np.uint8), "RGBA")
    return image.resize((size, size), Image.LANCZOS)


def main():
    global AMBER
    out = sys.argv[1] if len(sys.argv) > 1 else "assets/branding/silk-moth-icon.png"
    size = int(sys.argv[2]) if len(sys.argv) > 2 else 1024
    if len(sys.argv) > 3:  # another tile colour, as RRGGBB, for trying alternatives
        AMBER = tuple(int(sys.argv[3][k : k + 2], 16) for k in (0, 2, 4))
    # Small icons are reductions of the full drawing, so the engraving thins out evenly.
    draw(max(size, 1024)).resize((size, size), Image.LANCZOS).save(out)
    print(out)


if __name__ == "__main__":
    main()
