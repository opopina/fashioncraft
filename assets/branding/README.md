# Artwork

## FashionCraft icon

`fashioncraft-icon.png` (1024 × 1024, the native window icon) and `fashioncraft-icon-128.png`
(128 × 128, the size app catalogs show) are a **vicuña**, engraved in ink on paper, on
FashionCraft's fuchsia (`#dd2cce`). The vicuña gives the finest fibre there is and lives in
Ecuador's Chimborazo reserve. The icon follows the ArtCraft apps' icon template: a portrait on a
full-bleed field, the body running off the tile, exactly three colours (ink `#0b0b0c`, paper
`#efe9dc` and the app colour), a 512-unit tile with 112-unit corners, the drawing stopping
5 units short of the edge. None of their artwork is reused.

The drawing, `source/vicuna-chatgpt.webp`, was made by Ricardo Rivadeneira with ChatGPT (OpenAI)
on 2026-11-20 from prompts written for this repository (an original nineteenth-century-style
engraving portrait of a vicuña with its fleecy shoulders, three flat colours, no text).
`scripts/fashioncraft-icon.py` turns it into the icon: it finds the drawing on the source's tile,
recolours it by unmixing each pixel into ink, paper and field and repainting those shares in the
template's colours (so fine strokes and antialiasing survive), continues the fleece a little past
the source's rounder corners, frames it with the ears whole and the body running off the bottom,
and adds the template's corners and thin rim. To remake the icons:

```sh
python3 scripts/fashioncraft-icon.py
```

The icon is distributed under this repository's [MIT License](../../LICENSE).

Earlier icons, kept for the record: `silk-moth-icon.png`, a silk moth on amber drawn entirely by
`scripts/silk-moth-icon.py`, and `fashioncraft-icon.svg`, a dress on a hanger.

## ClothCraft icon (inherited)

`clothcraft-icon.png` is the original generated ClothCraft ram icon, used by ClothCraft's README and native window; FashionCraft keeps the file for the record but no longer uses it. The ram and fleece refer to wool and textile craft. The PNG is 1254 × 1254 RGBA with transparent corners around a turquoise rounded tile. Native window icons are supplied through eframe; this does not embed a Windows Explorer icon resource or provide an installer.

## Visual references

Viewed the official [ArtCraft app lineup](https://getartcraft.com/apps) on 2026-10-14, including PhotoCraft, VectorCraft and FilmCraft icons. The shared visual language is a monochrome natural-history-style animal engraving on a saturated rounded square. The [PhotoCraft](https://getartcraft.com/images/apps/photocraft/icon.webp) and [VectorCraft](https://getartcraft.com/images/apps/vectorcraft/icon.webp) image URLs identify the references; their files are not bundled or reused. ClothCraft is independent and not an ArtCraft product.

For the FashionCraft icon the app icons in `storytold/craft-launcher` (`assets/app-icons/`) were
viewed again on 2026-11-20 for the style only. They are ArtCraft trademarks, not open source; none
is copied, traced or redrawn here.

## Generation record

Generated with Codex's built-in `image_gen` tool, followed by one edge-cleanup edit using the same tool. No external API key or fallback CLI was used. The generated PNG was copied into this directory; the original remains outside the repository. The project artwork is distributed under the repository's [MIT License](../../LICENSE) where applicable; this does not change the rights in the external visual references.

### Initial prompt

```text
Use case: logo-brand. Asset type: final desktop app icon for ClothCraft, an independent garment pattern drafting and cloth simulation app.
Create one original square 1024x1024 application icon. Match the broad visual language observed in ArtCraft's PhotoCraft / VectorCraft / FilmCraft app family: a detailed black-ink and warm ivory vintage animal engraving, strongly silhouetted, sitting on a saturated flat-color rounded square. Their icons have tactile fine crosshatching, naturalistic expressive animals, high contrast, clean color fields and no lettering.
Subject: a majestic wool ram's head and upper fleece, turned slightly to the left in a three-quarter portrait, two beautiful curled ridged horns clearly visible, intelligent calm eye, dense curly wool. This is an original animal illustration, not a redraw of any existing logo. Strong compact silhouette; head and horns fill most of the tile, fleece reaches the bottom edge. Crisp nineteenth-century wood engraving / natural-history etching, black outlines and ivory highlights, not a photo, not 3D, not cute cartoon.
Backdrop: one flat saturated deep turquoise / teal rounded-square tile, in harmony with ClothCraft's cyan UI accent. Rounded corners about 20% radius. Keep horns inside tile, ample small-icon readability. Thin transparent margin outside the rounded tile; genuine alpha in the corner cutouts, no checkerboard drawn. No surrounding cast shadow or background.
Composition: one icon only, front-facing tile, no perspective, centered balanced head and horns, simplified large light/dark masses readable at 32px with fine engraved detail at full size.
No text, letters, monograms, sewing tools, extra objects, watermark, app name, border, mockup, multiple variants or collage. Output a production-ready PNG asset with alpha outside the colored tile.
```

### Cleanup prompt

```text
Use case: precise-object-edit. This is the new ClothCraft ram app icon. Preserve the ram engraving, its face, curled horns, fleece, palette, composition and rounded-square silhouette. Change only the OUTER CUTOUT EDGE: remove every stray colored fleck/pixel outside the rounded square (especially above, left/right edges and below the bottom). Make the rounded-square edge smooth, clean and antialiased, with uniform transparent padding all around. Inside the tile preserve the artwork and flat turquoise background. Genuine alpha transparency outside. No shadow, no text, no new elements, no redesign. Production app-icon edge cleanup only.
```
