//! The AAMA/ASTM grade rule table (`.RUL`): the growths that grade the points of an apparel
//! DXF, written beside it.
//!
//! An ASCII file: a header of `KEY: value` lines (version, author, creation date and time,
//! units, rule table name, number of sizes, size list from small to large, sample size),
//! then one `RULE: DELTA <id>` per grade rule followed by one `x, y` growth per size in
//! size-list order, measured from the sample size (whose growth is `0, 0`), and `END`.
//! Metric tables are in millimetres with 2 decimals, English ones in inches with 4.
//! Separators between values may be commas, spaces, tabs or line breaks.
//!
//! Source: S. Bakker, *ePattern ASTM Standard*, DH Patterns and Fit (2011–12), the
//! "Grading" section: syntax table and example table. ASTM D6673 itself was not consulted.
//! What FashionCraft decides where that description is silent is listed in
//! `docs/grading.md`.
use crate::garment::dxf_aama::Units;
use glam::DVec2;
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// The version line of an ASTM rule table, as in the public example.
pub const ASTM_VERSION: &str = "ASTM/D13 Proposal 1 Version: D 6673-04";
/// Growth pairs per line, as in the public example.
const PAIRS_PER_LINE: usize = 4;
/// Most rules a table may hold when read.
pub const MAX_RULES: usize = 200_000;

/// A grade rule table. Growths are in metres, DXF orientation (+Y up), one per size.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RuleTable {
    pub name: String,
    pub units: Units,
    /// Smallest to largest, sample size included.
    pub sizes: Vec<String>,
    pub sample: String,
    pub author: String,
    /// `(dd-mm-yyyy, hh:mm)`.
    pub created: Option<(String, String)>,
    /// Version line, written as given; empty writes none.
    pub version: String,
    pub rules: BTreeMap<i64, Vec<DVec2>>,
    /// What reading had to adjust, for the import review.
    pub notes: Vec<String>,
}

/// A size name as the size list can hold it: 7-bit ASCII, no spaces.
pub fn size_word(name: &str) -> String {
    let word: String = crate::garment::dxf_aama::ascii(name)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_");
    if word.is_empty() { "?".into() } else { word }
}

fn number(value: f64, units: Units) -> String {
    let (scale, decimals) = match units {
        Units::Millimetres => (0.001, 2),
        Units::Inches => (0.0254, 4),
    };
    let mut s = format!("{:.*}", decimals, value / scale);
    if s.starts_with('-') && s[1..].bytes().all(|b| b == b'0' || b == b'.') {
        s.remove(0);
    }
    s
}

/// The rule table as text.
pub fn write(table: &RuleTable) -> Result<String, String> {
    if table.sizes.is_empty() || !table.sizes.contains(&table.sample) {
        return Err("A rule table needs its sizes and a sample size among them".into());
    }
    let mut out = String::new();
    if !table.version.is_empty() {
        writeln!(out, "{}", table.version).unwrap();
    }
    writeln!(out, "AUTHOR: {}", table.author).unwrap();
    let (date, time) = table
        .created
        .clone()
        .unwrap_or_else(|| (String::new(), String::new()));
    writeln!(out, "CREATION DATE: {date}").unwrap();
    writeln!(out, "CREATION TIME: {time}").unwrap();
    let units = match table.units {
        Units::Millimetres => "METRIC",
        Units::Inches => "ENGLISH",
    };
    writeln!(out, "UNITS: {units}").unwrap();
    writeln!(out, "GRADE RULE TABLE: {}", size_word(&table.name)).unwrap();
    writeln!(out, "NUMBER OF SIZES: {}", table.sizes.len()).unwrap();
    let sizes: Vec<String> = table.sizes.iter().map(|s| size_word(s)).collect();
    writeln!(out, "SIZE LIST: {}", sizes.join(" ")).unwrap();
    writeln!(out, "SAMPLE SIZE: {}", size_word(&table.sample)).unwrap();
    for (id, growths) in &table.rules {
        if growths.len() != table.sizes.len() {
            return Err(format!("Rule {id} needs one growth per size"));
        }
        writeln!(out, "RULE: DELTA {id}").unwrap();
        for line in growths.chunks(PAIRS_PER_LINE) {
            let pairs: Vec<String> = line
                .iter()
                .map(|g| format!("{}, {}", number(g.x, table.units), number(g.y, table.units)))
                .collect();
            writeln!(out, "{}", pairs.join(" ")).unwrap();
        }
    }
    writeln!(out, "END").unwrap();
    Ok(out)
}

/// The header key of a line, in capitals with single spaces, and its value.
fn key_of(line: &str) -> Option<(String, &str)> {
    let (key, value) = line.split_once(':')?;
    let key = key.split_whitespace().collect::<Vec<_>>().join(" ");
    (!key.is_empty() && key.len() <= 48).then(|| (key.to_uppercase(), value.trim()))
}

