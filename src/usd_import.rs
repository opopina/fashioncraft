//! USD mesh import, for avatars and props: the text layer (`.usda`), or a USDZ package whose
//! root layer is text. Every `Mesh` prim is read in its first time sample, placed by the
//! transforms of its prim and its ancestors, and turned to metres and Y-up from the stage's
//! `metersPerUnit` (USD's own default is centimetres) and `upAxis`. Polygons are fanned into
//! triangles; a left-handed mesh has its winding turned. Binary layers (`.usdc`) are not
//! read: save the stage as `.usda`.
use crate::{avatar::SurfaceMesh, mesh_import::Imported};
use glam::{DMat4, DQuat, DVec3, EulerRot};

/// Largest number of values one array may hold.
const MAX_VALUES: usize = 30_000_000;

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Word(String),
    Text(String),
    Number(f64),
    Punct(char),
}

fn tokens(text: &str) -> Result<Vec<Token>, String> {
    let mut out = vec![];
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c == '#' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if c == '"' || c == '\'' {
            // Plain and triple-quoted strings.
            let triple = text[i..].starts_with(&format!("{c}{c}{c}"));
            let quote = if triple { 3 } else { 1 };
            let start = i + quote;
            let mut j = start;
            let close: String = std::iter::repeat_n(c, quote).collect();
            loop {
                if j >= bytes.len() {
                    return Err("USD text has an unterminated string".into());
                }
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if text[j..].starts_with(&close) {
                    break;
                }
                j += 1;
            }
            out.push(Token::Text(text[start..j].to_string()));
            i = j + quote;
        } else if c == '@' {
            // Asset paths: @path@ or @@@path@@@.
            let fence = if text[i..].starts_with("@@@") {
                "@@@"
            } else {
                "@"
            };
            let start = i + fence.len();
            let end = text[start..]
                .find(fence)
                .ok_or("USD text has an unterminated asset path")?;
            out.push(Token::Text(text[start..start + end].to_string()));
            i = start + end + fence.len();
        } else if c == '<' {
            // Paths: </World/Mesh>.
            let end = text[i..]
                .find('>')
                .ok_or("USD text has an unterminated path")?;
            out.push(Token::Text(text[i + 1..i + end].to_string()));
            i += end + 1;
        } else if c.is_ascii_digit() || c == '-' || c == '+' || c == '.' {
            let start = i;
            i += 1;
            while i < bytes.len() {
                let d = bytes[i] as char;
                if d.is_ascii_alphanumeric() || d == '.' || d == '-' || d == '+' {
                    i += 1;
                } else {
                    break;
                }
            }
            let word = &text[start..i];
            match word.parse::<f64>() {
                Ok(v) => out.push(Token::Number(v)),
                Err(_) => match word {
                    "-inf" => out.push(Token::Number(f64::NEG_INFINITY)),
                    _ => out.push(Token::Word(word.to_string())),
                },
            }
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < bytes.len() {
                let d = bytes[i] as char;
                if d.is_ascii_alphanumeric() || d == '_' || d == ':' || d == '.' {
                    i += 1;
                } else {
                    break;
                }
            }
            let word = &text[start..i];
            match word {
                "inf" => out.push(Token::Number(f64::INFINITY)),
                "nan" => out.push(Token::Number(f64::NAN)),
                _ => out.push(Token::Word(word.to_string())),
            }
        } else if "()[]{}=,;".contains(c) {
            out.push(Token::Punct(c));
            i += 1;
        } else {
            // Anything else (a stray character of a construct this reader skips) stands alone.
            out.push(Token::Punct(c));
            i += 1;
        }
    }
    Ok(out)
}

/// A parsed value: a number, a word or string, or a list of values (arrays and tuples alike),
/// or a dictionary (time samples) of keys and values.
#[derive(Clone, Debug)]
enum Value {
    Number(f64),
    Text(String),
    List(Vec<Value>),
    Samples(Vec<(f64, Value)>),
    None,
}

impl Value {
    fn numbers(&self, out: &mut Vec<f64>) -> Result<(), String> {
        match self {
            Value::Number(v) => out.push(*v),
            Value::List(items) => {
                for item in items {
                    item.numbers(out)?;
                    if out.len() > MAX_VALUES {
                        return Err("A USD array is too large".into());
                    }
                }
            }
            _ => return Err("Expected numbers in USD".into()),
        }
        Ok(())
    }
    fn flat(&self) -> Result<Vec<f64>, String> {
        let mut out = vec![];
        self.numbers(&mut out)?;
        Ok(out)
    }
    fn texts(&self) -> Vec<String> {
        match self {
            Value::Text(t) => vec![t.clone()],
            Value::List(items) => items.iter().flat_map(Value::texts).collect(),
            _ => vec![],
        }
    }
    /// The first time sample of a sampled value, or the value itself.
    fn first(&self) -> &Value {
        match self {
            Value::Samples(samples) => samples.first().map_or(&Value::None, |(_, v)| v),
            v => v,
        }
    }
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
}

