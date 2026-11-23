# Fabric maps, texture sets and the photoreal render

Two look features, written from public PBR conventions (glTF 2.0, UsdPreviewSurface, the MTL
PBR extension) and from what texture tools write as a user sees it. No proprietary format is
read: Substance `.sbsar` archives need Adobe's engine, so the supported route is the maps a
tool exports as images.

## Fabric maps

`FabricTexture` (`src/garment/appearance.rs`) gains, besides its colour image, normal map and
roughness map:

| Field | Meaning |
| --- | --- |
| `metalness_asset_id` | grey, black cloth to white metal; replaces `FabricAppearance::metalness` |
| `opacity_asset_id` | grey times its own alpha; the cloth is cut away under one half (lace, mesh) |
| `displacement_asset_id`, `displacement_mm` | grey height, black low, white high; `displacement_mm` between them (0 to 10 mm) |
| `occlusion_asset_id` | ambient occlusion; darkens light from all around |
| `normal_directx` | the normal map's green points down (DirectX); flipped when read |
| `roughness_is_glossiness` | the roughness map is glossiness; inverted when read |

`FabricAppearance::metalness` (0 to 1) is the single value without a map. Every new field is
optional and skipped when empty, so older files open unchanged and plain fabrics write no new
keys. `MapSlot` names the six maps; `FabricTexture::map`/`map_mut` reach them.

Drawing (`src/cloth.wgsl`, `src/render/fabric_maps.rs`):

- Every map is a layer of the fabric texture array. `LayerKind` stores data maps so sampling
  returns their values, with a DirectX normal map's green already flipped and a glossiness map
  already inverted: the shader reads every map one way.
- The vertex gains two vectors (37 floats): the metalness, opacity, displacement and occlusion
  layers, and the metalness value and displacement height.
- Displacement is parallax occlusion mapping: the surface sits at the map's white and the view
  ray walks 24 steps down to the height field; every later sample uses the shifted coordinates
  (`textureSampleGrad`, so it may run in non-uniform control flow). Without a normal map, the
  height also shades its relief. The simulated mesh never moves.
- Opacity discards fragments under one half; metalness gives F0 = mix(0.04, colour, metal), no
  diffuse part, and a dim stand-in for the room's reflection; occlusion scales the ambient term.

## Texture sets

`src/garment/texture_set.rs` reads file names only. A stem is split into lower-case words at
separators and case changes; trailing resolution, format and "map" words are dropped; the last
one to three words are matched against map names (longest first):

- base colour: basecolor, albedo, diffuse, diff, color, col…
- normal: normal, nrm, nor; `gl`/`opengl` or `dx`/`directx` words set the convention (OpenGL
  when unsaid, the glTF and Blender way)
- roughness, rough, rgh; glossiness, gloss, smoothness (inverted)
- metallic, metalness, metal
- height, displacement, disp, bump (bump last)
- opacity, alpha, transparency
- ao, ambientocclusion, occlusion, mixed_ao

The words before the suffix (without resolutions) name the material; `group` makes one set per
material and keeps the surest file for each map (roughness over glossiness, an OpenGL normal map
over a DirectX one, height over bump). Packed channel maps (`occlusionRoughnessMetallic`, `ARM`,
`MetallicSmoothness`, mask maps) are left out rather than mistaken for one map. PNG, JPEG, TGA
and BMP are read (the `image` crate gains TGA, BMP and HDR, which need no new crates); EXR and
TIFF are reported as not recognised.

In the Fabric inspector, **Texture set › Import… › From a folder… / From files…** stages a set:
the base colour becomes the texture image (keeping the repeat, angle and relief), every map goes
in its row, the DirectX and glossiness boxes are ticked from the names, and a height map starts
at 1 mm. With several materials in a folder, their names are offered to choose from. The
button's tooltip says why `.sbsar` is not supported. Each map row has Image…/Replace…/Remove;
the normal row has *DirectX normal map (flip green)*, the roughness row *Glossiness map
(inverted)*, the displacement row its height. Nothing is committed before **Apply appearance**.

## Exports

`src/texture_maps.rs` gives each exporter the file it needs: the original when it fits, a
converted PNG otherwise (`TextureFile`).

