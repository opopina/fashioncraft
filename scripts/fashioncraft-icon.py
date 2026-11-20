#!/usr/bin/env python3
"""Make the FashionCraft app icon from its source drawing: a vicuña, engraved in ink on paper,
on FashionCraft's fuchsia, in the ArtCraft apps' icon template.

The source (`assets/branding/source/vicuna-chatgpt.webp`) was drawn with ChatGPT on an orange
tile with white round it and rounder corners than the template's. This program keeps the
drawing and remakes the rest:

- it finds the source's tile (from transparency, or, when there is none, from the saturated
  field and the shape of its corners) and the vicuña in it;
- it recolours by unmixing: each pixel is read as so much of the source's ink, paper and field
  and painted with the same shares of the template's ink (#0b0b0c) and paper (#efe9dc) and the
  app colour, so fine strokes, antialiased edges and lashes over the field keep their weight;
- where the source's rounder corners cut the drawing, it continues the drawing a little past
  them by reflection, so the template's corners find fleece there too;
- it frames the source's square, ears a little below the top, the body running off the bottom;
- the tile is a 512-unit rounded square with 112-unit corners, and, as in the ArtCraft icons,
  the drawing stops 5 units short of its edge, leaving a thin rim of the field all round.

    python3 scripts/fashioncraft-icon.py [SOURCE] [FIELD RRGGBB]

Writes assets/branding/fashioncraft-icon.png (1024 px), fashioncraft-icon-128.png and
fashioncraft-icon-macos.png (the tile on Apple's 824/1024 grid). Needs numpy, SciPy and Pillow.
"""
import sys

import numpy as np
from PIL import Image, ImageDraw
from scipy import ndimage

INK = np.array([0x0B, 0x0B, 0x0C], float)
PAPER = np.array([0xEF, 0xE9, 0xDC], float)
FUCHSIA = "DD2CCE"
# The template: a 512-unit tile with 112-unit corners; the drawing stops 5 units short.
CORNER, RIM = 112 / 512, 5 / 512


def tile_mask(size):
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        [0, 0, size - 1, size - 1], radius=size * CORNER, fill=255
    )
    return mask


