//! Points of measure: named pattern measurements kept with the garment and read in every
//! size, with their difference from the base size and an allowed tolerance.
use super::{Grading, apply};
use crate::garment::{GarmentTemplate, markings::polyline_length, polygon::PolygonPattern};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Most boundaries one length measurement may add up.
pub const MAX_SEGMENTS: usize = 64;

/// One boundary of one panel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Segment {
    pub panel: String,
    pub edge: String,
}

/// What a point of measure measures.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum MeasureKind {
    /// Straight distance between two contour points of one panel, each named by the
    /// boundary that starts at it.
    Distance {
        panel: String,
        from: String,
        to: String,
    },
    /// Length along one or more boundaries, added up; they may belong to different panels
    /// (the gap between panels is not counted).
    Length { segments: Vec<Segment> },
}

/// A named measurement.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Measurement {
    pub id: String,
    pub name: String,
    pub kind: MeasureKind,
    /// Allowed deviation either way, metres.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub tolerance_m: f64,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.
}

impl Measurement {
    pub fn validate(&self) -> Result<(), String> {
        if !super::name_ok(&self.name) {
            return Err("A point of measure needs a name of 1 to 40 characters".into());
        }
        if !self.tolerance_m.is_finite() || !(0.0..=1.0).contains(&self.tolerance_m) {
            return Err("A point of measure needs a tolerance from 0 to 1000 mm".into());
        }
        match &self.kind {
            MeasureKind::Distance { from, to, .. } if from == to => {
                Err("A distance needs two different points".into())
            }
            MeasureKind::Length { segments }
                if segments.is_empty() || segments.len() > MAX_SEGMENTS =>
            {
                Err(format!("A length adds 1 to {MAX_SEGMENTS} boundaries"))
            }
            _ => Ok(()),
        }
    }
    /// The panels it reads, the first deciding its size group.
    pub fn panels(&self) -> Vec<&str> {
        match &self.kind {
            MeasureKind::Distance { panel, .. } => vec![panel.as_str()],
            MeasureKind::Length { segments } => {
                let mut out: Vec<&str> = vec![];
                for s in segments {
                    if !out.contains(&s.panel.as_str()) {
                        out.push(&s.panel);
                    }
                }
                out
            }
        }
    }
}

/// The polygons of `panels` in `size` of group `group` (the base for panels of other groups).
fn polygons(
    template: &GarmentTemplate,
    panels: &[&str],
    group: Option<&str>,
    size: &str,
) -> Result<BTreeMap<String, PolygonPattern>, String> {
    let grading = template.grading.as_ref();
    panels
        .iter()
        .map(|id| {
            let same = grading
                .and_then(|g| g.group_of(id))
                .is_some_and(|g| Some(g.id.as_str()) == group);
            let size = if same { size } else { "" };
            let graded = apply::panel_in_size(template, id, size)?
                .ok_or_else(|| format!("Panel {id} has no outline to measure"))?;
            Ok((id.to_string(), graded.polygon))
        })
        .collect()
}

/// The value of `m` in `size` of the group of its first panel, metres.
pub fn measure(template: &GarmentTemplate, m: &Measurement, size: &str) -> Result<f64, String> {
    let panels = m.panels();
    let group = template
        .grading
        .as_ref()
        .zip(panels.first())
        .and_then(|(g, p)| g.group_of(p))
        .map(|g| g.id.as_str());
    let polys = polygons(template, &panels, group, size)?;
    match &m.kind {
        MeasureKind::Distance { panel, from, to } => {
            let poly = &polys[panel];
            let point = |edge: &str| {
                poly.edges
                    .iter()
                    .position(|e| e == edge)
                    .map(|i| poly.points[i])
                    .ok_or_else(|| format!("The point at {edge} is gone"))
            };
            Ok(point(from)?.distance(point(to)?))
        }
        MeasureKind::Length { segments } => segments
            .iter()
            .map(|s| {
                polys[&s.panel]
                    .named_boundary(&s.edge)
                    .map(|b| polyline_length(&b))
                    .ok_or_else(|| format!("The boundary {} is gone", s.edge))
            })
            .sum(),
    }
}