#[derive(Default)]
struct Prim {
    kind: String,
    /// Attributes by name (`points`, `xformOp:translate`, `points.timeSamples` folded in).
    attributes: Vec<(String, Value)>,
    children: Vec<Prim>,
}

impl Prim {
    fn get(&self, name: &str) -> Option<&Value> {
        self.attributes
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.first())
    }
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }
    fn next(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.at).cloned();
        self.at += 1;
        t
    }
    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(&Token::Punct(c)) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    /// Skip a balanced group opened by the token just taken.
    fn skip_group(&mut self, open: char) -> Result<(), String> {
        let close = match open {
            '(' => ')',
            '[' => ']',
            _ => '}',
        };
        let mut depth = 1;
        while depth > 0 {
            match self.next() {
                Some(Token::Punct(c)) if c == open => depth += 1,
                Some(Token::Punct(c)) if c == close => depth -= 1,
                Some(_) => {}
                None => return Err("USD text ends inside a group".into()),
            }
        }
        Ok(())
    }
    fn value(&mut self) -> Result<Value, String> {
        match self.next() {
            Some(Token::Number(v)) => Ok(Value::Number(v)),
            Some(Token::Text(t)) | Some(Token::Word(t)) => Ok(Value::Text(t)),
            Some(Token::Punct(open @ ('(' | '['))) => {
                let close = if open == '(' { ')' } else { ']' };
                let mut items = vec![];
                loop {
                    if self.eat(close) {
                        break;
                    }
                    items.push(self.value()?);
                    if !self.eat(',') && self.peek() != Some(&Token::Punct(close)) {
                        return Err("A USD list is missing a comma".into());
                    }
                }
                Ok(Value::List(items))
            }
            Some(Token::Punct('{')) => {
                let mut samples = vec![];
                loop {
                    if self.eat('}') {
                        break;
                    }
                    let key = match self.next() {
                        Some(Token::Number(k)) => k,
                        _ => {
                            // A dictionary of named entries: skip it.
                            self.skip_group('{')?;
                            return Ok(Value::None);
                        }
                    };
                    if !self.eat(':') && !self.eat(';') {
                        // Time sample keys are followed by a colon.
                        return Err("A USD time sample is missing its colon".into());
                    }
                    samples.push((key, self.value()?));
                    self.eat(',');
                }
                samples.sort_by(|a, b| a.0.total_cmp(&b.0));
                Ok(Value::Samples(samples))
            }
            _ => Err("Unexpected token in a USD value".into()),
        }
    }
    /// The statements of a prim body up to its closing brace.
    fn body(&mut self, prim: &mut Prim) -> Result<(), String> {
        loop {
            match self.next() {
                Some(Token::Punct('}')) => return Ok(()),
                None => return Err("USD text ends inside a prim".into()),
                Some(Token::Word(w)) if w == "def" || w == "over" || w == "class" => {
                    prim.children.push(self.prim()?);
                }
                Some(Token::Word(w)) if w == "variantSet" => {
                    // variantSet "name" = { ... }: skipped.
                    while !self.eat('{') {
                        if self.next().is_none() {
                            return Err("USD text ends inside a variant set".into());
                        }
                    }
                    self.skip_group('{')?;
                }
                Some(Token::Word(w)) if w == "rel" => {
                    self.next();
                    if self.eat('=') {
                        self.value()?;
                    }
                    if self.eat('(') {
                        self.skip_group('(')?;
                    }
                }
                Some(Token::Word(w)) => {
                    // An attribute: qualifiers, a type, a name, maybe a value and metadata.
                    let mut words = vec![w];
                    while let Some(Token::Word(_)) = self.peek() {
                        if let Some(Token::Word(w)) = self.next() {
                            words.push(w);
                        }
                    }
                    let name = words.last().cloned().unwrap_or_default();
                    if self.eat('[') {
                        // A type like `int[]` splits as `int` `[` `]` before the name.
                        self.eat(']');
                        if let Some(Token::Word(w)) = self.next() {
                            words.push(w);
                        }
                    }
                    let name = if words.len() > 1 {
                        words.last().cloned().unwrap_or(name)
                    } else {
                        name
                    };
                    if self.eat('=') {
                        let value = self.value()?;
                        let name = name.trim_end_matches(".timeSamples").to_string();
                        prim.attributes.push((name, value));
                    }
                    if self.eat('(') {
                        self.skip_group('(')?;
                    }
                }
                Some(Token::Punct('(')) => self.skip_group('(')?,
                Some(Token::Punct('{')) => self.skip_group('{')?,
                Some(Token::Punct('[')) => self.skip_group('[')?,
                Some(_) => {}
            }
        }
    }
    /// A prim after its `def`/`over`/`class`: an optional type, its name, metadata, body.
    fn prim(&mut self) -> Result<Prim, String> {
        let mut prim = Prim::default();
        if let Some(Token::Word(kind)) = self.peek().cloned() {
            prim.kind = kind;
            self.at += 1;
        }
        match self.next() {
            Some(Token::Text(_)) => {}
            _ => return Err("A USD prim is missing its name".into()),
        }
        if self.eat('(') {
            self.skip_group('(')?;
        }
        if !self.eat('{') {
            return Err("A USD prim is missing its body".into());
        }
        self.body(&mut prim)?;
        Ok(prim)
    }
}

