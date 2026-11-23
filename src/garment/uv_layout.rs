//! UV layout: where each piece's island sits in texture space, for export and for the baked
//! atlas. An island is the piece's flat pattern (its material coordinates, metres) turned,
//! scaled and moved into the unit square; texture space runs u to the right and v downwards,
//! (0, 0) at the image's top-left corner, as images are read.
//!
//! The layout is presentation metadata, never physics and never the look in the editor: the
//! 3D view keeps drawing fabric textures in material space, so moving an island changes only
//! what the exported texture coordinates and the baked atlas show. Placements are keyed by
//! piece id and anchored at the centre of the piece's material bounds, so remeshing, or an
//! edit that moves the piece's material origin, leaves its island where it was.
//!
//! Pieces without a placement are put in automatically: the whole layout is packed when no
//! piece has one, otherwise the missing pieces go into the free space at the layout's texel
//! density (beside the square when they do not fit; the editor reports them).
use super::template::{CompiledGarment, GarmentTemplate};
use glam::DVec2;
use serde::{Deserialize, Serialize};

/// Where one piece's island sits: its material bounds' centre lands on `center`, the piece
/// turned by `rotation_degrees` and scaled by `scale` texture units per metre of material.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UvPlacement {
    pub center: DVec2,
    /// Turn of the island, degrees: positive turns clockwise as the square is drawn (v down),
    /// the same way as a piece's board rotation.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rotation_degrees: f64,
    /// Texture units per metre of material.
    pub scale: f64,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.
}

/// Largest scale accepted, texture units per metre (a 0.1 mm piece filling the square).
pub const MAX_SCALE: f64 = 1e4;

impl UvPlacement {
    pub fn validate(&self) -> Result<(), String> {
        if !self.center.is_finite() || self.center.abs().max_element() > 100. {
            return Err("A UV island must sit within 100 units of the square".into());
        }
        if !self.rotation_degrees.is_finite() {
            return Err("A UV island's turn must be finite".into());
        }
        if !(self.scale.is_finite() && self.scale > 0. && self.scale <= MAX_SCALE) {
            return Err(format!(
                "A UV island's scale must be above 0 and at most {MAX_SCALE} per metre"
            ));
        }
        Ok(())
    }

    /// Texture coordinates of material point `p` of a piece whose material bounds' centre is
    /// `anchor`.
    pub fn apply(&self, anchor: DVec2, p: DVec2) -> DVec2 {
        self.center + rotate(p - anchor, self.rotation_degrees) * self.scale
    }

    /// Material point of a piece (centre `anchor`) under texture point `uv`.
    pub fn inverse(&self, anchor: DVec2, uv: DVec2) -> DVec2 {
        anchor + rotate((uv - self.center) / self.scale, -self.rotation_degrees)
    }
}

/// `p` turned by `degrees` (clockwise as drawn with v down).
pub fn rotate(p: DVec2, degrees: f64) -> DVec2 {
    let (s, c) = degrees.to_radians().sin_cos();
    DVec2::new(c * p.x - s * p.y, s * p.x + c * p.y)
}

/// How to pack islands into the unit square.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PackOptions {
    /// Gap between islands and from the square's edges, texture units.
    pub margin: f64,
    /// Islands may be turned a quarter turn when that packs them tighter.
    pub rotate: bool,
    /// Keep the islands' sizes relative to each other instead of giving every piece the same
    /// texel density.
    pub keep_relative_scale: bool,
}

/// Margin of the automatic layout, texture units (20 pixels of a 2048 image).
pub const DEFAULT_MARGIN: f64 = 0.01;
/// Largest margin accepted.
pub const MAX_MARGIN: f64 = 0.2;

impl Default for PackOptions {
    fn default() -> Self {
        Self {
            margin: DEFAULT_MARGIN,
            rotate: false,
            keep_relative_scale: false,
        }
    }
}

/// A piece's island in material space: its vertices, triangles and outline (indices into
/// `points`) and the centre of its bounds, the anchor of its placement.
#[derive(Clone, Debug)]
pub struct IslandShape {
    pub id: String,
    pub points: Vec<DVec2>,
    pub triangles: Vec<[usize; 3]>,
    /// Edges used by one triangle only: the outline and the edges of holes.
    pub outline: Vec<[usize; 2]>,
    pub anchor: DVec2,
}

