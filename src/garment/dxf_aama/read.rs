//! Reading DXF-AAMA and DXF-ASTM: tokens, sections, blocks, then one piece per block.
use super::{
    Dialect, Issue, Line, NotchMark, NotchShape, PatternFile, Piece, Quantity, Ring, Units,
    key_value, layer,
};
use glam::DVec2;
use std::collections::BTreeMap;
use std::f64::consts::{FRAC_PI_2, TAU};

const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_POINTS: usize = 2_000_000;
const MAX_PIECES: usize = 1024;
const NO_PIECES: &str = "No pattern pieces: an AAMA/ASTM file has a block per piece with its \
     cut line on layer 1. Use Import › DXF patterns for plain drawings.";

#[derive(Clone, Copy, Debug)]
pub struct ReadOptions {
    /// Metres per drawing unit, overriding the file's units.
    pub metres_per_unit: Option<f64>,
    /// Largest distance between an arc or bulge and the points it is sampled to, metres.
    pub chord_error_m: f64,
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            metres_per_unit: None,
            chord_error_m: 0.00005,
        }
    }
}

#[derive(Clone, Copy)]
struct Pair<'a> {
    code: i32,
    value: &'a str,
}

/// One entity: its type, its own groups, and for POLYLINE its VERTEX groups.
struct Entity<'a> {
    kind: &'a str,
    fields: &'a [Pair<'a>],
    vertices: Vec<&'a [Pair<'a>]>,
}

impl Entity<'_> {
    fn layer(&self) -> &str {
        first(self.fields, 8).unwrap_or("0")
    }
}