/// The stage's root prims, its metres per unit and whether it is Z-up.
fn stage(text: &str) -> Result<(Vec<Prim>, Option<f64>, bool), String> {
    if !text.starts_with("#usda") {
        return Err("This is not a USD text layer (.usda); binary .usdc is not read".into());
    }
    let mut parser = Parser {
        tokens: tokens(text)?,
        at: 0,
    };
    let (mut meters, mut z_up) = (None, false);
    if parser.eat('(') {
        // Stage metadata.
        let mut depth = 1;
        while depth > 0 {
            match parser.next() {
                Some(Token::Punct('(')) => depth += 1,
                Some(Token::Punct(')')) => depth -= 1,
                Some(Token::Word(w)) if depth == 1 && w == "metersPerUnit" => {
                    if parser.eat('=')
                        && let Some(Token::Number(v)) = parser.next()
                    {
                        meters = Some(v);
                    }
                }
                Some(Token::Word(w)) if depth == 1 && w == "upAxis" => {
                    if parser.eat('=')
                        && let Some(Token::Text(axis)) = parser.next()
                    {
                        z_up = axis.eq_ignore_ascii_case("z");
                    }
                }
                Some(_) => {}
                None => return Err("USD text ends inside the stage metadata".into()),
            }
        }
    }
    let mut roots = vec![];
    while let Some(token) = parser.next() {
        match token {
            Token::Word(w) if w == "def" || w == "over" || w == "class" => {
                roots.push(parser.prim()?)
            }
            Token::Punct(c @ ('(' | '[' | '{')) => parser.skip_group(c)?,
            _ => {}
        }
    }
    Ok((roots, meters, z_up))
}

/// The prim's own transform from its `xformOpOrder`.
fn local(prim: &Prim) -> Result<DMat4, String> {
    let order = prim
        .get("xformOpOrder")
        .map(Value::texts)
        .unwrap_or_default();
    let mut m = DMat4::IDENTITY;
    for op in order {
        let (inverse, op) = match op.strip_prefix("!invert!") {
            Some(rest) => (true, rest.to_string()),
            None => (false, op),
        };
        if op == "!resetXformStack!" {
            m = DMat4::IDENTITY;
            continue;
        }
        let Some(value) = prim.get(&op) else {
            continue;
        };
        let v = value.flat()?;
        let kind = op.trim_start_matches("xformOp:");
        let kind = kind.split(':').next().unwrap_or(kind);
        let r = |deg: f64| deg.to_radians();
        let step = match (kind, v.as_slice()) {
            ("translate", [x, y, z]) => DMat4::from_translation(DVec3::new(*x, *y, *z)),
            ("scale", [x, y, z]) => DMat4::from_scale(DVec3::new(*x, *y, *z)),
            ("rotateX", [a]) => DMat4::from_rotation_x(r(*a)),
            ("rotateY", [a]) => DMat4::from_rotation_y(r(*a)),
            ("rotateZ", [a]) => DMat4::from_rotation_z(r(*a)),
            // rotateXYZ turns about X first, then Y, then Z.
            ("rotateXYZ", [x, y, z]) => DMat4::from_euler(EulerRot::ZYX, r(*z), r(*y), r(*x)),
            ("rotateXZY", [x, y, z]) => DMat4::from_euler(EulerRot::YZX, r(*y), r(*z), r(*x)),
            ("rotateYXZ", [x, y, z]) => DMat4::from_euler(EulerRot::ZXY, r(*z), r(*x), r(*y)),
            ("rotateYZX", [x, y, z]) => DMat4::from_euler(EulerRot::XZY, r(*x), r(*z), r(*y)),
            ("rotateZXY", [x, y, z]) => DMat4::from_euler(EulerRot::YXZ, r(*y), r(*x), r(*z)),
            ("rotateZYX", [x, y, z]) => DMat4::from_euler(EulerRot::XYZ, r(*x), r(*y), r(*z)),
            // Quaternions are written real part first.
            ("orient", [w, x, y, z]) => {
                DMat4::from_quat(DQuat::from_xyzw(*x, *y, *z, *w).normalize())
            }
            // Row-major with the translation in the last row: its rows are glam's columns.
            ("transform", values) if values.len() == 16 => {
                DMat4::from_cols_array(&values.try_into().unwrap())
            }
            _ => return Err(format!("Unsupported USD transform {op}")),
        };
        let step = if inverse { step.inverse() } else { step };
        if !step.is_finite() {
            return Err(format!("USD transform {op} is not finite"));
        }
        m *= step;
    }
    Ok(m)
}