impl IslandShape {
    /// Bounds of the island turned by `degrees` about its anchor, at unit scale.
    pub fn turned_bounds(&self, degrees: f64) -> (DVec2, DVec2) {
        let mut lo = DVec2::splat(f64::INFINITY);
        let mut hi = DVec2::splat(f64::NEG_INFINITY);
        for [a, _] in &self.outline {
            let q = rotate(self.points[*a] - self.anchor, degrees);
            lo = lo.min(q);
            hi = hi.max(q);
        }
        if !lo.is_finite() {
            return (DVec2::ZERO, DVec2::ZERO);
        }
        (lo, hi)
    }

    /// Material area, square metres.
    pub fn area(&self) -> f64 {
        self.triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| self.points[i]);
                (b - a).perp_dot(c - a).abs() / 2.
            })
            .sum()
    }
}

/// Every compiled piece's island, in the garment's piece order.
pub fn shapes(garment: &CompiledGarment) -> Vec<IslandShape> {
    garment
        .panels
        .iter()
        .map(|panel| {
            let first = panel.vertices.start;
            let points: Vec<DVec2> = garment.mesh.uv[panel.vertices.clone()].to_vec();
            let triangles: Vec<[usize; 3]> = garment.mesh.triangles[panel.triangles.clone()]
                .iter()
                .map(|t| t.vertices.map(|v| v - first))
                .collect();
            let mut uses = std::collections::HashMap::<(usize, usize), usize>::new();
            for t in &triangles {
                for k in 0..3 {
                    let (a, b) = (t[k], t[(k + 1) % 3]);
                    *uses.entry((a.min(b), a.max(b))).or_default() += 1;
                }
            }
            let outline: Vec<[usize; 2]> = triangles
                .iter()
                .flat_map(|t| (0..3).map(move |k| [t[k], t[(k + 1) % 3]]))
                .filter(|[a, b]| uses[&((*a).min(*b), (*a).max(*b))] == 1)
                .collect();
            let (lo, hi) = points.iter().fold(
                (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
                |(lo, hi), p| (lo.min(*p), hi.max(*p)),
            );
            let anchor = if lo.is_finite() {
                (lo + hi) / 2.
            } else {
                DVec2::ZERO
            };
            IslandShape {
                id: panel.id.clone(),
                points,
                triangles,
                outline,
                anchor,
            }
        })
        .collect()
}

/// An axis-aligned rectangle of the packing.
#[derive(Clone, Copy, Debug)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

const EPS: f64 = 1e-12;

impl Rect {
    fn overlaps(&self, o: &Rect) -> bool {
        self.x < o.x + o.w - EPS
            && o.x < self.x + self.w - EPS
            && self.y < o.y + o.h - EPS
            && o.y < self.y + self.h - EPS
    }
    fn contains(&self, o: &Rect) -> bool {
        o.x >= self.x - EPS
            && o.y >= self.y - EPS
            && o.x + o.w <= self.x + self.w + EPS
            && o.y + o.h <= self.y + self.h + EPS
    }
}

/// Free space of a square as the maximal empty rectangles left by the rectangles placed in it
/// (Jukka Jylänki's MaxRects, best short side fit).
struct MaxRects {
    free: Vec<Rect>,
}

impl MaxRects {
    fn new(side: f64) -> Self {
        Self {
            free: vec![Rect {
                x: 0.,
                y: 0.,
                w: side,
                h: side,
            }],
        }
    }

    /// Place a `w` × `h` rectangle (turned a quarter when `turn` allows and that fits better):
    /// where it went and whether it was turned.
    fn insert(&mut self, w: f64, h: f64, turn: bool) -> Option<(Rect, bool)> {
        let mut best: Option<((f64, f64), Rect, bool)> = None;
        for f in &self.free {
            for (rw, rh, turned) in [(w, h, false), (h, w, true)] {
                if turned && (!turn || (w - h).abs() < EPS) {
                    continue;
                }
                if rw > f.w + EPS || rh > f.h + EPS {
                    continue;
                }
                let (dw, dh) = (f.w - rw, f.h - rh);
                let score = (dw.min(dh), dw.max(dh));
                if best.as_ref().is_none_or(|(s, _, _)| score < *s) {
                    let rect = Rect {
                        x: f.x,
                        y: f.y,
                        w: rw,
                        h: rh,
                    };
                    best = Some((score, rect, turned));
                }
            }
        }
        let (_, rect, turned) = best?;
        self.occupy(rect);
        Some((rect, turned))
    }

    /// Take `r` out of the free space.
    fn occupy(&mut self, r: Rect) {
        let mut next = Vec::with_capacity(self.free.len() + 4);
        for f in &self.free {
            if !f.overlaps(&r) {
                next.push(*f);
                continue;
            }
            if r.x > f.x + EPS {
                next.push(Rect { w: r.x - f.x, ..*f });
            }
            if r.x + r.w < f.x + f.w - EPS {
                next.push(Rect {
                    x: r.x + r.w,
                    w: f.x + f.w - (r.x + r.w),
                    ..*f
                });
            }
            if r.y > f.y + EPS {
                next.push(Rect { h: r.y - f.y, ..*f });
            }
            if r.y + r.h < f.y + f.h - EPS {
                next.push(Rect {
                    y: r.y + r.h,
                    h: f.y + f.h - (r.y + r.h),
                    ..*f
                });
            }
        }
        next.retain(|f| f.w > EPS && f.h > EPS);
        // Keep only maximal rectangles.
        let mut keep = vec![true; next.len()];
        for i in 0..next.len() {
            if !keep[i] {
                continue;
            }
            for j in 0..next.len() {
                if i != j && keep[j] && next[j].contains(&next[i]) {
                    // Of two equal rectangles keep the first.
                    if next[i].contains(&next[j]) && i < j {
                        continue;
                    }
                    keep[i] = false;
                    break;
                }
            }
        }
        self.free = next
            .into_iter()
            .zip(keep)
            .filter_map(|(f, k)| k.then_some(f))
            .collect();
    }
}

/// Pack `sizes` (width, height) at scale `s`, each grown by `margin`, into a square of side
/// `side`, in `order`: where each went and whether it was turned.
fn try_pack(
    sizes: &[DVec2],
    order: &[usize],
    s: f64,
    margin: f64,
    side: f64,
    turn: bool,
) -> Option<Vec<(Rect, bool)>> {
    let mut space = MaxRects::new(side);
    let mut out = vec![
        (
            Rect {
                x: 0.,
                y: 0.,
                w: 0.,
                h: 0.
            },
            false
        );
        sizes.len()
    ];
    for &k in order {
        out[k] = space.insert(sizes[k].x * s + margin, sizes[k].y * s + margin, turn)?;
    }
    Some(out)
}

/// Pack the islands into the unit square without overlap, `margin` apart and from the edges,
/// as large as they fit. Each island keeps its turn from `turns` (degrees), plus a quarter
/// turn where `options.rotate` lets that pack tighter; `weights` scale islands relative to each
/// other (all 1 for one texel density). Islands are packed by their bounds.
pub fn pack(
    shapes: &[IslandShape],
    turns: &[f64],
    weights: &[f64],
    options: &PackOptions,
) -> Result<Vec<UvPlacement>, String> {
    let m = options.margin;
    if !(0.0..=MAX_MARGIN).contains(&m) {
        return Err(format!(
            "The UV margin must be between 0 and {}% of the square",
            MAX_MARGIN * 100.
        ));
    }
    if shapes.is_empty() {
        return Ok(vec![]);
    }
    if turns.len() != shapes.len() || weights.len() != shapes.len() {
        return Err("One turn and one weight per island".into());
    }
    if weights.iter().any(|w| !(w.is_finite() && *w > 0.)) || turns.iter().any(|t| !t.is_finite()) {
        return Err("Island turns and weights must be finite, weights above 0".into());
    }
    let bounds: Vec<(DVec2, DVec2)> = shapes
        .iter()
        .zip(turns)
        .map(|(s, t)| s.turned_bounds(*t))
        .collect();
    let sizes: Vec<DVec2> = bounds
        .iter()
        .zip(weights)
        .map(|((lo, hi), w)| (*hi - *lo).max(DVec2::splat(1e-9)) * *w)
        .collect();
    // The rectangles grow by the margin and fill a square inset by half of it, so islands end
    // up a margin apart and a margin from the edges.
    let side = 1. - m;
    let area: f64 = sizes.iter().map(|s| s.x * s.y).sum();
    let mut hi = (side * side / area).sqrt();
    for s in &sizes {
        let limit = if options.rotate {
            (side - m) / s.max_element()
        } else {
            ((side - m) / s.x).min((side - m) / s.y)
        };
        hi = hi.min(limit);
    }
    if !(hi.is_finite() && hi > 0.) {
        return Err("The margin leaves no room for the islands".into());
    }
    let n = sizes.len();
    let by = |key: &dyn Fn(&DVec2) -> f64| {
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by(|a, b| key(&sizes[*b]).total_cmp(&key(&sizes[*a])).then(a.cmp(b)));
        order
    };
    let orders = [
        by(&|s| s.max_element()),
        by(&|s| s.x * s.y),
        by(&|s| s.y),
        by(&|s| s.x),
    ];
    let mut best: Option<(f64, Vec<(Rect, bool)>)> = None;
    for order in &orders {
        let floor = best.as_ref().map_or(0., |(s, _)| *s);
        // Only worth searching when this order beats the best one so far.
        if let Some(rects) = try_pack(&sizes, order, floor.max(hi * 1e-6), m, side, options.rotate)
        {
            let (mut lo, mut top, mut found) = (floor.max(hi * 1e-6), hi, rects);
            if let Some(rects) = try_pack(&sizes, order, hi, m, side, options.rotate) {
                lo = hi;
                found = rects;
            } else {
                for _ in 0..30 {
                    if top - lo <= lo * 1e-5 {
                        break;
                    }
                    let mid = (lo + top) / 2.;
                    match try_pack(&sizes, order, mid, m, side, options.rotate) {
                        Some(rects) => {
                            lo = mid;
                            found = rects;
                        }
                        None => top = mid,
                    }
                }
            }
            if best.as_ref().is_none_or(|(s, _)| lo > *s) {
                best = Some((lo, found));
            }
        }
    }
    let (s, rects) = best.ok_or("Too many pieces for this margin: lower it")?;
    Ok(shapes
        .iter()
        .enumerate()
        .map(|(k, shape)| {
            let (rect, turned) = rects[k];
            let turn = turns[k] + if turned { 90. } else { 0. };
            let (lo, _) = shape.turned_bounds(turn);
            let scale = s * weights[k];
            // The rectangle sits in the square inset by half the margin, and the island inside
            // it by another half.
            let corner = DVec2::new(rect.x, rect.y) + DVec2::splat(m);
            UvPlacement {
                center: corner - lo * scale,
                rotation_degrees: normalize_degrees(turn),
                scale,
            }
        })
        .collect())
}

/// `degrees` within (-180, 180].
pub fn normalize_degrees(degrees: f64) -> f64 {
    let d = (degrees + 180.).rem_euclid(360.) - 180.;
    if d <= -180. { d + 360. } else { d }
}

/// Bounds of an island placed by `placement`, texture units.
pub fn placed_bounds(shape: &IslandShape, placement: &UvPlacement) -> (DVec2, DVec2) {
    let (lo, hi) = shape.turned_bounds(placement.rotation_degrees);
    (
        placement.center + lo * placement.scale,
        placement.center + hi * placement.scale,
    )
}

/// The layout every piece of `garment` gets from `template`: its own placement, or one found
/// automatically (see the module documentation). In the garment's piece order.
pub fn resolve(template: &GarmentTemplate, garment: &CompiledGarment) -> Vec<UvPlacement> {
    let shapes = shapes(garment);
    resolve_shapes(template, &shapes)
}

/// [`resolve`] for islands already built.
pub fn resolve_shapes(template: &GarmentTemplate, shapes: &[IslandShape]) -> Vec<UvPlacement> {
    let board_turn = |id: &str| {
        template
            .panels
            .iter()
            .find(|p| p.id == id)
            .map_or(0., |p| p.layout_rotation_degrees)
    };
    let placed: Vec<Option<UvPlacement>> = shapes
        .iter()
        .map(|s| {
            template
                .uv_layout
                .get(&s.id)
                .copied()
                .filter(|p| p.validate().is_ok())
        })
        .collect();
    if placed.iter().all(Option::is_some) {
        return placed.into_iter().flatten().collect();
    }
    if placed.iter().all(Option::is_none) {
        let turns: Vec<f64> = shapes.iter().map(|s| board_turn(&s.id)).collect();
        let weights = vec![1.; shapes.len()];
        return pack(shapes, &turns, &weights, &PackOptions::default())
            .or_else(|_| {
                let options = PackOptions {
                    margin: 0.,
                    ..Default::default()
                };
                pack(shapes, &turns, &weights, &options)
            })
            .unwrap_or_else(|_| {
                vec![
                    UvPlacement {
                        center: DVec2::splat(0.5),
                        rotation_degrees: 0.,
                        scale: 1.,
                    };
                    shapes.len()
                ]
            });
    }
    // Some pieces are placed: the others go into the free space at the placed pieces' median
    // texel density.
    let m = DEFAULT_MARGIN;
    let side = 1. - m;
    let mut scales: Vec<f64> = placed.iter().flatten().map(|p| p.scale).collect();
    scales.sort_by(f64::total_cmp);
    let scale = scales[scales.len() / 2];
    let mut space = MaxRects::new(side);
    for (shape, placement) in shapes.iter().zip(&placed) {
        if let Some(p) = placement {
            let (lo, hi) = placed_bounds(shape, p);
            // In the inset square, grown by half the margin all round.
            let x = lo.x - m;
            let y = lo.y - m;
            space.occupy(Rect {
                x,
                y,
                w: hi.x - lo.x + m,
                h: hi.y - lo.y + m,
            });
        }
    }
    // Biggest first; what does not fit goes in a column beside the square.
    let mut order: Vec<usize> = (0..shapes.len()).filter(|k| placed[*k].is_none()).collect();
    let size = |k: usize| {
        let (lo, hi) = shapes[k].turned_bounds(board_turn(&shapes[k].id));
        (hi - lo) * scale
    };
    order.sort_by(|a, b| size(*b).max_element().total_cmp(&size(*a).max_element()));
    let mut out: Vec<UvPlacement> = placed
        .iter()
        .map(|p| {
            p.unwrap_or(UvPlacement {
                center: DVec2::ZERO,
                rotation_degrees: 0.,
                scale,
            })
        })
        .collect();
    let mut beside = m;
    for k in order {
        let turn = board_turn(&shapes[k].id);
        let (lo, hi) = shapes[k].turned_bounds(turn);
        let dims = (hi - lo) * scale;
        let corner = match space.insert(dims.x + m, dims.y + m, false) {
            Some((rect, _)) => DVec2::new(rect.x, rect.y) + DVec2::splat(m),
            None => {
                let corner = DVec2::new(1. + m, beside);
                beside += dims.y + m;
                corner
            }
        };
        out[k] = UvPlacement {
            center: corner - lo * scale,
            rotation_degrees: normalize_degrees(turn),
            scale,
        };
    }
    out
}

/// The whole garment packed again: every piece starts from the turn its island has now (and,
/// when asked, keeps its size relative to the others), and the result is a placement for every
/// piece, by id.
pub fn pack_layout(
    template: &GarmentTemplate,
    garment: &CompiledGarment,
    options: &PackOptions,
) -> Result<std::collections::BTreeMap<String, UvPlacement>, String> {
    let shapes = shapes(garment);
    if shapes.is_empty() {
        return Err("There are no pieces to lay out".into());
    }
    let now = resolve_shapes(template, &shapes);
    let turns: Vec<f64> = now.iter().map(|p| p.rotation_degrees).collect();
    let largest = now.iter().map(|p| p.scale).fold(0., f64::max);
    let weights: Vec<f64> = now
        .iter()
        .map(|p| {
            if options.keep_relative_scale && largest > 0. {
                p.scale / largest
            } else {
                1.
            }
        })
        .collect();
    let placed = pack(&shapes, &turns, &weights, options)?;
    Ok(shapes
        .into_iter()
        .zip(placed)
        .map(|(s, p)| (s.id, p))
        .collect())
}

/// Every garment vertex's texture coordinates in the layout (v down, (0, 0) at the top-left).
pub fn coordinates(template: &GarmentTemplate, garment: &CompiledGarment) -> Vec<DVec2> {
    let shapes = shapes(garment);
    let placements = resolve_shapes(template, &shapes);
    let mut uv = vec![DVec2::ZERO; garment.mesh.uv.len()];
    for ((panel, shape), placement) in garment.panels.iter().zip(&shapes).zip(&placements) {
        for v in panel.vertices.clone() {
            uv[v] = placement.apply(shape.anchor, garment.mesh.uv[v]);
        }
    }
    uv
}

/// An island laid out: its triangles and outline in texture space.
pub struct PlacedIsland<'a> {
    pub shape: &'a IslandShape,
    pub uv: Vec<DVec2>,
    pub lo: DVec2,
    pub hi: DVec2,
}

