//! The generic edit tool. Every undoable change to a pattern is one [`authoring::Command`];
//! the `edit` tool takes any of them as JSON and the window applies it through its undo history,
//! as a menu command would. `edit_ops` lists them with their JSON Schemas.
//!
//! The schemas are derived from the Rust source of the enum and of the types its fields use
//! (read at compile time with `include_str!`), honouring the serde attributes the derive
//! honours. A new command, a new field or a changed type therefore shows up without anyone
//! writing a schema by hand, and a test builds a sample value from every schema and checks that
//! serde accepts it as that command.
//!
//! [`authoring::Command`]: crate::authoring::Command
use crate::authoring::Command;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// The source files the command and the types of its fields are declared in.
const SOURCES: &[&str] = &[
    include_str!("authoring.rs"),
    include_str!("authoring/arrange2d.rs"),
    include_str!("garment/template.rs"),
    include_str!("garment/seam.rs"),
    include_str!("garment/shapes.rs"),
    include_str!("garment/shapes2d.rs"),
    include_str!("garment/pleats.rs"),
    include_str!("garment/markings.rs"),
    include_str!("garment/meshing.rs"),
    include_str!("garment/steam.rs"),
    include_str!("garment/trims.rs"),
    include_str!("garment/graphics.rs"),
    include_str!("garment/appearance.rs"),
    include_str!("garment/polygon.rs"),
    include_str!("garment/curves.rs"),
    include_str!("garment/pins.rs"),
    include_str!("garment/domain.rs"),
];

/// Commands `edit` does not take, and why.
pub const NOT_OFFERED: &[(&str, &str)] = &[
    (
        "SetUvLayout",
        "made by the UV editor's packing; its name is fixed by the window",
    ),
    (
        "BreakStitches",
        "made by the simulation when stitches break; tear a seam with SetSeamTears",
    ),
    (
        "RunScript",
        "replaces the whole garment with one a Python script saved; edit the pieces instead",
    ),
    (
        "Paste",
        "needs the window's clipboard; Duplicate or piece_edit duplicate copy a piece",
    ),
];

/// One field of a struct or struct variant, as serde reads it.
#[derive(Clone, Debug)]
struct Field {
    name: String,
    ty: String,
    doc: String,
    required: bool,
    /// Its schema is not derivable (a custom `with`), or it is flattened into its parent.
    custom: bool,
    flatten: bool,
}

#[derive(Clone, Debug)]
enum VariantKind {
    Unit,
    Tuple(Vec<String>),
    Struct(Vec<Field>),
}

#[derive(Clone, Debug)]
struct Variant {
    name: String,
    doc: String,
    kind: VariantKind,
    skipped: bool,
}

#[derive(Clone, Debug)]
enum Shape {
    Struct(Vec<Field>),
    Tuple(Vec<String>),
    Enum(Vec<Variant>),
    Unit,
}

/// The container's serde attributes that change its JSON.
#[derive(Clone, Debug, Default)]
struct Attrs {
    rename_all: Option<String>,
    tag: Option<String>,
    content: Option<String>,
    untagged: bool,
    default: bool,
    transparent: bool,
    from: Option<String>,
    deserializes: bool,
}

#[derive(Clone, Debug)]
struct Item {
    doc: String,
    shape: Shape,
    attrs: Attrs,
}

/// One command `edit` takes.
#[derive(Clone, Debug)]
pub struct Op {
    pub name: String,
    pub doc: String,
    /// The JSON Schema of its fields (`$defs` holds the named types it uses).
    pub schema: Value,
}

struct Catalog {
    items: BTreeMap<String, Item>,
    ops: Vec<Op>,
}

fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let mut items = BTreeMap::new();
        for source in SOURCES {
            for (name, item) in parse_items(source) {
                items.entry(name).or_insert(item);
            }
        }
        let mut ops = Vec::new();
        if let Some(Item {
            shape: Shape::Enum(variants),
            ..
        }) = items.get("Command")
        {
            for v in variants {
                if v.skipped || NOT_OFFERED.iter().any(|(n, _)| *n == v.name) {
                    continue;
                }
                let mut defs = BTreeMap::new();
                let schema = match &v.kind {
                    VariantKind::Struct(fields) => {
                        struct_schema(&items, fields, &Attrs::default(), &mut defs)
                    }
                    VariantKind::Tuple(types) => tuple_schema(&items, types, &mut defs),
                    VariantKind::Unit => json!({"type": "object", "properties": {}}),
                };
                let mut schema = schema;
                if !defs.is_empty() {
                    schema["$defs"] = Value::Object(defs.into_iter().collect());
                }
                ops.push(Op {
                    name: v.name.clone(),
                    doc: v.doc.clone(),
                    schema,
                });
            }
        }
        Catalog { items, ops }
    })
}

/// Every command `edit` takes.
pub fn ops() -> &'static [Op] {
    &catalog().ops
}

/// The commands whose name or description has every word of `query` (all when empty), with
/// their schemas when `names` asks for them by name or few match.
pub fn describe(query: &str, names: &[String]) -> Value {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let matching: Vec<&Op> = ops()
        .iter()
        .filter(|op| {
            if !names.is_empty() {
                return names.iter().any(|n| n.eq_ignore_ascii_case(&op.name));
            }
            let text = format!("{} {}", op.name, op.doc).to_lowercase();
            words.iter().all(|w| text.contains(w.as_str()))
        })
        .collect();
    let full = !names.is_empty() || matching.len() <= 6;
    json!({
        "units": "These are FashionCraft's own edit commands: lengths in metres unless a field's \
                  name says mm, degrees, percent or a fraction; 2D positions [x, y] in metres in \
                  the piece's material frame (x right, y up); 3D [x, y, z] metres, y up.",
        "ops": matching.iter().map(|op| {
            let mut entry = json!({"op": op.name, "does": first_sentence(&op.doc)});
            if full {
                entry["does"] = json!(op.doc);
                entry["fields"] = op.schema.clone();
            }
            entry
        }).collect::<Vec<_>>(),
        "note": if full { "" } else { "Ask for ops by name to get their fields" },
        "not_offered": NOT_OFFERED.iter().map(|(n, why)| json!({"op": n, "because": why})).collect::<Vec<_>>(),
    })
}

/// The command an agent sent: its op name and its fields, checked against the enum by serde.
pub fn parse(op: &str, fields: &Value) -> Result<Command, String> {
    if let Some((_, why)) = NOT_OFFERED.iter().find(|(n, _)| *n == op) {
        return Err(format!("edit does not take {op}: {why}"));
    }
    let Some(known) = ops().iter().find(|o| o.name == op) else {
        let near: Vec<&str> = ops()
            .iter()
            .filter(|o| o.name.to_lowercase().contains(&op.to_lowercase()))
            .map(|o| o.name.as_str())
            .take(8)
            .collect();
        return Err(if near.is_empty() {
            format!("No edit op '{op}': edit_ops lists them")
        } else {
            format!("No edit op '{op}'. Did you mean {}?", near.join(", "))
        });
    };
    let fields = if fields.is_null() {
        json!({})
    } else {
        fields.clone()
    };
    serde_json::from_value::<Command>(json!({ known.name.clone(): fields })).map_err(|e| {
        format!("{op}'s fields are wrong: {e}. edit_ops with names [\"{op}\"] shows its schema.")
    })
}

fn first_sentence(doc: &str) -> String {
    match doc.find(". ") {
        Some(i) => doc[..=i].to_owned(),
        None => doc.to_owned(),
    }
}

// ---------------------------------------------------------------------------------------------
// Reading the declarations.