def tile_alpha(rgb):
    """For a source with no transparency (white round its tile): an alpha that is opaque inside
    the tile, found from the saturated field and the shape of its top corners, mirrored below."""
    hi, lo = rgb.max(-1), rgb.min(-1)
    field = np.where(hi > 0, (hi - lo) / np.maximum(hi, 1e-6), 0) > 0.45
    ys, xs = np.where(field)
    y0, y1, x0, x1 = ys.min(), ys.max(), xs.min(), xs.max()
    profile = []
    for d in range(int(0.4 * (y1 - y0))):
        row = np.where(field[y0 + d, x0 : x1 + 1])[0]
        if not len(row):
            profile.append(profile[-1] if profile else (x1 - x0) // 2)
            continue
        inset = min(row.min(), (x1 - x0) - row.max())
        profile.append(min(inset, profile[-1]) if profile else inset)
        if inset <= 0:
            break
    alpha = np.zeros(rgb.shape[:2])
    for y in range(y0, y1 + 1):
        d = min(y - y0, y1 - y)
        inset = profile[d] if d < len(profile) else 0
        alpha[y, x0 + inset : x1 - inset + 1] = 255
    return alpha


def figure_and_colours(source, field):
    """The cropped source's opaque alpha, the vicuña's silhouette and the recoloured image."""
    im = np.asarray(Image.open(source).convert("RGBA")).astype(float)
    if im[..., 3].min() > 250:
        im[..., 3] = tile_alpha(im[..., :3] / 255)
    ys, xs = np.where(im[..., 3] > 50)
    im = im[ys.min() : ys.max() + 1, xs.min() : xs.max() + 1]
    rgb, a = im[..., :3] / 255, im[..., 3]
    hi, lo = rgb.max(-1), rgb.min(-1)
    sat = np.where(hi > 0, (hi - lo) / np.maximum(hi, 1e-6), 0)
    lum = rgb @ np.array([0.299, 0.587, 0.114])

    fig = (a > 200) & (sat < 0.35)
    fig = ndimage.binary_fill_holes(ndimage.binary_closing(fig, iterations=4))
    labels, n = ndimage.label(fig)
    sizes = ndimage.sum(fig, labels, range(1, n + 1))
    fig = labels == 1 + int(np.argmax(sizes))
    # Recolour by unmixing: each pixel is read as so much of the source's ink, paper and field,
    # and painted with the same shares of the template's ink and paper and the app colour. Fine
    # strokes, antialiased edges and lashes over the field keep their weight.
    opaque = a > 200
    src_field = np.median(rgb[opaque & (sat > 0.45)], axis=0)
    inside = opaque & fig & (sat < 0.35)
    src_ink = np.median(rgb[inside & (lum <= np.percentile(lum[inside], 2))], axis=0)
    src_paper = np.median(rgb[inside & (lum >= np.percentile(lum[inside], 98))], axis=0)
    basis = np.stack([src_ink - src_field, src_paper - src_field], axis=1)
    shares, *_ = np.linalg.lstsq(basis, (rgb - src_field).reshape(-1, 3).T, rcond=None)
    k, p = shares.reshape(2, *rgb.shape[:2])
    f = 1 - k - p
    weights = np.clip(np.stack([k, p, f], -1), 0, None)
    # Specks of a colour too faint to see are noise from compression: drop them.
    weights[weights < 0.04] = 0
    weights /= np.maximum(weights.sum(-1, keepdims=True), 1e-6)
    colour = (
        weights[..., :1] * INK + weights[..., 1:2] * PAPER + weights[..., 2:] * field
    )
    colour[~opaque] = field

    # The source's own tile cut the drawing (the fleece) with corners rounder than the
    # template's. Past that edge, near where it cut the drawing, continue the drawing by
    # reflecting it across the edge, so the template's tile finds fleece there too.
    h, w = fig.shape
    pad = int(0.15 * h)
    tile = np.zeros((h, w), bool)
    for y in range(h):
        xs_ = np.where(a[y] > 50)[0]
        if len(xs_):
            tile[y, xs_.min() : xs_.max() + 1] = True
    tile = np.pad(tile, pad)
    colour = np.pad(colour, ((pad, pad), (pad, pad), (0, 0)))
    colour[~tile] = field
    fig = np.pad(fig, pad)
    cut = fig & ~ndimage.binary_erosion(tile, iterations=3)
    near_cut = ndimage.distance_transform_edt(~cut) < 0.08 * h
    _, (iy, ix) = ndimage.distance_transform_edt(~tile, return_indices=True)
    yy, xx = np.indices(tile.shape)
    ry = np.clip(2 * iy - yy, 0, tile.shape[0] - 1)
    rx = np.clip(2 * ix - xx, 0, tile.shape[1] - 1)
    extend = ~tile & near_cut & tile[ry, rx]
    colour[extend] = colour[ry[extend], rx[extend]]
    fig = fig | (extend & fig[ry, rx])
    a = np.pad(a, pad)
    return a, fig, colour, pad


def frame(fig, pad):
    """The source's own square, centred on it, moved up when needed so the ears sit a little
    below the top; the body runs off the bottom."""
    h, w = fig.shape
    h, w = h - 2 * pad, w - 2 * pad
    side = min(h, w)
    ear_top = np.where(fig.any(1))[0].min() - pad
    top = min(h - side, int(ear_top - 0.04 * side))
    left = (w - side) // 2
    return side, pad + left, pad + top


def main():
    source = sys.argv[1] if len(sys.argv) > 1 else "assets/branding/source/vicuna-chatgpt.webp"
    hexa = sys.argv[2] if len(sys.argv) > 2 else FUCHSIA
    field = np.array([int(hexa[k : k + 2], 16) for k in (0, 2, 4)], float)
    _, fig, colour, pad = figure_and_colours(source, field)
    side, left, top = frame(fig, pad)
    colour = colour[top : top + side, left : left + side]

    n = 2048
    mask = tile_mask(n)
    image = np.asarray(
        Image.fromarray(colour.clip(0, 255).astype(np.uint8), "RGB").resize((n, n), Image.LANCZOS),
        float,
    )
    # The thin rim: the outside counts as edge on every side, not only at the corners.
    inset = ndimage.distance_transform_edt(np.pad(np.asarray(mask) > 127, 1))[1:-1, 1:-1]
    keep = np.clip((inset - RIM * n + 1.5) / 3, 0, 1)[..., None]
    image = field * (1 - keep) + image * keep
    icon = Image.fromarray(image.clip(0, 255).astype(np.uint8), "RGB")
    icon.putalpha(mask)
    for size, name in [(1024, "fashioncraft-icon.png"), (128, "fashioncraft-icon-128.png")]:
        path = f"assets/branding/{name}"
        icon.resize((size, size), Image.LANCZOS).save(path)
        print(path)
    # macOS draws app icons on Apple's grid: the tile 824 of 1024 wide, centred, clear round it.
    # The Dock shows this one for the running app and the app bundle's icon is made from it.
    grid = Image.new("RGBA", (1024, 1024), (0, 0, 0, 0))
    grid.paste(icon.resize((824, 824), Image.LANCZOS), (100, 100))
    path = "assets/branding/fashioncraft-icon-macos.png"
    grid.save(path)
    print(path)


if __name__ == "__main__":
    main()