fn collect(
    prim: &Prim,
    parent: DMat4,
    mesh: &mut SurfaceMesh,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    let world = parent * local(prim)?;
    if prim.kind == "Mesh" {
        let points = prim
            .get("points")
            .ok_or("A USD mesh has no points")?
            .flat()?;
        let counts = prim
            .get("faceVertexCounts")
            .ok_or("A USD mesh has no faceVertexCounts")?
            .flat()?;
        let indices = prim
            .get("faceVertexIndices")
            .ok_or("A USD mesh has no faceVertexIndices")?
            .flat()?;
        if points.len() % 3 != 0 {
            return Err("USD mesh points come in threes".into());
        }
        let left = prim
            .get("orientation")
            .is_some_and(|o| o.texts().iter().any(|t| t == "leftHanded"));
        let base = mesh.vertices.len();
        let count = points.len() / 3;
        for p in points.as_chunks::<3>().0 {
            mesh.vertices
                .push(world.transform_point3(DVec3::new(p[0], p[1], p[2])));
        }
        let mut at = 0;
        let index = |k: f64| -> Result<usize, String> {
            if k < 0. || k.fract() != 0. || k as usize >= count {
                return Err("A USD face points past its mesh's points".into());
            }
            Ok(base + k as usize)
        };
        for n in counts {
            let n = n as usize;
            if n < 3 || at + n > indices.len() {
                return Err("USD face counts and indices disagree".into());
            }
            let face = &indices[at..at + n];
            for k in 1..n - 1 {
                let mut t = [index(face[0])?, index(face[k])?, index(face[k + 1])?];
                if left {
                    t.swap(1, 2);
                }
                mesh.triangles.push(t);
            }
            at += n;
        }
        if prim
            .attributes
            .iter()
            .any(|(n, v)| n == "points" && matches!(v, Value::Samples(s) if s.len() > 1))
        {
            warnings.push(
                "The mesh is animated: its first time sample is imported, as a static mesh".into(),
            );
        }
    }
    for child in &prim.children {
        collect(child, world, mesh, warnings)?;
    }
    Ok(())
}

/// The meshes of a USD text layer, in metres, Y-up.
pub fn usda(text: &str) -> Result<Imported, String> {
    let (roots, meters, z_up) = stage(text)?;
    let mut warnings = vec![];
    let meters = meters.unwrap_or_else(|| {
        warnings.push(
            "The stage declares no metersPerUnit: USD's default, centimetres, is assumed".into(),
        );
        0.01
    });
    if !(meters.is_finite() && meters > 0.) {
        return Err("The stage's metersPerUnit must be positive".into());
    }
    let mut to_scene = DMat4::from_scale(DVec3::splat(meters));
    if z_up {
        // Z-up to Y-up: what pointed up along Z points up along Y.
        to_scene = DMat4::from_rotation_x(-std::f64::consts::FRAC_PI_2) * to_scene;
    }
    let mut mesh = SurfaceMesh {
        vertices: vec![],
        triangles: vec![],
    };
    for root in &roots {
        collect(root, to_scene, &mut mesh, &mut warnings)?;
    }
    if mesh.triangles.is_empty() {
        return Err("The USD stage has no mesh".into());
    }
    warnings.push(
        "Geometry only: materials, normals, UVs, skinning and animation are not retained".into(),
    );
    crate::mesh_import::finish(mesh, 0, warnings)
}