/// The source with plain comments removed and doc comments turned into `\u{1}text\u{2}`, so
/// brackets in prose never count.
fn clean(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    for line in source.lines() {
        let trimmed = line.trim_start();
        if let Some(doc) = trimmed.strip_prefix("///") {
            out.push('\u{1}');
            out.push_str(doc.trim());
            out.push('\u{2}');
            out.push('\n');
        } else if trimmed.starts_with("//") {
            out.push('\n');
        } else {
            let code = match line.find("//") {
                Some(i) if !line[..i].contains('"') => &line[..i],
                _ => line,
            };
            out.push_str(code);
            out.push('\n');
        }
    }
    out
}

/// The `{…}` or `(…)` group that starts at `open`, without its brackets, and where it ends.
fn group(text: &str, open: usize) -> Option<(&str, usize)> {
    let bytes = text.as_bytes();
    let (o, c) = match bytes[open] {
        b'{' => (b'{', b'}'),
        b'(' => (b'(', b')'),
        _ => return None,
    };
    let mut depth = 0usize;
    let mut in_doc = false;
    for (i, &b) in bytes.iter().enumerate().skip(open) {
        match b {
            1 => in_doc = true,
            2 => in_doc = false,
            _ if in_doc => {}
            _ if b == o => depth += 1,
            _ if b == c => {
                depth -= 1;
                if depth == 0 {
                    return Some((&text[open + 1..i], i));
                }
            }
            _ => {}
        }
    }
    None
}