impl<'a> PlacedIsland<'a> {
    pub fn new(shape: &'a IslandShape, placement: &UvPlacement) -> Self {
        let uv: Vec<DVec2> = shape
            .points
            .iter()
            .map(|p| placement.apply(shape.anchor, *p))
            .collect();
        let (lo, hi) = uv.iter().fold(
            (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
            |(lo, hi), p| (lo.min(*p), hi.max(*p)),
        );
        Self { shape, uv, lo, hi }
    }

    /// Whether texture point `q` is on the island.
    pub fn contains(&self, q: DVec2) -> bool {
        if q.cmplt(self.lo).any() || q.cmpgt(self.hi).any() {
            return false;
        }
        self.shape
            .triangles
            .iter()
            .any(|t| in_triangle(q, t.map(|i| self.uv[i])))
    }

    /// Whether the island lies inside the unit square.
    pub fn inside_square(&self) -> bool {
        self.lo.cmpge(DVec2::splat(-1e-9)).all() && self.hi.cmple(DVec2::splat(1. + 1e-9)).all()
    }
}

fn in_triangle(q: DVec2, [a, b, c]: [DVec2; 3]) -> bool {
    let area = (b - a).perp_dot(c - a);
    if area.abs() < 1e-30 {
        return false;
    }
    let w0 = (b - q).perp_dot(c - q) / area;
    let w1 = (c - q).perp_dot(a - q) / area;
    let w2 = 1. - w0 - w1;
    w0 >= -1e-12 && w1 >= -1e-12 && w2 >= -1e-12
}

/// Whether segments `ab` and `cd` cross (touching ends do not count).
fn segments_cross(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> bool {
    let d1 = (b - a).perp_dot(c - a);
    let d2 = (b - a).perp_dot(d - a);
    let d3 = (d - c).perp_dot(a - c);
    let d4 = (d - c).perp_dot(b - c);
    let scale = (b - a).length() * (d - c).length();
    let eps = 1e-12 * scale.max(1e-30);
    ((d1 > eps && d2 < -eps) || (d1 < -eps && d2 > eps))
        && ((d3 > eps && d4 < -eps) || (d3 < -eps && d4 > eps))
}

/// Whether two laid-out islands overlap: their outlines cross, or one lies on the other.
pub fn islands_overlap(a: &PlacedIsland, b: &PlacedIsland) -> bool {
    if a.hi.cmplt(b.lo).any() || b.hi.cmplt(a.lo).any() {
        return false;
    }
    for [p, q] in &a.shape.outline {
        let (p, q) = (a.uv[*p], a.uv[*q]);
        let (lo, hi) = (p.min(q), p.max(q));
        if hi.cmplt(b.lo).any() || lo.cmpgt(b.hi).any() {
            continue;
        }
        for [r, s] in &b.shape.outline {
            if segments_cross(p, q, b.uv[*r], b.uv[*s]) {
                return true;
            }
        }
    }
    let inside = |x: &PlacedIsland, y: &PlacedIsland| {
        x.shape
            .outline
            .first()
            .is_some_and(|[i, _]| y.contains(x.uv[*i]))
    };
    inside(a, b) || inside(b, a)
}

/// Pairs of islands (indices into `islands`) that overlap.
pub fn overlaps(islands: &[PlacedIsland]) -> Vec<(usize, usize)> {
    let mut out = vec![];
    for i in 0..islands.len() {
        for j in i + 1..islands.len() {
            if islands_overlap(&islands[i], &islands[j]) {
                out.push((i, j));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(id: &str, w: f64, h: f64) -> IslandShape {
        let points = vec![
            DVec2::ZERO,
            DVec2::new(w, 0.),
            DVec2::new(w, h),
            DVec2::new(0., h),
        ];
        IslandShape {
            id: id.into(),
            points,
            triangles: vec![[0, 1, 2], [0, 2, 3]],
            outline: vec![[0, 1], [1, 2], [2, 3], [3, 0]],
            anchor: DVec2::new(w / 2., h / 2.),
        }
    }

    #[test]
    fn placements_invert() {
        let p = UvPlacement {
            center: DVec2::new(0.3, 0.6),
            rotation_degrees: 37.,
            scale: 2.5,
        };
        let anchor = DVec2::new(0.1, -0.2);
        let m = DVec2::new(0.27, 0.11);
        assert!(p.inverse(anchor, p.apply(anchor, m)).distance(m) < 1e-12);
        assert!(p.apply(anchor, anchor).distance(p.center) < 1e-12);
    }

    #[test]
    fn packed_rectangles_keep_their_margin() {
        let shapes = vec![
            square("a", 0.5, 0.7),
            square("b", 0.2, 0.2),
            square("c", 0.4, 0.1),
            square("d", 0.3, 0.3),
        ];
        for rotate in [false, true] {
            let options = PackOptions {
                margin: 0.02,
                rotate,
                keep_relative_scale: false,
            };
            let placed = pack(&shapes, &[0.; 4], &[1.; 4], &options).unwrap();
            let boxes: Vec<_> = shapes
                .iter()
                .zip(&placed)
                .map(|(s, p)| placed_bounds(s, p))
                .collect();
            for (lo, hi) in &boxes {
                assert!(lo.min_element() >= 0.02 - 1e-9, "{lo}");
                assert!(hi.max_element() <= 0.98 + 1e-9, "{hi}");
            }
            for i in 0..4 {
                for j in i + 1..4 {
                    let (a, b) = (boxes[i], boxes[j]);
                    let gap = (b.0 - a.1).max(a.0 - b.1).max_element();
                    assert!(gap >= 0.02 - 1e-9, "{i} {j} {gap}");
                }
            }
            // One texel density.
            assert!(
                placed
                    .iter()
                    .all(|p| (p.scale - placed[0].scale).abs() < 1e-12)
            );
            // Tight enough: at least a third of the square is covered.
            let covered: f64 = boxes
                .iter()
                .map(|(lo, hi)| (hi.x - lo.x) * (hi.y - lo.y))
                .sum();
            assert!(covered > 0.33, "{covered}");
        }
    }

    #[test]
    fn overlap_is_found_and_separation_is_not_overlap() {
        let a = square("a", 1., 1.);
        let b = square("b", 1., 1.);
        let at = |x: f64, y: f64, turn: f64| UvPlacement {
            center: DVec2::new(x, y),
            rotation_degrees: turn,
            scale: 0.2,
        };
        let ia = PlacedIsland::new(&a, &at(0.3, 0.3, 0.));
        assert!(islands_overlap(
            &ia,
            &PlacedIsland::new(&b, &at(0.35, 0.32, 10.))
        ));
        // Inside entirely.
        let small = square("s", 0.1, 0.1);
        assert!(islands_overlap(
            &ia,
            &PlacedIsland::new(&small, &at(0.3, 0.3, 0.))
        ));
        assert!(!islands_overlap(
            &ia,
            &PlacedIsland::new(&b, &at(0.6, 0.3, 0.))
        ));
        // Turned 45°, its corner reaches 0.141 from the centre.
        assert!(islands_overlap(
            &ia,
            &PlacedIsland::new(&b, &at(0.53, 0.3, 45.))
        ));
        assert!(!islands_overlap(
            &ia,
            &PlacedIsland::new(&b, &at(0.55, 0.3, 45.))
        ));
    }

    #[test]
    fn too_wide_a_margin_is_refused() {
        let shapes = vec![square("a", 1., 1.)];
        let options = PackOptions {
            margin: 0.5,
            ..Default::default()
        };
        assert!(pack(&shapes, &[0.], &[1.], &options).is_err());
    }
}