| Map | glTF | USD (UsdPreviewSurface) | OBJ + MTL | FBX |
| --- | --- | --- | --- | --- |
| metalness value | `metallicFactor` | `metallic` | `Pm` | — |
| normal | `normalTexture`, made OpenGL | `normal`, DirectX flipped by scale/bias | `map_Bump`, made OpenGL | `NormalMap`, made OpenGL |
| roughness | packed green | `roughness`, glossiness inverted by scale/bias | `map_Pr`, made roughness | `ShininessExponent` as roughness (Blender's convention) |
| metalness | packed blue | `metallic` | `map_Pm` | `ReflectionFactor` |
| opacity | colour image alpha, `alphaMode` MASK 0.5 | `opacity`, `opacityThreshold` 0.5 | `map_d` | `TransparencyFactor` |
| displacement | none in glTF | `displacement` in metres | `disp` | `DisplacementColor` |
| occlusion | `occlusionTexture` | `occlusion` | none in MTL | none in FBX |

glTF and USDZ only take PNG and JPEG: other formats are re-encoded. The `image` crate is now a
regular dependency so the CPU-only build can convert maps.

Checked by importing all four into Blender 5.2 (headless): every map lands on the Principled
BSDF input listed, except FBX `DisplacementColor`, which Blender's importer ignores with a
warning, and glTF occlusion, which Blender keeps beside the BSDF.

## Photoreal render

**Render › Photoreal render…** (`render.photoreal`) opens a window that refines a path-traced
image while the editor keeps working.

- **Scene.** `Viewport::request_trace_scene` makes the next 3D frame keep what it drew: the
  garment's faces with their thickness and rims, trims, the avatar with its skin and parts, and
  prints, all in the viewport's vertex layout, with the texture array's layers. Diagnostic
  lines (seams, the technical view) are left out. `path_trace::prepare` shares identical
  per-face materials and builds the tree.
- **BVH** (`src/render/bvh.rs`): binned surface-area heuristic, 16 bins, leaves of up to 4
  triangles, flattened depth first (32-byte nodes). The CPU walk (`Bvh::intersect`) picks the
  focus distance and checks the tree against brute force in tests.
- **Integrator** (`src/render/path_trace.wgsl`): one sample per pixel per pass, summed in a
  buffer and tone mapped (ACES fit) on demand. The BSDF is the viewport's: GGX with Smith
  masking and Schlick's Fresnel over Lambert, metals in their own colour, VNDF sampling of the
  specular lobe. Every map, the weave and both face colours are read as in the viewport; the
  displacement map shades as a bump (the path tracer does not move geometry); opacity is
  stochastic, so lace casts patterned shadows. Lights: the viewport's key light as a 1.5° sun
  with soft shadows, the environment (an equirectangular HDR image, importance sampled by
  luminance and combined with BSDF sampling by the power heuristic, or the studio's sky
  colours), bounces (6 by default, Russian roulette after 3) and the floor. The backdrop and the
  floor's fade into it match the viewport; the backdrop colour is inverted through the tone
  curve so it shows the same. Depth of field is a thin lens (aperture radius, focus distance,
  or *Pick on the image*). Samples that come out not finite are dropped and counted (checked by
  bits: fast-math compilers fold `x != x`).
- **Not blocking the editor.** `RenderWorker` runs the tracer on its own thread and its own
  device of the viewport's adapter. A pass is split into dispatches of a few rows aimed at about
  12 ms, so the editor's frames keep their turns. The thread sends the image every 250 ms and
  when done; Save image writes the PNG from the thread. Closing the window drops the thread and
  its GPU memory.
- **Window.** Size in pixels (presets, or the 3D view's proportions; up to 8192), samples (256),
  bounces, exposure, depth of field, environment (HDR image, strength, turn, shown behind the
  scene or not), key light, floor, *Follow the 3D view* (takes the view again 200 ms after the
  camera, cloth or look stop changing) and *Update scene*.

## Tests

- `tests/fabric_maps.rs`: suffixes of Substance Painter, Designer and Sampler exports and of
  texture libraries, conventions, materials, packed maps and unsupported files; folder grouping
  and the surest file per map; the material round trip (all maps, old files, bounds); glTF,
  USD, OBJ and FBX carrying the maps (converted pixels checked).
- `tests/shader.rs`: the path tracer's WGSL validates with naga and its Frame, Node and Material
  layouts match the host (CPU only).
- `src/render/bvh.rs`: every triangle in one leaf inside its bounds; tree hits equal brute force.
- `src/render/path_trace.rs`: the tone curve's inverse, the environment's sampling tables, the
  scene's shared materials.
- `tests/path_trace.rs` (`--ignored`, hardware GPU): a grey sphere in a uniform sky converges to
  its albedo (0.49 for 0.5) with the change between fourfold sample counts halving, no sample
  dropped; a lace scene with every light (HDR-like sky, sun, floor, depth of field, metal,
  textures with normal and opacity maps, weave) converges with no sample dropped.

## The "looks" demo

`FASHIONCRAFT_DEMO=looks FASHIONCRAFT_DEMO_TEXTURES=<folder> [FASHIONCRAFT_DEMO_RENDER=<png>]
[FASHIONCRAFT_DEMO_HDR=<hdr>]` drives the interface: the texture set onto the first piece's
fabric, Apply, simulate, a closer look, Render › Photoreal render, a saved image, focus picked
on the image, and the HDR environment. Only the native file dialogs are steps without the
cursor.

## Limits

- No cloth sheen or light through the cloth in either renderer; the BSDF is a presentation
  model, not a measured textile BRDF.
- Displacement never moves geometry: parallax in the viewport, a bump in the render.
- The render samples textures at full resolution (progressive sampling filters them) and holds
  every layer at the viewport's 1024 × 1024.
- Packed channel maps, EXR and TIFF are not read; `.sbsar` is not supported by design.
- The render takes its scene from the 3D view, so the 3D view must be on screen when it starts
  or updates.