/// Splits at top-level commas (outside brackets and doc text).
fn split_top(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut in_doc = false;
    let mut start = 0;
    for (i, ch) in text.char_indices() {
        match ch {
            '\u{1}' => in_doc = true,
            '\u{2}' => in_doc = false,
            _ if in_doc => {}
            '{' | '(' | '[' | '<' => depth += 1,
            '}' | ')' | ']' | '>' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&text[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts.into_iter().filter(|p| !p.trim().is_empty()).collect()
}

/// A chunk's doc text, its `#[…]` attributes, and the code after them.
fn parts(chunk: &str) -> (String, Vec<String>, String) {
    let mut doc = Vec::new();
    let mut attrs = Vec::new();
    let mut rest = chunk.trim_start();
    loop {
        if let Some(after) = rest.strip_prefix('\u{1}') {
            let end = after.find('\u{2}').unwrap_or(after.len());
            doc.push(after[..end].to_owned());
            rest = after.get(end + 1..).unwrap_or("").trim_start();
        } else if rest.starts_with("#[") {
            let mut depth = 0;
            let mut end = rest.len();
            for (i, ch) in rest.char_indices() {
                match ch {
                    '[' => depth += 1,
                    ']' => {
                        depth -= 1;
                        if depth == 0 {
                            end = i + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            attrs.push(rest[..end].to_owned());
            rest = rest[end..].trim_start();
        } else {
            break;
        }
    }
    let code: String = rest
        .chars()
        .filter(|c| *c != '\u{1}' && *c != '\u{2}')
        .collect();
    (doc.join(" "), attrs, code.trim().to_owned())
}

/// The value of `key = "…"` in serde attributes, or whether a bare `key` is there.
fn serde_value(attrs: &[String], key: &str) -> Option<String> {
    for a in attrs.iter().filter(|a| a.starts_with("#[serde(")) {
        let inner = &a[8..a.len().saturating_sub(2)];
        for part in split_top(inner) {
            let part = part.trim();
            if part == key {
                return Some(String::new());
            }
            if let Some(v) = part.strip_prefix(key).map(str::trim_start)
                && let Some(v) = v.strip_prefix('=')
            {
                return Some(v.trim().trim_matches('"').to_owned());
            }
            if let Some(v) = part.strip_prefix(key).map(str::trim_start)
                && v.starts_with('(')
            {
                return Some(String::new());
            }
        }
    }
    None
}

fn fields_of(body: &str) -> Vec<Field> {
    split_top(body)
        .into_iter()
        .filter_map(|chunk| {
            let (doc, attrs, code) = parts(chunk);
            let code = code
                .strip_prefix("pub(crate)")
                .or_else(|| code.strip_prefix("pub(super)"))
                .or_else(|| code.strip_prefix("pub "))
                .unwrap_or(&code)
                .trim();
            let (name, ty) = code.split_once(':')?;
            if serde_value(&attrs, "skip").is_some()
                || serde_value(&attrs, "skip_deserializing").is_some()
            {
                return None;
            }
            let ty: String = ty.split_whitespace().collect::<Vec<_>>().join(" ");
            let name = serde_value(&attrs, "rename").unwrap_or_else(|| name.trim().to_owned());
            let optional = ty.starts_with("Option<") || serde_value(&attrs, "default").is_some();
            Some(Field {
                name,
                ty,
                doc,
                required: !optional,
                custom: serde_value(&attrs, "with").is_some()
                    || serde_value(&attrs, "deserialize_with").is_some(),
                flatten: serde_value(&attrs, "flatten").is_some(),
            })
        })
        .collect()
}

fn container_attrs(attrs: &[String]) -> Attrs {
    Attrs {
        rename_all: serde_value(attrs, "rename_all"),
        tag: serde_value(attrs, "tag"),
        content: serde_value(attrs, "content"),
        untagged: serde_value(attrs, "untagged").is_some(),
        default: serde_value(attrs, "default").is_some(),
        transparent: serde_value(attrs, "transparent").is_some(),
        from: serde_value(attrs, "from").or_else(|| serde_value(attrs, "try_from")),
        deserializes: attrs
            .iter()
            .any(|a| a.starts_with("#[derive(") && a.contains("Deserialize")),
    }
}

/// Every non-generic `pub struct` and `pub enum` declared in `source`.
fn parse_items(source: &str) -> Vec<(String, Item)> {
    let text = clean(source);
    let mut items = Vec::new();
    let mut search = 0;
    while let Some(found) = ["pub struct ", "pub enum "]
        .iter()
        .filter_map(|k| text[search..].find(k).map(|i| (search + i, *k)))
        .min()
    {
        let (at, keyword) = found;
        search = at + keyword.len();
        // Only declarations at the start of a line.
        let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
        if !text[line_start..at].trim().is_empty() {
            continue;
        }
        let after = &text[at + keyword.len()..];
        let name: String = after
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        let rest = after[name.len()..].trim_start();
        if name.is_empty() || rest.starts_with('<') {
            continue;
        }
        // The doc comments and attributes just above.
        let mut head = Vec::new();
        let mut cursor = line_start;
        while cursor > 0 {
            let prev_start = text[..cursor - 1].rfind('\n').map_or(0, |i| i + 1);
            let line = text[prev_start..cursor - 1].trim();
            if line.starts_with('\u{1}') || line.starts_with("#[") || line.ends_with(")]") {
                head.push(line.to_owned());
                cursor = prev_start;
            } else {
                break;
            }
        }
        head.reverse();
        let (doc, attrs, _) = parts(&head.join("\n"));
        let open = at + keyword.len() + name.len();
        let open = text[open..]
            .find(['{', '(', ';'])
            .map(|i| open + i)
            .unwrap_or(text.len() - 1);
        let shape = match text.as_bytes()[open] {
            b';' => Shape::Unit,
            _ => {
                let Some((body, end)) = group(&text, open) else {
                    continue;
                };
                search = end;
                if keyword == "pub struct " {
                    if text.as_bytes()[open] == b'(' {
                        Shape::Tuple(
                            split_top(body)
                                .into_iter()
                                .map(|t| {
                                    let (_, _, code) = parts(t);
                                    code.trim_start_matches("pub ")
                                        .split_whitespace()
                                        .collect::<Vec<_>>()
                                        .join(" ")
                                })
                                .collect(),
                        )
                    } else {
                        Shape::Struct(fields_of(body))
                    }
                } else {
                    Shape::Enum(variants_of(body))
                }
            }
        };
        items.push((
            name,
            Item {
                doc,
                shape,
                attrs: container_attrs(&attrs),
            },
        ));
    }
    items
}

fn variants_of(body: &str) -> Vec<Variant> {
    split_top(body)
        .into_iter()
        .filter_map(|chunk| {
            let (doc, attrs, code) = parts(chunk);
            let name: String = code
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if name.is_empty() {
                return None;
            }
            let rest = code[name.len()..].trim_start();
            let kind = if rest.starts_with('{') {
                let (inner, _) = group(rest, 0)?;
                VariantKind::Struct(fields_of(inner))
            } else if rest.starts_with('(') {
                let (inner, _) = group(rest, 0)?;
                VariantKind::Tuple(
                    split_top(inner)
                        .into_iter()
                        .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
                        .collect(),
                )
            } else {
                VariantKind::Unit
            };
            let renamed = serde_value(&attrs, "rename");
            Some(Variant {
                name: renamed.unwrap_or(name),
                doc,
                kind,
                skipped: serde_value(&attrs, "skip").is_some()
                    || serde_value(&attrs, "skip_deserializing").is_some(),
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Schemas.

fn renamed(name: &str, rule: Option<&str>) -> String {
    let words = || {
        let mut words: Vec<String> = Vec::new();
        for (i, ch) in name.char_indices() {
            if ch.is_uppercase() && i > 0 {
                words.push(String::new());
            }
            if words.is_empty() {
                words.push(String::new());
            }
            words.last_mut().unwrap().push(ch);
        }
        words
    };
    match rule {
        Some("snake_case") => words()
            .iter()
            .map(|w| w.to_lowercase())
            .collect::<Vec<_>>()
            .join("_"),
        Some("kebab-case") => words()
            .iter()
            .map(|w| w.to_lowercase())
            .collect::<Vec<_>>()
            .join("-"),
        Some("lowercase") => name.to_lowercase(),
        Some("UPPERCASE") => name.to_uppercase(),
        Some("camelCase") => {
            let mut c = name.chars();
            c.next()
                .map(|f| f.to_lowercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        }
        Some("SCREAMING_SNAKE_CASE") => words()
            .iter()
            .map(|w| w.to_uppercase())
            .collect::<Vec<_>>()
            .join("_"),
        _ => name.to_owned(),
    }
}

/// The generic arguments of `Outer<…>`, split at top-level commas.
fn generic_args(ty: &str) -> Vec<&str> {
    let open = ty.find('<').unwrap_or(0);
    let inner = &ty[open + 1..ty.len() - 1];
    split_top(inner).into_iter().map(str::trim).collect()
}

fn type_schema(
    items: &BTreeMap<String, Item>,
    ty: &str,
    defs: &mut BTreeMap<String, Value>,
) -> Value {
    let ty = ty.trim();
    if let Some(inner) = ty.strip_prefix('(') {
        let inner = inner.strip_suffix(')').unwrap_or(inner);
        let types: Vec<String> = split_top(inner)
            .into_iter()
            .map(|t| t.trim().to_owned())
            .collect();
        return tuple_schema(items, &types, defs);
    }
    if let Some(inner) = ty.strip_prefix('[') {
        let inner = inner.strip_suffix(']').unwrap_or(inner);
        return match inner.rsplit_once(';') {
            Some((t, n)) => {
                let n: usize = n.trim().parse().unwrap_or(0);
                json!({"type": "array", "items": type_schema(items, t, defs), "minItems": n, "maxItems": n})
            }
            None => json!({"type": "array", "items": type_schema(items, inner, defs)}),
        };
    }
    let ty = ty.trim_start_matches('&').trim_start_matches("'static ");
    let base = ty.split('<').next().unwrap_or(ty);
    let last = base.rsplit("::").next().unwrap_or(base).trim();
    let numbers = |n: usize, what: &str| json!({"type": "array", "items": {"type": "number"}, "minItems": n, "maxItems": n, "description": what});
    match last {
        "String" | "str" | "char" | "PathBuf" => json!({"type": "string"}),
        "f64" | "f32" => json!({"type": "number"}),
        "u8" | "u16" | "u32" | "u64" | "usize" => json!({"type": "integer", "minimum": 0}),
        "i8" | "i16" | "i32" | "i64" | "isize" => json!({"type": "integer"}),
        "bool" => json!({"type": "boolean"}),
        "DVec2" | "Vec2" => numbers(2, "[x, y]"),
        "DVec3" | "Vec3" => numbers(3, "[x, y, z]"),
        "DQuat" | "Quat" => numbers(
            4,
            "a rotation quaternion [x, y, z, w]; [0, 0, 0, 1] is none",
        ),
        "Vec" | "VecDeque" | "BTreeSet" | "HashSet" | "SmallVec" => {
            let args = generic_args(ty);
            json!({"type": "array", "items": type_schema(items, args.first().copied().unwrap_or("String"), defs)})
        }
        "Option" => {
            let inner = type_schema(items, generic_args(ty)[0], defs);
            json!({"anyOf": [inner, {"type": "null"}]})
        }
        "Box" | "Arc" | "Rc" => type_schema(items, generic_args(ty)[0], defs),
        "BTreeMap" | "HashMap" | "IndexMap" => {
            let args = generic_args(ty);
            json!({"type": "object", "additionalProperties": type_schema(items, args.get(1).copied().unwrap_or("String"), defs)})
        }
        name => match items.get(name) {
            Some(_) => {
                if !defs.contains_key(name) {
                    // Insert first so a type that contains itself refers back to it.
                    defs.insert(name.to_owned(), json!({}));
                    let schema = item_schema(items, name, defs);
                    defs.insert(name.to_owned(), schema);
                }
                json!({"$ref": format!("#/$defs/{name}")})
            }
            None => json!({"description": format!("a {name}")}),
        },
    }
}

fn tuple_schema(
    items: &BTreeMap<String, Item>,
    types: &[String],
    defs: &mut BTreeMap<String, Value>,
) -> Value {
    if types.len() == 1 {
        return type_schema(items, &types[0], defs);
    }
    json!({
        "type": "array",
        "prefixItems": types.iter().map(|t| type_schema(items, t, defs)).collect::<Vec<_>>(),
        "minItems": types.len(),
        "maxItems": types.len(),
    })
}

fn struct_schema(
    items: &BTreeMap<String, Item>,
    fields: &[Field],
    attrs: &Attrs,
    defs: &mut BTreeMap<String, Value>,
) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for f in fields {
        let name = renamed(&f.name, attrs.rename_all.as_deref());
        if f.flatten {
            let inner = type_schema(items, &f.ty, defs);
            let resolved = resolve(&inner, defs);
            if let Some(props) = resolved.get("properties").and_then(Value::as_object) {
                for (k, v) in props {
                    properties.insert(k.clone(), v.clone());
                }
            }
            if let Some(req) = resolved.get("required").and_then(Value::as_array) {
                required.extend(req.iter().filter_map(|v| v.as_str().map(str::to_owned)));
            }
            continue;
        }
        let mut schema = if f.custom {
            json!({})
        } else {
            type_schema(items, &f.ty, defs)
        };
        if !f.doc.is_empty() {
            schema["description"] = json!(f.doc);
        }
        if f.required && !attrs.default {
            required.push(name.clone());
        }
        properties.insert(name, schema);
    }
    json!({"type": "object", "properties": properties, "required": required})
}

fn resolve(schema: &Value, defs: &BTreeMap<String, Value>) -> Value {
    match schema.get("$ref").and_then(Value::as_str) {
        Some(r) => defs
            .get(r.trim_start_matches("#/$defs/"))
            .cloned()
            .unwrap_or_default(),
        None => schema.clone(),
    }
}

fn item_schema(
    items: &BTreeMap<String, Item>,
    name: &str,
    defs: &mut BTreeMap<String, Value>,
) -> Value {
    let item = &items[name];
    let attrs = &item.attrs;
    let mut schema = if let Some(from) = &attrs.from {
        type_schema(items, from, defs)
    } else {
        match &item.shape {
            Shape::Unit => json!({"type": "null"}),
            Shape::Tuple(types) => tuple_schema(items, types, defs),
            Shape::Struct(fields) if attrs.transparent && fields.len() == 1 => {
                type_schema(items, &fields[0].ty, defs)
            }
            Shape::Struct(fields) => struct_schema(items, fields, attrs, defs),
            Shape::Enum(variants) => enum_schema(items, variants, attrs, defs),
        }
    };
    if !item.doc.is_empty() && schema.is_object() && schema.get("description").is_none() {
        schema["description"] = json!(first_sentence(&item.doc));
    }
    schema
}

fn enum_schema(
    items: &BTreeMap<String, Item>,
    variants: &[Variant],
    attrs: &Attrs,
    defs: &mut BTreeMap<String, Value>,
) -> Value {
    let variants: Vec<&Variant> = variants.iter().filter(|v| !v.skipped).collect();
    let name_of = |v: &Variant| renamed(&v.name, attrs.rename_all.as_deref());
    if variants.iter().all(|v| matches!(v.kind, VariantKind::Unit))
        && attrs.tag.is_none()
        && !attrs.untagged
    {
        return json!({"type": "string", "enum": variants.iter().map(|v| name_of(v)).collect::<Vec<_>>()});
    }
    let mut options = Vec::new();
    for v in &variants {
        let name = name_of(v);
        let content = match &v.kind {
            VariantKind::Unit => None,
            VariantKind::Tuple(types) => Some(tuple_schema(items, types, defs)),
            VariantKind::Struct(fields) => {
                Some(struct_schema(items, fields, &Attrs::default(), defs))
            }
        };
        let option = match (&attrs.tag, &attrs.content, attrs.untagged, content) {
            (_, _, true, Some(c)) => c,
            (_, _, true, None) => json!({"type": "null"}),
            (Some(tag), Some(key), _, Some(c)) => json!({
                "type": "object",
                "properties": {tag: {"const": name}, key: c},
                "required": [tag, key],
            }),
            (Some(tag), _, _, Some(mut c)) => {
                if let Some(props) = c.get_mut("properties").and_then(Value::as_object_mut) {
                    props.insert(tag.clone(), json!({"const": name}));
                }
                if let Some(req) = c.get_mut("required").and_then(Value::as_array_mut) {
                    req.push(json!(tag));
                }
                c
            }
            (Some(tag), _, _, None) => json!({
                "type": "object",
                "properties": {tag: {"const": name}},
                "required": [tag],
            }),
            (None, _, _, None) => json!({"const": name}),
            (None, _, _, Some(c)) => json!({
                "type": "object",
                "properties": {name.clone(): c},
                "required": [name],
            }),
        };
        let mut option = option;
        if !v.doc.is_empty() && option.is_object() {
            option["description"] = json!(first_sentence(&v.doc));
        }
        options.push(option);
    }
    json!({"oneOf": options})
}

/// Named types the commands use that no source declares or that serde reads its own way; a
/// test keeps this empty.
pub fn unresolved_types() -> Vec<String> {
    let mut out = Vec::new();
    fn walk(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::Object(m) => {
                if m.len() == 1
                    && let Some(d) = m.get("description").and_then(Value::as_str)
                    && let Some(name) = d.strip_prefix("a ")
                {
                    out.push(name.to_owned());
                }
                m.values().for_each(|x| walk(x, out));
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    for op in ops() {
        walk(&op.schema, &mut out);
    }
    for (name, item) in &catalog().items {
        let used = ops().iter().any(|op| {
            op.schema
                .get("$defs")
                .is_some_and(|d| d.get(name.as_str()).is_some())
        });
        if used && !item.attrs.deserializes && item.attrs.from.is_none() {
            out.push(format!("{name} (no derived Deserialize)"));
        }
    }
    out.sort();
    out.dedup();
    out
}

/// A value that fits `schema`: the smallest one, for the tests.
pub fn sample(schema: &Value, defs: &Value) -> Value {
    if let Some(r) = schema.get("$ref").and_then(Value::as_str) {
        let def = &defs[r.trim_start_matches("#/$defs/")];
        return sample(def, defs);
    }
    if let Some(c) = schema.get("const") {
        return c.clone();
    }
    if let Some(e) = schema.get("enum").and_then(Value::as_array) {
        return e[0].clone();
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(options) = schema.get(key).and_then(Value::as_array) {
            return sample(&options[0], defs);
        }
    }
    match schema.get("type").and_then(Value::as_str) {
        Some("string") => json!("x"),
        Some("number") => json!(0.5),
        Some("integer") => json!(0),
        Some("boolean") => json!(false),
        Some("null") => Value::Null,
        Some("array") => {
            if let Some(prefix) = schema.get("prefixItems").and_then(Value::as_array) {
                return Value::Array(prefix.iter().map(|s| sample(s, defs)).collect());
            }
            let n = schema.get("minItems").and_then(Value::as_u64).unwrap_or(0) as usize;
            let item = schema.get("items").cloned().unwrap_or_default();
            Value::Array((0..n).map(|_| sample(&item, defs)).collect())
        }
        Some("object") => {
            let mut out = Map::new();
            let required: Vec<&str> = schema
                .get("required")
                .and_then(Value::as_array)
                .map(|r| r.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            if let Some(props) = schema.get("properties").and_then(Value::as_object) {
                for (k, v) in props {
                    if required.contains(&k.as_str()) {
                        out.insert(k.clone(), sample(v, defs));
                    }
                }
            }
            Value::Object(out)
        }
        _ => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_is_offered_or_explained() {
        let source = include_str!("authoring.rs");
        let start = source.find("pub fn name(&self)").unwrap();
        let body = &source[start..start + source[start..].find("\n    }\n").unwrap()];
        let arms: std::collections::BTreeSet<&str> = body
            .split("Self::")
            .skip(1)
            .map(|a| a.split(|c: char| !c.is_alphanumeric()).next().unwrap_or(""))
            .collect();
        let arms = arms.len();
        assert_eq!(
            ops().len() + NOT_OFFERED.len(),
            arms,
            "authoring::Command has {arms} variants: each is an edit op or in NOT_OFFERED"
        );
    }

    #[test]
    fn every_schema_is_one_serde_takes() {
        assert!(ops().len() > 100);
        assert_eq!(unresolved_types(), Vec::<String>::new());
        for op in ops() {
            let defs = op.schema.get("$defs").cloned().unwrap_or_default();
            let value = sample(&op.schema, &defs);
            if let Err(e) = serde_json::from_value::<Command>(json!({op.name.clone(): value})) {
                panic!("{}: sample {value} rejected: {e}", op.name);
            }
        }
    }

    #[test]
    fn parse_checks_fields_and_names() {
        let error = |op: &str, fields: Value| parse(op, &fields).err().unwrap_or_default();
        assert!(error("Sew", json!({})).contains("missing field"));
        assert!(error("Sow", json!({})).contains("No edit op"));
        assert!(error("RunScript", json!({})).contains("does not take"));
        assert!(error("Duplicate", json!({"panel": "a", "extra": 1})).contains("unknown field"));
        assert!(parse("Duplicate", &json!({"panel": "front"})).is_ok());
        let sew = describe("", &["Sew".into()]);
        assert_eq!(sew["ops"][0]["op"], "Sew");
        assert!(sew["ops"][0]["fields"]["$defs"]["BoundaryRef"].is_object());
    }
}