/// The root layer of a USDZ package, which must be a text layer. USDZ entries are stored
/// uncompressed, so this reads the package's local headers in order.
pub fn usdz(bytes: &[u8]) -> Result<Imported, String> {
    let mut at = 0;
    while at + 30 <= bytes.len() {
        let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]) as usize;
        let u32_at = |i: usize| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap()) as usize;
        if u32_at(at) != 0x0403_4b50 {
            break;
        }
        let method = u16_at(at + 8);
        let size = u32_at(at + 18);
        let name_len = u16_at(at + 26);
        let extra = u16_at(at + 28);
        let name_end = at + 30 + name_len;
        let data = name_end + extra;
        if data + size > bytes.len() {
            return Err("The USDZ package is cut short".into());
        }
        let name = String::from_utf8_lossy(&bytes[at + 30..name_end]).to_ascii_lowercase();
        if method != 0 {
            return Err("USDZ entries must be stored uncompressed".into());
        }
        if name.ends_with(".usda") {
            let text = std::str::from_utf8(&bytes[data..data + size])
                .map_err(|e| format!("The USDZ layer is not UTF-8: {e}"))?;
            return usda(text);
        }
        if name.ends_with(".usdc") || name.ends_with(".usd") {
            return Err(
                "This USDZ holds a binary layer (.usdc), which is not read; export the stage as .usda".into(),
            );
        }
        at = data + size;
    }
    Err("The USDZ package has no USD layer".into())
}

/// A USD file by its extension: `.usda`, `.usdz`, or `.usd` holding text.
pub fn usd(bytes: &[u8], extension: &str) -> Result<Imported, String> {
    match extension {
        "usdz" => usdz(bytes),
        _ => {
            usda(std::str::from_utf8(bytes).map_err(
                |_| "This USD file is binary (.usdc), which is not read; save it as .usda",
            )?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUAD: &str = r#"#usda 1.0
(
    metersPerUnit = 0.01
    upAxis = "Z"
)

def Xform "World" (
    kind = "component"
)
{
    double3 xformOp:translate = (100, 0, 0)
    uniform token[] xformOpOrder = ["xformOp:translate"]

    def Mesh "Quad"
    {
        int[] faceVertexCounts = [4]
        int[] faceVertexIndices = [0, 1, 2, 3]
        point3f[] points = [(0, 0, 0), (10, 0, 0), (10, 0, 20), (0, 0, 20)]
        rel material:binding = </World/Looks/Red>
        color3f[] primvars:displayColor = [(1, 0, 0)] (
            interpolation = "constant"
        )
    }
}
"#;

    #[test]
    fn a_z_up_centimetre_quad_lands_y_up_in_metres() {
        let imported = usda(QUAD).unwrap();
        let mesh = &imported.mesh;
        assert_eq!(mesh.triangles.len(), 2);
        // Translated 1 m along X; Z-up turns the 20 cm along Z into 0.2 m up along Y.
        let top = mesh
            .vertices
            .iter()
            .map(|p| p.y)
            .fold(f64::NEG_INFINITY, f64::max);
        assert!((top - 0.2).abs() < 1e-9, "{top}");
        let x = mesh
            .vertices
            .iter()
            .map(|p| p.x)
            .fold(f64::INFINITY, f64::min);
        assert!((x - 1.0).abs() < 1e-9, "{x}");
    }

    #[test]
    fn transforms_compose_in_order_and_matrices_read_row_major() {
        let text = r#"#usda 1.0
(
    metersPerUnit = 1
)
def Mesh "M"
{
    matrix4d xformOp:transform = ( (2, 0, 0, 0), (0, 2, 0, 0), (0, 0, 2, 0), (1, 2, 3, 1) )
    uniform token[] xformOpOrder = ["xformOp:transform"]
    int[] faceVertexCounts = [3]
    int[] faceVertexIndices = [0, 1, 2]
    point3f[] points = [(1, 0, 0), (0, 1, 0), (0, 0, 1)]
}
"#;
        let mesh = usda(text).unwrap().mesh;
        assert!(mesh.vertices[0].distance(DVec3::new(3., 2., 3.)) < 1e-12);
    }

    #[test]
    fn binary_and_broken_files_are_refused() {
        assert!(usda("PXR-USDC").is_err());
        assert!(usda("#usda 1.0\ndef Mesh \"M\" {\n int[] faceVertexCounts = [3]\n").is_err());
        let bad = QUAD.replace("[0, 1, 2, 3]", "[0, 1, 2, 9]");
        assert!(usda(&bad).is_err());
    }
}