fn first<'a>(pairs: &[Pair<'a>], code: i32) -> Option<&'a str> {
    pairs.iter().find(|p| p.code == code).map(|p| p.value)
}
fn number(pairs: &[Pair<'_>], code: i32, default: Option<f64>) -> Result<f64, String> {
    let value = match first(pairs, code) {
        Some(v) => v
            .parse::<f64>()
            .map_err(|_| format!("Invalid number in group {code}"))?,
        None => default.ok_or_else(|| format!("Missing group {code}"))?,
    };
    if !value.is_finite() {
        return Err(format!("Nonfinite group {code}"));
    }
    Ok(value)
}
fn integer(pairs: &[Pair<'_>], code: i32, default: i32) -> Result<i32, String> {
    first(pairs, code).map_or(Ok(default), |v| {
        v.parse()
            .map_err(|_| format!("Invalid integer in group {code}"))
    })
}
fn point(pairs: &[Pair<'_>], x: i32) -> Result<DVec2, String> {
    Ok(DVec2::new(
        number(pairs, x, None)?,
        number(pairs, x + 10, None)?,
    ))
}

/// The drawing as group pairs, comments (999) left out.
fn pairs(text: &str) -> Result<Vec<Pair<'_>>, String> {
    let mut lines = text.trim_start_matches('\u{feff}').lines();
    let mut out = vec![];
    while let Some(code) = lines.next() {
        if code.trim().is_empty() && out.is_empty() {
            continue;
        }
        let value = lines.next().ok_or("Incomplete DXF group pair")?;
        let code = code
            .trim()
            .parse::<i32>()
            .map_err(|_| "Invalid DXF group code; is this a binary DXF?")?;
        if code != 999 {
            out.push(Pair {
                code,
                value: value.trim(),
            });
        }
    }
    Ok(out)
}

/// The named sections, each the pairs between its name and ENDSEC.
fn sections<'a>(pairs: &'a [Pair<'a>]) -> Result<BTreeMap<&'a str, &'a [Pair<'a>]>, String> {
    let mut out = BTreeMap::new();
    let mut i = 0;
    while i < pairs.len() {
        if pairs[i].code == 0 && pairs[i].value == "EOF" {
            return Ok(out);
        }
        if pairs[i].code != 0 || pairs[i].value != "SECTION" {
            return Err("Expected SECTION or EOF".into());
        }
        let name = pairs
            .get(i + 1)
            .filter(|p| p.code == 2)
            .ok_or("Missing section name")?
            .value;
        let start = i + 2;
        let mut end = start;
        while end < pairs.len() && !(pairs[end].code == 0 && pairs[end].value == "ENDSEC") {
            end += 1;
        }
        if end == pairs.len() {
            return Err("Unclosed DXF section".into());
        }
        if out.insert(name, &pairs[start..end]).is_some() {
            return Err(format!("Duplicate {name} section"));
        }
        i = end + 1;
    }
    Err("Missing EOF".into())
}

/// Group a run of pairs into entities; a POLYLINE takes its VERTEX records up to SEQEND.
fn entities<'a>(pairs: &'a [Pair<'a>]) -> Result<Vec<Entity<'a>>, String> {
    let mut out = vec![];
    let mut i = 0;
    let record = |i: &mut usize| {
        let start = *i + 1;
        *i = start;
        while *i < pairs.len() && pairs[*i].code != 0 {
            *i += 1;
        }
        &pairs[start..*i]
    };
    while i < pairs.len() {
        if pairs[i].code != 0 {
            return Err("Entity must start with group 0".into());
        }
        let kind = pairs[i].value;
        let fields = record(&mut i);
        let mut vertices = vec![];
        if kind == "POLYLINE" {
            while i < pairs.len() && pairs[i].value == "VERTEX" {
                vertices.push(record(&mut i));
            }
            if i >= pairs.len() || pairs[i].value != "SEQEND" {
                return Err("POLYLINE missing SEQEND".into());
            }
            record(&mut i);
        }
        out.push(Entity {
            kind,
            fields,
            vertices,
        });
    }
    Ok(out)
}

/// `$NAME` header variables with their groups.
fn header<'a>(pairs: &'a [Pair<'a>]) -> BTreeMap<&'a str, &'a [Pair<'a>]> {
    let mut out = BTreeMap::new();
    let mut i = 0;
    while i < pairs.len() {
        if pairs[i].code == 9 {
            let start = i + 1;
            let mut end = start;
            while end < pairs.len() && pairs[end].code != 9 {
                end += 1;
            }
            out.insert(pairs[i].value, &pairs[start..end]);
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

/// TEXT and MTEXT contents: `\U+XXXX` escapes decoded, MTEXT formatting removed.
fn text_of(entity: &Entity<'_>) -> Option<String> {
    let mut raw = String::new();
    if entity.kind == "MTEXT" {
        for p in entity.fields.iter().filter(|p| p.code == 3) {
            raw.push_str(p.value);
        }
    }
    raw.push_str(first(entity.fields, 1)?);
    let raw = unescape(&raw);
    let text = if entity.kind == "MTEXT" {
        mtext_plain(&raw)
    } else {
        raw.replace("%%d", "°")
            .replace("%%p", "±")
            .replace("%%c", "Ø")
    };
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!text.is_empty()).then_some(text)
}

/// `\U+00F1` → `ñ`: how DXF writes characters outside its code page.
pub(crate) fn unescape(raw: &str) -> String {
    let mut out = String::new();
    let mut rest = raw;
    while let Some(at) = rest.find("\\U+") {
        out.push_str(&rest[..at]);
        let hex = rest.get(at + 3..at + 7);
        match hex
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .and_then(char::from_u32)
        {
            Some(c) => {
                out.push(c);
                rest = &rest[at + 7..];
            }
            None => {
                out.push_str("\\U+");
                rest = &rest[at + 3..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn mtext_plain(raw: &str) -> String {
    let mut out = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' | '}' => {}
            '\\' => match chars.next() {
                Some('P') | Some('~') => out.push(' '),
                Some(c @ ('\\' | '{' | '}')) => out.push(c),
                Some('A' | 'C' | 'F' | 'f' | 'H' | 'Q' | 'T' | 'W' | 'p') => {
                    for c in chars.by_ref() {
                        if c == ';' {
                            break;
                        }
                    }
                }
                _ => {}
            },
            _ => out.push(c),
        }
    }
    out
}

/// Geometry an entity contributes, in drawing units.
enum Shape {
    Point(DVec2),
    Path { points: Vec<DVec2>, closed: bool },
    Text(String),
}

/// Points along an arc from `start` (radians) sweeping `sweep`, the start left out.
fn arc(
    center: DVec2,
    radius: f64,
    start: f64,
    sweep: f64,
    error: f64,
) -> Result<Vec<DVec2>, String> {
    if !radius.is_finite() || radius <= 0. || !center.is_finite() {
        return Err("Invalid arc radius or centre".into());
    }
    let step = (4. * (error / (2. * radius)).min(1.).sqrt().asin()).min(FRAC_PI_2);
    let count = (sweep.abs() / step).ceil().max(1.);
    if !count.is_finite() || count > 100_000. {
        return Err("Arc sampling budget exceeded".into());
    }
    let count = count as usize;
    Ok((1..=count)
        .map(|k| {
            let a = start + sweep * k as f64 / count as f64;
            center + radius * DVec2::new(a.cos(), a.sin())
        })
        .collect())
}

/// Vertices with their bulges (the arc to the next vertex) as a sampled path.
fn bulged(vertices: &[(DVec2, f64)], closed: bool, error: f64) -> Result<Vec<DVec2>, String> {
    let mut out = vec![vertices[0].0];
    let segments = vertices.len() - usize::from(!closed);
    for i in 0..segments {
        let (a, bulge) = vertices[i];
        let b = vertices[(i + 1) % vertices.len()].0;
        let chord = b - a;
        let length = chord.length();
        if bulge == 0. || length == 0. || bulge.abs() * length * 0.5 <= error {
            out.push(b);
            continue;
        }
        let normal = DVec2::new(-chord.y, chord.x) / length;
        let center = a + chord * 0.5 + normal * (length * 0.25 * (1. / bulge - bulge));
        let radius = length * 0.25 * (bulge.abs() + 1. / bulge.abs());
        let start = (a - center).y.atan2((a - center).x);
        let mut points = arc(center, radius, start, 4. * bulge.atan(), error)?;
        *points.last_mut().unwrap() = b;
        out.extend(points);
    }
    if closed {
        out.pop();
    }
    Ok(out)
}

/// Only drawings in the XY plane are read; an extrusion of -Z mirrors X (the arbitrary-axis
/// rule for that one direction), any other is refused.
fn mirror_x(pairs: &[Pair<'_>]) -> Result<bool, String> {
    let (x, y, z) = (
        number(pairs, 210, Some(0.))?,
        number(pairs, 220, Some(0.))?,
        number(pairs, 230, Some(1.))?,
    );
    if x == 0. && y == 0. && z == 1. {
        Ok(false)
    } else if x == 0. && y == 0. && z == -1. {
        Ok(true)
    } else {
        Err("Extrusion outside the drawing plane is unsupported".into())
    }
}

fn shape(entity: &Entity<'_>, error: f64) -> Result<Shape, String> {
    let f = entity.fields;
    let flip = mirror_x(f)?;
    let fix = |p: DVec2| if flip { DVec2::new(-p.x, p.y) } else { p };
    let shape = match entity.kind {
        "POINT" => Shape::Point(fix(point(f, 10)?)),
        "LINE" => Shape::Path {
            points: vec![fix(point(f, 10)?), fix(point(f, 11)?)],
            closed: false,
        },
        "ARC" | "CIRCLE" => {
            let center = point(f, 10)?;
            let radius = number(f, 40, None)?;
            let (start, sweep) = if entity.kind == "ARC" {
                let start = number(f, 50, None)?.to_radians();
                let end = number(f, 51, None)?.to_radians();
                (start, (end - start).rem_euclid(TAU))
            } else {
                (0., TAU)
            };
            if sweep == 0. {
                return Err("Arc with no sweep".into());
            }
            let mut points = vec![center + radius * DVec2::new(start.cos(), start.sin())];
            points.extend(arc(center, radius, start, sweep, error)?);
            let closed = entity.kind == "CIRCLE";
            if closed {
                points.pop();
            }
            Shape::Path {
                points: points.into_iter().map(fix).collect(),
                closed,
            }
        }
        "LWPOLYLINE" | "POLYLINE" => {
            let flags = integer(f, 70, 0)?;
            if flags & !(1 | 128) != 0 {
                return Err("Curve-fitted, spline, 3D or mesh polylines are unsupported".into());
            }
            let mut vertices = vec![];
            if entity.kind == "LWPOLYLINE" {
                for (i, p) in f.iter().enumerate() {
                    if p.code == 10 {
                        let x = p.value.parse::<f64>().map_err(|_| "Invalid vertex")?;
                        let y = f
                            .get(i + 1)
                            .filter(|q| q.code == 20)
                            .ok_or("Vertex without Y")?
                            .value
                            .parse::<f64>()
                            .map_err(|_| "Invalid vertex")?;
                        vertices.push((DVec2::new(x, y), 0.));
                    } else if p.code == 42 {
                        let bulge = p.value.parse::<f64>().map_err(|_| "Invalid bulge")?;
                        vertices.last_mut().ok_or("Bulge before vertex")?.1 = bulge;
                    }
                }
            } else {
                for v in &entity.vertices {
                    if integer(v, 70, 0)? & !(32) != 0 {
                        return Err("Fitted or 3D polyline vertices are unsupported".into());
                    }
                    vertices.push((point(v, 10)?, number(v, 42, Some(0.))?));
                }
            }
            if vertices.len() < 2 {
                return Err("Polyline needs at least two vertices".into());
            }
            if vertices
                .iter()
                .any(|(p, b)| !p.is_finite() || !b.is_finite())
            {
                return Err("Nonfinite polyline vertex".into());
            }
            let mut closed = flags & 1 != 0;
            let mut points = bulged(&vertices, closed, error)?;
            // A path drawn back to its start is closed, whatever its flag says.
            if !closed && points.len() > 3 && points[0].distance(*points.last().unwrap()) <= error {
                points.pop();
                closed = true;
            }
            Shape::Path {
                points: points.into_iter().map(fix).collect(),
                closed,
            }
        }
        "TEXT" | "MTEXT" => Shape::Text(text_of(entity).ok_or("Text without characters")?),
        other => {
            return Err(format!(
                "{other} entities are not part of the apparel layers"
            ));
        }
    };
    Ok(shape)
}

/// Collects issues, counting repeats.
#[derive(Default)]
struct Issues(Vec<Issue>);

impl Issues {
    fn add(&mut self, piece: &str, layer: &str, entity: &str, reason: impl Into<String>) {
        let reason = reason.into();
        if let Some(i) = self.0.iter_mut().find(|i| {
            i.piece == piece && i.layer == layer && i.entity == entity && i.reason == reason
        }) {
            i.count += 1;
        } else {
            self.0.push(Issue {
                piece: piece.into(),
                layer: layer.into(),
                entity: entity.into(),
                reason,
                count: 1,
            });
        }
    }
}

/// A notch as drawn: a point on the cut line with its depth, opening and, when given, the
/// point its depth reaches; or a line from the cut line inward.
struct RawNotch {
    base: DVec2,
    /// The far end, when the file gives a direction (an angle, or a line's other end).
    tip: Option<DVec2>,
    /// Drawing units; zero when the file gives none.
    depth: f64,
    width: f64,
    layer: u32,
}

/// A piece's geometry before scaling, sorted by layer.
#[derive(Default)]
struct Raw {
    name: String,
    boundary: Vec<(Vec<DVec2>, bool)>,
    sew: Vec<(Vec<DVec2>, bool)>,
    cutouts: Vec<(Vec<DVec2>, bool)>,
    internal: Vec<(Vec<DVec2>, bool)>,
    notches: Vec<RawNotch>,
    grain: Vec<Vec<DVec2>>,
    turns: Vec<DVec2>,
    drill: Vec<DVec2>,
    /// Grade rule numbers with the point they sit on.
    grades: Vec<super::GradePoint>,
    grade_reference: Option<[DVec2; 2]>,
    /// Texts with their layer number, if numbered.
    texts: Vec<(Option<u32>, String)>,
    points: usize,
}

impl Raw {
    fn transform(&mut self, f: impl Fn(DVec2) -> DVec2) {
        let paths = self
            .boundary
            .iter_mut()
            .chain(self.sew.iter_mut())
            .chain(self.cutouts.iter_mut())
            .chain(self.internal.iter_mut())
            .map(|(p, _)| p)
            .chain(self.grain.iter_mut());
        for path in paths {
            for p in path {
                *p = f(*p);
            }
        }
        for p in self.turns.iter_mut().chain(self.drill.iter_mut()) {
            *p = f(*p);
        }
        for g in &mut self.grades {
            g.at = f(g.at);
        }
        if let Some(line) = &mut self.grade_reference {
            for p in line {
                *p = f(*p);
            }
        }
        for notch in &mut self.notches {
            notch.base = f(notch.base);
            notch.tip = notch.tip.map(&f);
        }
    }
}

fn layer_number(name: &str) -> Option<u32> {
    let name = name.trim();
    (!name.is_empty() && name.len() <= 3 && name.bytes().all(|b| b.is_ascii_digit()))
        .then(|| name.parse().ok())
        .flatten()
}

/// Sort a block's entities into a raw piece, listing what does not fit.
fn collect(
    raw: &mut Raw,
    list: &[Entity<'_>],
    error: f64,
    issues: &mut Issues,
) -> Result<(), String> {
    for entity in list {
        let layer_name = entity.layer().to_owned();
        let piece = raw.name.clone();
        let layer_no = layer_number(&layer_name);
        // A notch point: Z is its depth, thickness its opening, angle its direction.
        if entity.kind == "POINT"
            && let Some(n @ (layer::NOTCHES | 80..=83)) = layer_no
        {
            let f = entity.fields;
            let read = || -> Result<RawNotch, String> {
                let flip = mirror_x(f)?;
                let fix = |p: DVec2| if flip { DVec2::new(-p.x, p.y) } else { p };
                let base = fix(point(f, 10)?);
                let depth = number(f, 30, Some(0.))?.abs();
                let width = number(f, 39, Some(0.))?.abs();
                let tip = match first(f, 50) {
                    Some(_) if depth > 0. => {
                        let a = number(f, 50, None)?.to_radians();
                        Some(fix(base + DVec2::new(a.cos(), a.sin()) * depth))
                    }
                    _ => None,
                };
                Ok(RawNotch {
                    base,
                    tip,
                    depth,
                    width,
                    layer: n,
                })
            };
            match read() {
                Ok(notch) => raw.notches.push(notch),
                Err(reason) => issues.add(&piece, &layer_name, entity.kind, reason),
            }
            continue;
        }
        // A grade rule number: `# n` on the point it grades (layer 5, or 4 for a notch; some
        // systems put it on the point's own layer).
        if matches!(entity.kind, "TEXT" | "MTEXT")
            && let Some(n @ (1..=5 | 13 | 80..=83)) = layer_no
            && let Some(rule) = text_of(entity)
                .as_deref()
                .and_then(super::GradePoint::parse)
        {
            let at = mirror_x(entity.fields).and_then(|flip| {
                let p = point(entity.fields, 10)?;
                Ok(if flip { DVec2::new(-p.x, p.y) } else { p })
            });
            match at {
                Ok(at) => raw.grades.push(super::GradePoint {
                    at,
                    rule,
                    notch: matches!(n, layer::NOTCHES | 80..=83),
                }),
                Err(reason) => issues.add(&piece, &layer_name, entity.kind, reason),
            }
            continue;
        }
        let shape = match shape(entity, error) {
            Ok(s) => s,
            Err(reason) => {
                issues.add(&piece, &layer_name, entity.kind, reason);
                continue;
            }
        };
        if let Shape::Path { points, .. } = &shape {
            raw.points += points.len();
            if raw.points > MAX_POINTS {
                return Err("Drawing point budget exceeded".into());
            }
        }
        if let Shape::Text(text) = shape {
            raw.texts.push((layer_no, text));
            continue;
        }
        let mut skip = |reason: &str| issues.add(&piece, &layer_name, entity.kind, reason);
        match (layer_no, shape) {
            (Some(layer::BOUNDARY), Shape::Path { points, closed }) => {
                raw.boundary.push((points, closed))
            }
            (Some(layer::SEW), Shape::Path { points, closed }) => raw.sew.push((points, closed)),
            (Some(layer::CUTOUTS), Shape::Path { points, closed }) => {
                raw.cutouts.push((points, closed))
            }
            (Some(layer::INTERNAL), Shape::Path { points, closed }) => {
                raw.internal.push((points, closed))
            }
            (Some(layer::GRAIN), Shape::Path { points, .. }) => raw.grain.push(points),
            (Some(layer::TURN_POINTS), Shape::Point(p)) => raw.turns.push(p),
            // Curve points only say a boundary point is not a corner, which is the default.
            (Some(layer::CURVE_POINTS), Shape::Point(_)) => {}
            (Some(layer::DRILL_HOLES), Shape::Point(p)) => raw.drill.push(p),
            (
                Some(layer::DRILL_HOLES),
                Shape::Path {
                    points,
                    closed: true,
                },
            ) => {
                let centre = points.iter().copied().sum::<DVec2>() / points.len() as f64;
                raw.drill.push(centre);
            }
            // Some systems draw a notch as a line from the cut line inward.
            (
                Some(n @ (layer::NOTCHES | 80..=83)),
                Shape::Path {
                    points,
                    closed: false,
                },
            ) => {
                let (a, b) = (points[0], *points.last().unwrap());
                raw.notches.push(RawNotch {
                    base: a,
                    tip: Some(b),
                    depth: a.distance(b),
                    width: 0.,
                    layer: n,
                });
            }
            (Some(layer::MIRROR), _) => {
                skip("Mirror (fold) lines are not imported; the piece comes in as drawn")
            }
            (
                Some(layer::GRADE_REFERENCE),
                Shape::Path {
                    points,
                    closed: false,
                },
            ) if raw.grade_reference.is_none()
                && points[0].distance(*points.last().unwrap()) > 0. =>
            {
                raw.grade_reference = Some([points[0], *points.last().unwrap()]);
            }
            (Some(layer::GRADE_REFERENCE), _) => {
                skip("Alternate grade reference lines and other layer-5 entities are not imported")
            }
            (Some(layer::STRIPE | layer::PLAID), _) => {
                skip("Stripe and plaid matching lines are not imported")
            }
            (Some(84..=87), _) => skip("Quality validation curves are not imported"),
            (Some(n), Shape::Point(_)) => skip(&format!("Points on layer {n} are not imported")),
            (Some(n), _) => skip(&format!("This entity on layer {n} is not imported")),
            (None, _) => skip("Not on a numbered AAMA/ASTM layer"),
        }
    }
    Ok(())
}

/// Join open paths whose ends meet into closed rings; returns the rings and the rest.
fn chain(paths: Vec<(Vec<DVec2>, bool)>, tolerance: f64) -> (Vec<Vec<DVec2>>, Vec<Vec<DVec2>>) {
    let mut rings = vec![];
    let mut open = vec![];
    for (points, closed) in paths {
        if closed {
            rings.push(points);
        } else {
            open.push(points);
        }
    }
    let mut left = vec![];
    while let Some(mut current) = open.pop() {
        loop {
            let end = *current.last().unwrap();
            if current.len() > 2 && current[0].distance(end) <= tolerance {
                current.pop();
                rings.push(std::mem::take(&mut current));
                break;
            }
            let next = open.iter().position(|p| {
                p[0].distance(end) <= tolerance || p.last().unwrap().distance(end) <= tolerance
            });
            let Some(next) = next else {
                left.push(std::mem::take(&mut current));
                break;
            };
            let mut next = open.swap_remove(next);
            if next[0].distance(end) > tolerance {
                next.reverse();
            }
            current.extend_from_slice(&next[1..]);
        }
    }
    (rings, left)
}

fn area(points: &[DVec2]) -> f64 {
    (0..points.len())
        .map(|i| points[i].perp_dot(points[(i + 1) % points.len()]))
        .sum::<f64>()
        * 0.5
}

fn inside(p: DVec2, points: &[DVec2]) -> bool {
    let mut yes = false;
    for i in 0..points.len() {
        let (a, b) = (points[i], points[(i + 1) % points.len()]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            yes = !yes;
        }
    }
    yes
}

/// Distance from `p` to a closed polyline.
pub(crate) fn ring_distance(p: DVec2, ring: &[DVec2]) -> f64 {
    (0..ring.len())
        .map(|i| segment_distance(p, ring[i], ring[(i + 1) % ring.len()]))
        .fold(f64::INFINITY, f64::min)
}

/// The point of a closed polyline nearest to `p`.
fn nearest_on(p: DVec2, ring: &[DVec2]) -> DVec2 {
    (0..ring.len())
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            let d = b - a;
            a + d * ((p - a).dot(d) / d.length_squared().max(1e-300)).clamp(0., 1.)
        })
        .min_by(|x, y| x.distance(p).total_cmp(&y.distance(p)))
        .unwrap_or(p)
}

/// The normal into the ring at its point nearest `p`.
fn inward_normal(p: DVec2, ring: &[DVec2]) -> DVec2 {
    let i =
        (0..ring.len())
            .min_by(|a, b| {
                segment_distance(p, ring[*a], ring[(*a + 1) % ring.len()])
                    .total_cmp(&segment_distance(p, ring[*b], ring[(*b + 1) % ring.len()]))
            })
            .unwrap_or(0);
    let d = (ring[(i + 1) % ring.len()] - ring[i]).normalize_or_zero();
    let left = DVec2::new(-d.y, d.x);
    if area(ring) > 0. { left } else { -left }
}

pub(crate) fn segment_distance(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let d = b - a;
    let t = ((p - a).dot(d) / d.length_squared().max(1e-300)).clamp(0., 1.);
    p.distance(a + d * t)
}

/// A ring with the turn points that lie on its points; with none on it, its corners, any
/// point the line turns more than 30 degrees at.
fn ring(points: Vec<DVec2>, turns: &[DVec2], tolerance: f64) -> Ring {
    let mut flags: Vec<bool> = points
        .iter()
        .map(|p| turns.iter().any(|t| t.distance(*p) <= tolerance))
        .collect();
    if !flags.iter().any(|f| *f) {
        let n = points.len();
        flags = (0..n)
            .map(|i| {
                let a = points[(i + n - 1) % n];
                let (b, c) = (points[i], points[(i + 1) % n]);
                let (u, v) = ((b - a).normalize_or_zero(), (c - b).normalize_or_zero());
                u.dot(v) < 30f64.to_radians().cos()
            })
            .collect();
    }
    Ring {
        points,
        turns: flags,
    }
}

/// The piece built from a raw block, in metres.
fn piece(mut raw: Raw, scale: f64, tolerance: f64, issues: &mut Issues) -> Option<Piece> {
    raw.transform(|p| p * scale);
    let name = raw.name.clone();
    let (mut rings, left) = chain(std::mem::take(&mut raw.boundary), tolerance);
    for _ in left {
        issues.add(
            &name,
            "1",
            "path",
            "Open boundary segment that closes no outline",
        );
    }
    rings.retain(|r| r.len() >= 3 && area(r).abs() > 1e-12);
    // The largest outline is the piece; others inside it are cutouts drawn on layer 1.
    rings.sort_by(|a, b| area(b).abs().total_cmp(&area(a).abs()));
    if rings.is_empty() {
        issues.add(
            &name,
            "1",
            "block",
            "Block without a closed cut line: not a piece",
        );
        return None;
    }
    let cut = rings.remove(0);
    let mut cutouts = vec![];
    for r in rings {
        if inside(r[0], &cut) {
            cutouts.push(r);
        } else {
            issues.add(
                &name,
                "1",
                "path",
                "Second outline outside the piece's cut line",
            );
        }
    }
    let (more, left) = chain(std::mem::take(&mut raw.cutouts), tolerance);
    cutouts.extend(more);
    for _ in left {
        issues.add(
            &name,
            "11",
            "path",
            "Open cutout segment that closes no outline",
        );
    }
    let (mut sews, left) = chain(std::mem::take(&mut raw.sew), tolerance);
    for _ in left {
        issues.add(
            &name,
            "14",
            "path",
            "Open sew line segment that closes no outline",
        );
    }
    sews.sort_by(|a, b| area(b).abs().total_cmp(&area(a).abs()));
    let mut sew = None;
    let mut cutout_sews = vec![];
    for r in sews {
        if sew.is_none() && inside(r[0], &cut)
            || sew.is_none() && ring_distance(r[0], &cut) < tolerance
        {
            sew = Some(r);
        } else if sew.as_ref().is_some_and(|s| inside(r[0], s)) {
            cutout_sews.push(r);
        } else {
            issues.add(&name, "14", "path", "Sew line outside the piece's cut line");
        }
    }
    let turns = std::mem::take(&mut raw.turns);
    let mut notches = vec![];
    for raw_notch in raw.notches {
        let layer_name = raw_notch.layer.to_string();
        let depth = raw_notch.depth * scale;
        let width = raw_notch.width * scale;
        // A line's base is its end on the cut line; a point sits on it (a T notch's point
        // may sit at its depth, so it is brought back to the nearest cut-line point).
        let (mut base, mut tip) = (raw_notch.base, raw_notch.tip);
        if let Some(t) = tip
            && ring_distance(t, &cut) < ring_distance(base, &cut)
        {
            (base, tip) = (t, Some(base));
        }
        let gap = ring_distance(base, &cut);
        if gap > tolerance.max(1e-4) {
            if gap <= depth.max(0.001) + 1e-4 {
                base = nearest_on(base, &cut);
            } else {
                issues.add(&name, &layer_name, "notch", "Notch away from the cut line");
                continue;
            }
        }
        let depth = if depth > 1e-6 {
            depth
        } else {
            issues.add(
                &name,
                &layer_name,
                "notch",
                "Notch without a depth; 5 mm used",
            );
            0.005
        };
        // Notches cut from the boundary inward: point it into the piece.
        let mut direction = match tip {
            Some(t) if t.distance(base) > 1e-9 => (t - base).normalize(),
            _ => inward_normal(base, &cut),
        };
        if !inside(base + direction * 1e-4, &cut) {
            direction = -direction;
        }
        let shape = match raw_notch.layer {
            layer::T_NOTCH => NotchShape::T,
            layer::CASTLE_NOTCH => NotchShape::Castle,
            layer::CHECK_NOTCH => NotchShape::Check,
            layer::U_NOTCH => NotchShape::U,
            _ if width > 1e-6 => NotchShape::V,
            _ => NotchShape::Slit,
        };
        notches.push(NotchMark {
            base,
            tip: base + direction * depth,
            width,
            shape,
        });
    }
    let mut grain = None;
    for line in raw.grain {
        let (a, b) = (line[0], *line.last().unwrap());
        if grain.is_none() && a.distance(b) > 1e-6 {
            grain = Some([a, b]);
        } else {
            issues.add(&name, "7", "grain", "Extra grain line");
        }
    }
    let mut piece = Piece {
        name: raw.name.clone(),
        quantity: Quantity::default(),
        cut: ring(cut, &turns, tolerance),
        sew: sew.map(|s| ring(s, &turns, tolerance)),
        cutouts: cutouts
            .into_iter()
            .map(|r| ring(r, &turns, tolerance))
            .collect(),
        cutout_sews: cutout_sews
            .into_iter()
            .map(|r| ring(r, &turns, tolerance))
            .collect(),
        internal: raw
            .internal
            .into_iter()
            .map(|(points, closed)| {
                let mut turns: Vec<bool> = points
                    .iter()
                    .map(|p| turns.iter().any(|t| t.distance(*p) <= tolerance))
                    .collect();
                if !turns.iter().any(|t| *t) {
                    turns[0] = true;
                    *turns.last_mut().unwrap() = true;
                }
                Line {
                    points,
                    turns,
                    closed,
                }
            })
            .collect(),
        notches,
        grain,
        drill_holes: raw.drill,
        grade_points: raw.grades,
        grade_reference: raw.grade_reference,
        ..Default::default()
    };
    for (layer, text) in raw.texts {
        if layer == Some(super::layer::ANNOTATION) {
            piece.annotations.push(text);
            continue;
        }
        // Grade rule numbers (`# 12`) and drill or mirror marks sit on the point they label.
        if let Some(n) = layer.filter(|n| matches!(n, 2..=5 | 13 | 80..=83)) {
            issues.add(
                &name,
                &n.to_string(),
                "TEXT",
                "Texts on this layer (grade rule numbers, marks) are not imported",
            );
            continue;
        }
        match key_value(&text) {
            Some((key, value)) if key == "PIECE NAME" && !value.is_empty() => {
                piece.name = value.into()
            }
            Some((key, value)) if key == "SIZE" || key == "SIZE NAME" => piece.size = value.into(),
            Some((key, value)) if key == "MATERIAL" || key == "CATEGORY" => {
                piece.material = value.into()
            }
            Some((key, value)) if key == "QUANTITY" => match Quantity::parse(value) {
                Some(q) => piece.quantity = q,
                None => piece.annotations.push(text.clone()),
            },
            _ => piece.annotations.push(text),
        }
    }
    Some(piece)
}

/// The affine map an INSERT applies to its block: scale, rotate, then move.
fn insert_map(
    fields: &[Pair<'_>],
    base: DVec2,
) -> Result<(impl Fn(DVec2) -> DVec2 + use<>, Option<String>), String> {
    let at = point(fields, 10)?;
    let sx = number(fields, 41, Some(1.))?;
    let sy = number(fields, 42, Some(1.))?;
    let angle = number(fields, 50, Some(0.))?.to_radians();
    if integer(fields, 70, 1)? > 1 || integer(fields, 71, 1)? > 1 {
        return Err("Arrays of block insertions are unsupported".into());
    }
    if sx == 0. || sy == 0. {
        return Err("Insertion with zero scale".into());
    }
    let note = ((sx - 1.).abs() > 1e-9 || (sy - 1.).abs() > 1e-9)
        .then(|| format!("Piece is inserted at scale {sx} × {sy}; imported as inserted"));
    let (sin, cos) = angle.sin_cos();
    Ok((
        move |p: DVec2| {
            let q = (p - base) * DVec2::new(sx, sy);
            at + DVec2::new(q.x * cos - q.y * sin, q.x * sin + q.y * cos)
        },
        note,
    ))
}

/// A DXF file's text: UTF-8, or else the Windows-1252 code page release-12 files use.
pub fn load(path: &std::path::Path) -> Result<String, String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("DXF exceeds the 16 MiB input limit".into());
    }
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return Err("This DXF is saved as UTF-16; save it as ASCII or UTF-8 text".into());
    }
    Ok(String::from_utf8(bytes)
        .unwrap_or_else(|e| crate::pattern_import::windows_1252(e.as_bytes())))
}

/// Whether a drawing is laid out as an apparel DXF: some block draws on layer 1.
pub fn is_apparel(text: &str) -> bool {
    let Ok(pairs) = pairs(text) else {
        return false;
    };
    let Ok(sections) = sections(&pairs) else {
        return false;
    };
    let Some(blocks) = sections.get("BLOCKS") else {
        return false;
    };
    let Ok(list) = entities(blocks) else {
        return false;
    };
    let mut name = "";
    list.iter().any(|e| {
        if e.kind == "BLOCK" {
            name = first(e.fields, 2).unwrap_or("");
        }
        !name.starts_with('*')
            && !matches!(e.kind, "BLOCK" | "ENDBLK" | "TEXT" | "MTEXT")
            && layer_number(e.layer()) == Some(layer::BOUNDARY)
    })
}

/// The dialect a file declares in its version text, if it is an apparel DXF at all.
pub fn detect(text: &str) -> Option<Dialect> {
    let upper = text.to_uppercase();
    if upper.contains("ASTM/D13") || upper.contains("D6673") || upper.contains("D 6673") {
        Some(Dialect::Astm)
    } else if upper.contains("ANSI/AAMA") || upper.contains("AAMA VERSION") {
        Some(Dialect::Aama)
    } else {
        None
    }
}

fn version_dialect(text: &str) -> Option<Dialect> {
    let upper = text.to_uppercase();
    let declared =
        upper.contains("VERSION") || upper.starts_with("ASTM") || upper.starts_with("ANSI");
    if !declared {
        return None;
    }
    if upper.contains("ASTM") || upper.contains("6673") {
        Some(Dialect::Astm)
    } else if upper.contains("AAMA") {
        Some(Dialect::Aama)
    } else {
        None
    }
}

/// Read an apparel DXF: its dialect, units and pieces, with everything not imported listed.
/// Structural failures and unknown units are errors; anything else becomes an issue.
pub fn read(text: &str, options: ReadOptions) -> Result<PatternFile, String> {
    if text.len() > MAX_BYTES || text.contains('\0') {
        return Err("Not a text DXF of at most 16 MiB".into());
    }
    if !options.chord_error_m.is_finite() || options.chord_error_m <= 0. {
        return Err("Chord error must be positive".into());
    }
    let pairs = pairs(text)?;
    let sections = sections(&pairs)?;
    let head = sections
        .get("HEADER")
        .map(|h| header(h))
        .unwrap_or_default();
    let blocks = sections.get("BLOCKS").copied().unwrap_or(&[]);
    let model = entities(sections.get("ENTITIES").copied().unwrap_or(&[]))?;
    let mut issues = Issues::default();

    // Blocks: BLOCK header, its entities, ENDBLK.
    let block_entities = entities(blocks)?;
    let mut defined: Vec<(String, DVec2, Vec<Entity<'_>>)> = vec![];
    let mut current: Option<(String, DVec2, Vec<Entity<'_>>)> = None;
    for entity in block_entities {
        match entity.kind {
            "BLOCK" => {
                if current.is_some() {
                    return Err("Nested BLOCK".into());
                }
                let name = first(entity.fields, 2).unwrap_or("").to_owned();
                let base = point(entity.fields, 10).unwrap_or(DVec2::ZERO);
                current = Some((name, base, vec![]));
            }
            "ENDBLK" => defined.push(current.take().ok_or("ENDBLK without BLOCK")?),
            _ => current
                .as_mut()
                .ok_or("Entity outside a BLOCK in the BLOCKS section")?
                .2
                .push(entity),
        }
    }
    if current.is_some() {
        return Err("BLOCK without ENDBLK".into());
    }
    // Model-space anonymous blocks of newer releases are not pieces.
    defined.retain(|(name, _, _)| !name.starts_with('*'));

    // Drawing texts: the version, the style, the units.
    let mut version = String::new();
    let mut dialect = None;
    let mut style = String::new();
    let mut sample_size = String::new();
    let mut author = String::new();
    let mut unit_word = None;
    let all_texts = model
        .iter()
        .chain(defined.iter().flat_map(|(_, _, list)| list.iter()));
    for entity in all_texts.filter(|e| matches!(e.kind, "TEXT" | "MTEXT")) {
        let Some(text) = text_of(entity) else {
            continue;
        };
        if dialect.is_none()
            && let Some(d) = version_dialect(&text)
        {
            dialect = Some(d);
            version = text.clone();
        }
        if let Some((key, value)) = key_value(&text) {
            match key.as_str() {
                "UNITS" if unit_word.is_none() => unit_word = Some(value.to_uppercase()),
                "STYLE NAME" if style.is_empty() => style = value.into(),
                "SAMPLE SIZE" if sample_size.is_empty() => sample_size = value.into(),
                "AUTHOR" if author.is_empty() => author = value.into(),
                _ => {}
            }
        }
    }
    // A pattern has a cut line on layer 1 in some block; without one, units do not matter.
    if !defined.iter().any(|(_, _, list)| {
        list.iter()
            .any(|e| layer_number(e.layer()) == Some(layer::BOUNDARY) && e.kind != "TEXT")
    }) {
        return Err(NO_PIECES.into());
    }
    let text_units = match unit_word.as_deref() {
        Some(w) if w.starts_with("METRIC") => Some(Units::Millimetres),
        Some(w) if w.starts_with("ENGLISH") => Some(Units::Inches),
        Some(w) => {
            issues.add("", "", "TEXT", format!("Unknown units word {w}"));
            None
        }
        None => None,
    };
    let insunits = head
        .get("$INSUNITS")
        .and_then(|g| first(g, 70))
        .and_then(|v| v.parse::<i32>().ok());
    let header_units = match insunits {
        Some(4) => Some(Units::Millimetres),
        Some(1) => Some(Units::Inches),
        _ => None,
    };
    if let (Some(t), Some(h)) = (text_units, header_units)
        && t != h
    {
        issues.add(
            "",
            "",
            "HEADER",
            format!(
                "Units text says {} but $INSUNITS says {}; the text is used",
                t.label(),
                h.label()
            ),
        );
    }
    let (units, metres_per_unit, units_source) =
        match (options.metres_per_unit, text_units, header_units) {
            (Some(m), _, _) => {
                if !m.is_finite() || m <= 0. {
                    return Err("Scale must be positive".into());
                }
                (None, m, "chosen scale")
            }
            (None, Some(u), _) => (Some(u), u.metres(), "Units text"),
            (None, None, Some(u)) => (Some(u), u.metres(), "$INSUNITS"),
            (None, None, None) => match insunits {
                Some(5) => (None, 0.01, "$INSUNITS"),
                Some(6) => (None, 1., "$INSUNITS"),
                _ => {
                    return Err(
                    "The file does not say its units (no Units text or $INSUNITS); choose a scale"
                        .into(),
                );
                }
            },
        };
    let tolerance = 1e-5; // metres: well above written precision, well below a notch
    let error = options.chord_error_m / metres_per_unit;

    // Insertions, by block name.
    let mut inserts: BTreeMap<&str, Vec<&Entity<'_>>> = BTreeMap::new();
    for entity in &model {
        match entity.kind {
            "INSERT" => inserts
                .entry(first(entity.fields, 2).unwrap_or(""))
                .or_default()
                .push(entity),
            "TEXT" | "MTEXT" => {}
            kind => issues.add(
                "",
                entity.layer(),
                kind,
                "Drawn outside any piece block; not imported",
            ),
        }
    }
    let mut pieces = vec![];
    let mut has_astm_layers = false;
    for (name, base, list) in &defined {
        has_astm_layers |= list
            .iter()
            .any(|e| layer_number(e.layer()).is_some_and(|n| (80..=87).contains(&n)));
        let mut raw = Raw {
            name: name.clone(),
            ..Default::default()
        };
        collect(&mut raw, list, error, &mut issues)?;
        let uses = inserts.get(name.as_str()).map(Vec::as_slice).unwrap_or(&[]);
        if let Some(insert) = uses.first() {
            match insert_map(insert.fields, *base) {
                Ok((map, note)) => {
                    raw.transform(map);
                    if let Some(note) = note {
                        issues.add(name, insert.layer(), "INSERT", note);
                    }
                }
                Err(reason) => {
                    issues.add(name, insert.layer(), "INSERT", reason);
                    continue;
                }
            }
        }
        if uses.len() > 1 {
            issues.add(
                name,
                "",
                "INSERT",
                format!("Block inserted {} times; imported once", uses.len()),
            );
        }
        if let Some(piece) = piece(raw, metres_per_unit, tolerance, &mut issues) {
            // Issues name the piece, not its block.
            for issue in &mut issues.0 {
                if issue.piece == *name {
                    issue.piece = piece.name.clone();
                }
            }
            pieces.push(piece);
            if pieces.len() > MAX_PIECES {
                return Err("More than 1024 pieces".into());
            }
        }
    }
    for (name, list) in &inserts {
        if !defined.iter().any(|(n, _, _)| n == name) {
            issues.add(
                name,
                list[0].layer(),
                "INSERT",
                "Insertion of an undefined block",
            );
        }
    }
    // Pieces drawn in the sample size need not repeat it.
    for piece in &mut pieces {
        if piece.size.is_empty() {
            piece.size = sample_size.clone();
        }
    }
    if pieces.is_empty() {
        return Err(NO_PIECES.into());
    }
    let (dialect, dialect_reason) = match dialect {
        Some(Dialect::Astm) => (Dialect::Astm, "ASTM version text"),
        Some(Dialect::Aama) => (Dialect::Aama, "AAMA version text"),
        None if has_astm_layers => (Dialect::Astm, "ASTM-only layers 80–87"),
        None => (Dialect::Aama, "no ASTM version text or ASTM-only layers"),
    };
    Ok(PatternFile {
        dialect,
        dialect_reason,
        version,
        units,
        metres_per_unit,
        units_source,
        style,
        sample_size,
        author,
        pieces,
        issues: issues.0,
    })
}