/// Read a rule table. Keys are matched without regard to case or spacing; values may span
/// lines; a sample size with a growth other than zero is rebased to zero (and noted).
pub fn read(text: &str) -> Result<RuleTable, String> {
    let mut table = RuleTable::default();
    let mut declared: Option<usize> = None;
    let mut units: Option<Units> = None;
    // Every token after the first RULE:, with the rule keyword marking where rules start.
    let mut body = String::new();
    let mut in_rules = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let upper = line.to_uppercase();
        if upper.starts_with("RULE") && key_of(line).is_some_and(|(k, _)| k == "RULE") {
            in_rules = true;
        }
        if in_rules {
            body.push_str(line);
            body.push('\n');
            continue;
        }
        if upper.starts_with("ASTM/")
            || (upper.contains("VERSION") && !upper.starts_with("VERSION"))
        {
            table.version = line.into();
            continue;
        }
        let Some((key, value)) = key_of(line) else {
            table.notes.push(format!("Line not understood: {line}"));
            continue;
        };
        match key.as_str() {
            "AUTHOR" => table.author = value.into(),
            "CREATION DATE" => {
                let time = table.created.take().map(|c| c.1).unwrap_or_default();
                table.created = Some((value.into(), time));
            }
            "CREATION TIME" => {
                let date = table.created.take().map(|c| c.0).unwrap_or_default();
                table.created = Some((date, value.into()));
            }
            "UNITS" => {
                let word = value.to_uppercase();
                units = if word.starts_with("METRIC") {
                    Some(Units::Millimetres)
                } else if word.starts_with("ENGLISH") {
                    Some(Units::Inches)
                } else {
                    return Err(format!("Unknown units {value}"));
                };
            }
            "GRADE RULE TABLE" => table.name = value.into(),
            "NUMBER OF SIZES" => {
                declared = Some(
                    value
                        .parse()
                        .map_err(|_| format!("Number of sizes {value} is not a number"))?,
                )
            }
            "SIZE LIST" => table.sizes = value.split_whitespace().map(String::from).collect(),
            "SAMPLE SIZE" => table.sample = value.into(),
            // PRODUCT, VERSION, UNIT FORMAT and other optional keys say nothing the growths need.
            _ => {}
        }
    }
    table.units = units.ok_or("The rule table does not say its units")?;
    if table.sizes.is_empty() {
        return Err("The rule table has no size list".into());
    }
    if let Some(n) = declared
        && n != table.sizes.len()
    {
        table.notes.push(format!(
            "NUMBER OF SIZES says {n}, the size list has {}; the list was used",
            table.sizes.len()
        ));
    }
    let Some(sample) = table
        .sizes
        .iter()
        .position(|s| s.eq_ignore_ascii_case(&table.sample))
    else {
        return Err(format!(
            "The sample size {} is not in the size list",
            table.sample
        ));
    };
    table.sample = table.sizes[sample].clone();
    let n = table.sizes.len();
    let scale = table.units.metres();
    // Tokens: words and numbers split on colons, commas and white space.
    let tokens: Vec<&str> = body
        .split(|c: char| c == ',' || c == ':' || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .collect();
    let mut i = 0;
    while i < tokens.len() {
        let token = tokens[i].to_uppercase();
        if token == "END" {
            break;
        }
        if token != "RULE" {
            return Err(format!("Expected RULE:, found {}", tokens[i]));
        }
        i += 1;
        let kind = tokens.get(i).ok_or("A rule ends early")?.to_uppercase();
        if kind != "DELTA" {
            return Err(format!("Rule type {kind} is not supported; only DELTA is"));
        }
        let id: i64 = tokens
            .get(i + 1)
            .and_then(|t| t.parse().ok())
            .ok_or("A rule needs a whole-number identifier")?;
        i += 2;
        let mut values = Vec::with_capacity(2 * n);
        for _ in 0..2 * n {
            let t = tokens
                .get(i)
                .ok_or_else(|| format!("Rule {id} has fewer growths than sizes"))?;
            let v: f64 = t
                .parse()
                .map_err(|_| format!("Rule {id}: {t} is not a number"))?;
            if !v.is_finite() {
                return Err(format!("Rule {id}: growths must be finite"));
            }
            values.push(v * scale);
            i += 1;
        }
        let mut growths: Vec<DVec2> = values.chunks(2).map(|p| DVec2::new(p[0], p[1])).collect();
        let at_sample = growths[sample];
        if at_sample != DVec2::ZERO {
            for g in &mut growths {
                *g -= at_sample;
            }
            table.notes.push(format!(
                "Rule {id} grew the sample size; measured from it instead"
            ));
        }
        if table.rules.insert(id, growths).is_some() {
            table
                .notes
                .push(format!("Rule {id} is given twice; the last was kept"));
        }
        if table.rules.len() > MAX_RULES {
            return Err("Too many rules".into());
        }
    }
    Ok(table)
}

/// The rule table beside a DXF file: the same name with `.rul` (any case) instead of `.dxf`.
pub fn beside(dxf: &std::path::Path) -> Option<std::path::PathBuf> {
    let stem = dxf.file_stem()?.to_string_lossy().to_lowercase();
    let dir = dxf.parent()?;
    let dir = if dir.as_os_str().is_empty() {
        std::path::Path::new(".")
    } else {
        dir
    };
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.extension().is_some_and(|e| e.eq_ignore_ascii_case("rul"))
                && p.file_stem()
                    .is_some_and(|s| s.to_string_lossy().to_lowercase() == stem)
        })
}