/// One row of the measurement table: the value in each size of its group, smallest first.
#[derive(Clone, Debug)]
pub struct Row {
    pub id: String,
    pub name: String,
    pub group: String,
    pub tolerance_m: f64,
    /// (size id, value in metres or why it cannot be read).
    pub values: Vec<(String, Result<f64, String>)>,
    pub base: Result<f64, String>,
}

impl Row {
    /// The value in `size` less the base value.
    pub fn difference(&self, size: &str) -> Option<f64> {
        let base = self.base.as_ref().ok()?;
        let (_, v) = self.values.iter().find(|(s, _)| s == size)?;
        Some(v.as_ref().ok()? - base)
    }
}

/// Every point of measure in every size of its group.
pub fn table(template: &GarmentTemplate) -> Vec<Row> {
    let Some(grading) = &template.grading else {
        return vec![];
    };
    grading
        .measurements
        .iter()
        .filter_map(|m| {
            let group = grading.group_of(m.panels().first()?)?;
            let values: Vec<_> = group
                .sizes
                .iter()
                .map(|s| (s.id.clone(), measure(template, m, &s.id)))
                .collect();
            let base = values
                .iter()
                .find(|(s, _)| group.is_base(s))
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| Err("No base size".into()));
            Some(Row {
                id: m.id.clone(),
                name: m.name.clone(),
                group: group.id.clone(),
                tolerance_m: m.tolerance_m,
                values,
                base,
            })
        })
        .collect()
}

impl Grading {
    /// Add a point of measure; returns its id.
    pub fn add_measurement(
        &mut self,
        name: &str,
        kind: MeasureKind,
        tolerance_m: f64,
    ) -> Result<String, String> {
        let id = (1..)
            .map(|n| format!("pom{n}"))
            .find(|id| !self.measurements.iter().any(|m| &m.id == id))
            .expect("unbounded");
        let m = Measurement {
            id: id.clone(),
            name: name.trim().into(),
            kind,
            tolerance_m,
        };
        m.validate()?;
        self.measurements.push(m);
        Ok(id)
    }
    pub fn remove_measurement(&mut self, id: &str) -> Result<(), String> {
        let before = self.measurements.len();
        self.measurements.retain(|m| m.id != id);
        if self.measurements.len() == before {
            return Err("No such point of measure".into());
        }
        Ok(())
    }
    pub fn measurement_mut(&mut self, id: &str) -> Option<&mut Measurement> {
        self.measurements.iter_mut().find(|m| m.id == id)
    }
}

/// The table as comma-separated values, millimetres: name, then each size, the tolerance.
pub fn csv(template: &GarmentTemplate) -> String {
    let Some(grading) = &template.grading else {
        return String::new();
    };
    let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let mut out = String::new();
    let mut header_for: Option<String> = None;
    for row in table(template) {
        let Some(group) = grading.group(&row.group) else {
            continue;
        };
        if header_for.as_deref() != Some(&row.group) {
            let sizes: Vec<String> = group
                .sizes
                .iter()
                .map(|s| {
                    let base = if group.is_base(&s.id) { " (base)" } else { "" };
                    quote(&format!("{}{base}", s.name))
                })
                .collect();
            out.push_str(&format!(
                "{},{},{}\n",
                quote(&format!("POM · {}", group.name)),
                sizes.join(","),
                quote("Tolerance ±")
            ));
            header_for = Some(row.group.clone());
        }
        let values: Vec<String> = row
            .values
            .iter()
            .map(|(_, v)| {
                v.as_ref()
                    .map_or(String::new(), |v| format!("{:.1}", v * 1000.))
            })
            .collect();
        out.push_str(&format!(
            "{},{},{:.1}\n",
            quote(&row.name),
            values.join(","),
            row.tolerance_m * 1000.
        ));
    }
    out
}

/// Shorthand for a distance between two contour points.
pub fn distance(panel: &str, from: &str, to: &str) -> MeasureKind {
    MeasureKind::Distance {
        panel: panel.into(),
        from: from.into(),
        to: to.into(),
    }
}

/// Shorthand for the length of boundaries.
pub fn length(segments: &[(&str, &str)]) -> MeasureKind {
    MeasureKind::Length {
        segments: segments
            .iter()
            .map(|(p, e)| Segment {
                panel: (*p).into(),
                edge: (*e).into(),
            })
            .collect(),
    }
}
