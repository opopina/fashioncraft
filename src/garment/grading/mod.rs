//! Grading: one garment in several sizes, drawn once in its base size.
//!
//! The document's panels are the base size. A [`Grading`] adds size groups, grade rules and
//! points of measure (POM); every other size is derived on demand by [`apply`] and is never
//! stored as geometry. A contour point is named by the boundary that starts at it, the same
//! stable name seams, notches and allowances use, so rules survive edits that keep names.
//!
//! - **Grade points** move by a growth (X/Y delta, metres, in the panel's material frame)
//!   per size, measured from the base size, or by an equal step between consecutive sizes.
//! - **Proportional points** have no rule: they move between the grade points before and
//!   after them along the contour, in proportion to the base contour's length.
//! - **Curve rules** move the middle of a curved boundary by an X/Y growth on top of the
//!   proportional movement of its ends.
//! - **Notches** follow their boundary in proportion, or keep their distance from its start
//!   or its end, with an optional extra distance per size.
//! - Holes, internal lines, graphics, trims and pins inside a panel follow the contour by a
//!   mean-value interpolation of its movement; graphics can add an offset and a scale.
//!
//! Clean-room: the behaviour follows public descriptions of industrial grading (grade points
//! with growth deltas from the sample size, other points proportional between them) and of
//! the AAMA/ASTM rule table. Sources are listed in `docs/grading.md`.
use glam::DVec2;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub mod apply;
pub mod dxf;
pub mod pom;
pub mod rul;
#[cfg(test)]
mod tests;

/// Most size groups a garment may hold.
pub const MAX_GROUPS: usize = 16;
/// Most sizes in one group.
pub const MAX_SIZES: usize = 64;
/// Largest growth a rule may give one point in one size, metres.
pub const MAX_DELTA_M: f64 = 1.0;
/// Smallest and largest scale a graphic may take in a size.
pub const GRAPHIC_SCALE_RANGE: std::ops::RangeInclusive<f64> = 0.05..=20.0;

/// One size of a group.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Size {
    /// Stable identifier; rules refer to it, so renaming a size keeps them.
    pub id: String,
    pub name: String,
    /// Left out of the size selector and the 2D review; its rules are kept.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
}

/// Sizes graded together from one base size.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SizeGroup {
    pub id: String,
    pub name: String,
    /// Smallest to largest: an equal step counts sizes in this order.
    pub sizes: Vec<Size>,
    /// The size the panels are drawn in.
    pub base: String,
    /// The size shown in 3D and simulated; absent means the base size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
}

impl SizeGroup {
    pub fn rank(&self, size: &str) -> Option<usize> {
        self.sizes.iter().position(|s| s.id == size)
    }
    pub fn size(&self, id: &str) -> Option<&Size> {
        self.sizes.iter().find(|s| s.id == id)
    }
    /// The size's name, or its id when it has gone.
    pub fn name_of<'a>(&'a self, id: &'a str) -> &'a str {
        self.size(id).map_or(id, |s| s.name.as_str())
    }
    /// How many sizes `size` is above the base (negative below it).
    pub fn steps_from_base(&self, size: &str) -> Option<i64> {
        Some(self.rank(size)? as i64 - self.rank(&self.base)? as i64)
    }
    /// The size shown and simulated.
    pub fn active(&self) -> &str {
        self.active
            .as_deref()
            .filter(|a| self.rank(a).is_some())
            .unwrap_or(&self.base)
    }
    pub fn is_base(&self, size: &str) -> bool {
        size == self.base
    }
    pub fn base_name(&self) -> &str {
        self.name_of(&self.base)
    }
    /// The sizes not hidden, smallest first.
    pub fn visible(&self) -> impl Iterator<Item = &Size> {
        self.sizes.iter().filter(|s| !s.hidden)
    }
    fn fresh_size_id(&self) -> String {
        (1..)
            .map(|n| format!("s{n}"))
            .find(|id| self.rank(id).is_none())
            .expect("unbounded")
    }
}

/// How one point (or one curve's middle) grows in each size.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// Growth from the base size, by size id, metres in the panel's material frame
    /// (X along u, Y along v as the board shows it). Absent sizes do not move.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub deltas: BTreeMap<String, DVec2>,
    /// An equal growth from each size to the next; when set it replaces `deltas`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<DVec2>,
}

impl Rule {
    /// The growth in `size`, from the base size.
    pub fn delta(&self, group: &SizeGroup, size: &str) -> DVec2 {
        if group.is_base(size) {
            return DVec2::ZERO;
        }
        if let Some(step) = self.step {
            return step * group.steps_from_base(size).unwrap_or(0) as f64;
        }
        self.deltas.get(size).copied().unwrap_or(DVec2::ZERO)
    }
    /// The same growths with a step written out size by size.
    pub fn spelled(&self, group: &SizeGroup) -> Rule {
        let deltas = group
            .sizes
            .iter()
            .filter(|s| !group.is_base(&s.id))
            .map(|s| (s.id.clone(), self.delta(group, &s.id)))
            .filter(|(_, d)| *d != DVec2::ZERO)
            .collect();
        Rule { deltas, step: None }
    }
    pub fn is_zero(&self) -> bool {
        self.step.is_none_or(|s| s == DVec2::ZERO)
            && self.deltas.values().all(|d| *d == DVec2::ZERO)
    }
    fn validate(&self) -> Result<(), String> {
        let ok = |d: &DVec2| d.is_finite() && d.abs().max_element() <= MAX_DELTA_M;
        if !self.deltas.values().all(ok) || !self.step.iter().all(ok) {
            return Err(format!(
                "A grade rule needs finite growths of at most {} mm",
                MAX_DELTA_M * 1000.
            ));
        }
        Ok(())
    }
}

/// Where a notch goes in another size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotchMode {
    /// The same share of its boundary as in the base size.
    #[default]
    Proportional,
    /// The same distance from the boundary's start as in the base size.
    FromStart,
    /// The same distance from the boundary's end as in the base size: the direction reversed.
    FromEnd,
}

impl NotchMode {
    pub const ALL: [Self; 3] = [Self::Proportional, Self::FromStart, Self::FromEnd];
    pub fn label(self) -> &'static str {
        match self {
            Self::Proportional => "Proportional",
            Self::FromStart => "Fixed from the start",
            Self::FromEnd => "Fixed distance from the end",
        }
    }
}

/// The grading of one notch, found by its boundary and its place there in the base size.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NotchRule {
    pub edge: String,
    /// The notch's `at` in the base size; a notch moved in the base loses its rule.
    pub at: f64,
    #[serde(default)]
    pub mode: NotchMode,
    /// Extra distance along the boundary per size, metres, measured from the end the mode
    /// keeps (the start for proportional notches): positive moves away from that end.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub deltas: BTreeMap<String, f64>,
}

/// How close a notch's `at` must be to a rule's to take it.
pub const NOTCH_MATCH: f64 = 1e-6;

/// The rules of one panel.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelRules {
    /// Grade point rules, by the name of the boundary that starts at the point.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub points: BTreeMap<String, Rule>,
    /// Points that are not grade points: they move in proportion between their neighbours.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub proportional: BTreeSet<String>,
    /// Curve rules, by boundary name: the growth of the curve's middle.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub curves: BTreeMap<String, Rule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notches: Vec<NotchRule>,
}

impl PanelRules {
    pub fn is_empty(&self) -> bool {
        self.points.values().all(Rule::is_zero)
            && self.proportional.is_empty()
            && self.curves.values().all(Rule::is_zero)
            && self.notches.is_empty()
    }
    pub fn notch(&self, edge: &str, at: f64) -> Option<&NotchRule> {
        self.notches
            .iter()
            .find(|n| n.edge == edge && (n.at - at).abs() <= NOTCH_MATCH)
    }
}

/// A graphic's own grading on top of following its panel.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphicRule {
    /// Extra offset of the centre per size, metres.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub offsets: BTreeMap<String, DVec2>,
    /// Size per size, as a share of the base size; the proportions stay locked.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub scales: BTreeMap<String, f64>,
}

/// What a row of the grade table grows.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Target {
    /// A contour point, by the boundary that starts at it.
    Point { panel: String, edge: String },
    /// The middle of a curved boundary.
    Curve { panel: String, edge: String },
}

impl Target {
    pub fn panel(&self) -> &str {
        match self {
            Self::Point { panel, .. } | Self::Curve { panel, .. } => panel,
        }
    }
    pub fn edge(&self) -> &str {
        match self {
            Self::Point { edge, .. } | Self::Curve { edge, .. } => edge,
        }
    }
}

/// Sizes, rules and points of measure of a garment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grading {
    pub groups: Vec<SizeGroup>,
    /// The group each panel is graded in, by panel id; panels not listed use the first group.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub panel_groups: BTreeMap<String, String>,
    /// Rules by panel id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub panels: BTreeMap<String, PanelRules>,
    /// Graphic rules by graphic placement id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub graphics: BTreeMap<String, GraphicRule>,
    /// Points of measure.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub measurements: Vec<pom::Measurement>,
}

/// Strictly between 0 and 1.
fn open_unit(x: f64) -> bool {
    x > 0. && x < 1.
}

fn name_ok(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty() && name.chars().count() <= 40 && !name.chars().any(char::is_control)
}

/// Value of a size for interpolation: the base is zero, a missing size `zero`.
fn value_at<T: Copy>(map: &BTreeMap<String, T>, group: &SizeGroup, rank: usize, zero: T) -> T {
    let id = &group.sizes[rank].id;
    if group.is_base(id) {
        zero
    } else {
        map.get(id).copied().unwrap_or(zero)
    }
}

/// A value for a size just inserted at `rank`: halfway between its neighbours, or carried on
/// from the two sizes before (or after) it at either end of the list.
fn interpolate<T>(map: &mut BTreeMap<String, T>, group: &SizeGroup, rank: usize, zero: T)
where
    T: Copy + PartialEq + std::ops::Add<Output = T> + std::ops::Sub<Output = T>,
    T: std::ops::Mul<f64, Output = T>,
{
    let n = group.sizes.len();
    let at = |r: usize| value_at(map, group, r, zero);
    let value = match (rank.checked_sub(1), (rank + 1 < n).then_some(rank + 1)) {
        (Some(lo), Some(hi)) => (at(lo) + at(hi)) * 0.5,
        (Some(lo), None) => match lo.checked_sub(1) {
            Some(lo2) => at(lo) + (at(lo) - at(lo2)),
            None => at(lo),
        },
        (None, Some(hi)) if hi + 1 < n => at(hi) + (at(hi) - at(hi + 1)),
        (None, Some(hi)) => at(hi),
        (None, None) => zero,
    };
    if value != zero {
        map.insert(group.sizes[rank].id.clone(), value);
    }
}

impl Grading {
    /// A grading with one group of `sizes`, `base` being the index of the base size.
    pub fn new(group: &str, sizes: &[&str], base: usize) -> Result<Self, String> {
        let mut grading = Self {
            groups: vec![],
            panel_groups: BTreeMap::new(),
            panels: BTreeMap::new(),
            graphics: BTreeMap::new(),
            measurements: vec![],
        };
        grading.add_group(group, sizes, base)?;
        Ok(grading)
    }

    pub fn group(&self, id: &str) -> Option<&SizeGroup> {
        self.groups.iter().find(|g| g.id == id)
    }
    fn group_mut(&mut self, id: &str) -> Result<&mut SizeGroup, String> {
        self.groups
            .iter_mut()
            .find(|g| g.id == id)
            .ok_or_else(|| format!("No size group {id}"))
    }
    /// The group a panel is graded in.
    pub fn group_of(&self, panel: &str) -> Option<&SizeGroup> {
        self.panel_groups
            .get(panel)
            .and_then(|g| self.group(g))
            .or(self.groups.first())
    }
    /// The size a panel is shown in, when it is not its base size.
    pub fn active_size(&self, panel: &str) -> Option<&str> {
        let group = self.group_of(panel)?;
        let active = group.active();
        (!group.is_base(active)).then_some(active)
    }
    /// Whether any group shows a size other than its base.
    pub fn shows_graded_size(&self) -> bool {
        self.groups.iter().any(|g| !g.is_base(g.active()))
    }
    /// The size each group is shown in, by group id.
    pub fn active_sizes(&self) -> BTreeMap<String, String> {
        self.groups
            .iter()
            .map(|g| (g.id.clone(), g.active().to_string()))
            .collect()
    }

    /// Structural checks: names, ids, sizes and finite rules.
    pub fn validate(&self) -> Result<(), String> {
        if self.groups.is_empty() || self.groups.len() > MAX_GROUPS {
            return Err(format!("Grading needs 1 to {MAX_GROUPS} size groups"));
        }
        let mut ids = BTreeSet::new();
        for group in &self.groups {
            if !name_ok(&group.id) || !ids.insert(group.id.as_str()) || !name_ok(&group.name) {
                return Err("Size groups need unique ids and names of 1 to 40 characters".into());
            }
            if group.sizes.is_empty() || group.sizes.len() > MAX_SIZES {
                return Err(format!(
                    "Size group {} needs 1 to {MAX_SIZES} sizes",
                    group.name
                ));
            }
            let mut size_ids = BTreeSet::new();
            let mut names = BTreeSet::new();
            for size in &group.sizes {
                if !name_ok(&size.id)
                    || !size_ids.insert(size.id.as_str())
                    || !name_ok(&size.name)
                    || !names.insert(size.name.trim().to_lowercase())
                {
                    return Err(format!(
                        "Sizes of {} need unique names of 1 to 40 characters",
                        group.name
                    ));
                }
            }
            match group.size(&group.base) {
                Some(base) if !base.hidden => {}
                _ => return Err(format!("Size group {} needs a shown base size", group.name)),
            }
            if group
                .active
                .as_ref()
                .is_some_and(|a| group.rank(a).is_none())
            {
                return Err(format!("Size group {} shows an unknown size", group.name));
            }
        }
        if self.panel_groups.values().any(|g| self.group(g).is_none()) {
            return Err("A panel is graded in an unknown size group".into());
        }
        for rules in self.panels.values() {
            for rule in rules.points.values().chain(rules.curves.values()) {
                rule.validate()?;
            }
            for notch in &rules.notches {
                if !open_unit(notch.at)
                    || !notch
                        .deltas
                        .values()
                        .all(|d| d.is_finite() && d.abs() <= MAX_DELTA_M)
                {
                    return Err(
                        "A notch rule needs a place on its boundary and finite growths".into(),
                    );
                }
            }
        }
        for rule in self.graphics.values() {
            if !rule
                .offsets
                .values()
                .all(|d| d.is_finite() && d.abs().max_element() <= MAX_DELTA_M)
                || !rule
                    .scales
                    .values()
                    .all(|s| GRAPHIC_SCALE_RANGE.contains(s))
            {
                return Err(
                    "A graphic rule needs finite offsets and scales of 5 % to 2000 %".into(),
                );
            }
        }
        let mut pom_ids = BTreeSet::new();
        for m in &self.measurements {
            if !pom_ids.insert(m.id.as_str()) {
                return Err("Points of measure need unique ids".into());
            }
            m.validate()?;
        }
        Ok(())
    }

    /// Forget rules of panels, boundaries, graphics and sizes that no longer exist.
    pub fn sanitize(&mut self, template: &crate::garment::GarmentTemplate) {
        let groups: BTreeSet<String> = self.groups.iter().map(|g| g.id.clone()).collect();
        self.panel_groups
            .retain(|p, g| groups.contains(g) && template.panels.iter().any(|q| &q.id == p));
        let sizes: BTreeSet<String> = self
            .groups
            .iter()
            .flat_map(|g| g.sizes.iter().map(|s| s.id.clone()))
            .collect();
        let keep_sizes = |m: &mut BTreeMap<String, DVec2>| m.retain(|s, _| sizes.contains(s));
        self.panels.retain(|id, rules| {
            let Some(polygon) = template
                .panels
                .iter()
                .find(|p| &p.id == id)
                .and_then(|p| p.polygon.as_ref())
            else {
                return false;
            };
            let has = |e: &String| polygon.edges.contains(e);
            rules.points.retain(|e, _| has(e));
            rules.proportional.retain(has);
            rules
                .curves
                .retain(|e, _| polygon.curves.contains_key(e) && has(e));
            for rule in rules.points.values_mut().chain(rules.curves.values_mut()) {
                keep_sizes(&mut rule.deltas);
            }
            rules.notches.retain(|n| {
                polygon
                    .notches
                    .get(&n.edge)
                    .is_some_and(|list| list.iter().any(|m| (m.at - n.at).abs() <= NOTCH_MATCH))
            });
            for notch in &mut rules.notches {
                notch.deltas.retain(|s, _| sizes.contains(s));
            }
            !rules.is_empty()
        });
        self.graphics.retain(|id, rule| {
            rule.offsets.retain(|s, _| sizes.contains(s));
            rule.scales.retain(|s, _| sizes.contains(s));
            template.graphics.iter().any(|g| &g.id == id)
                && (!rule.offsets.is_empty() || !rule.scales.is_empty())
        });
    }

    // --- Sizes -------------------------------------------------------------------------

    /// Add a group of `sizes` (smallest first) with `sizes[base]` as base; returns its id.
    pub fn add_group(&mut self, name: &str, sizes: &[&str], base: usize) -> Result<String, String> {
        if self.groups.len() >= MAX_GROUPS {
            return Err(format!("At most {MAX_GROUPS} size groups"));
        }
        if base >= sizes.len() {
            return Err("The base size must be one of the sizes".into());
        }
        let id = (1..)
            .map(|n| format!("g{n}"))
            .find(|id| self.group(id).is_none())
            .expect("unbounded");
        let sizes: Vec<Size> = sizes
            .iter()
            .enumerate()
            .map(|(i, s)| Size {
                id: format!("s{}", i + 1),
                name: s.trim().to_string(),
                hidden: false,
            })
            .collect();
        self.groups.push(SizeGroup {
            id: id.clone(),
            name: name.trim().to_string(),
            base: sizes[base].id.clone(),
            sizes,
            active: None,
        });
        if let Err(e) = self.validate() {
            self.groups.pop();
            return Err(e);
        }
        Ok(id)
    }
    pub fn rename_group(&mut self, group: &str, name: &str) -> Result<(), String> {
        if !name_ok(name) {
            return Err("A size group needs a name of 1 to 40 characters".into());
        }
        self.group_mut(group)?.name = name.trim().into();
        Ok(())
    }
    /// Remove a group; its panels move to the first remaining group. The last group cannot go.
    pub fn remove_group(&mut self, group: &str) -> Result<(), String> {
        if self.groups.len() <= 1 {
            return Err("The last size group cannot be removed".into());
        }
        self.group_mut(group)?;
        self.groups.retain(|g| g.id != group);
        self.panel_groups.retain(|_, g| g != group);
        Ok(())
    }
    /// Grade `panels` in `group`.
    pub fn assign(&mut self, panels: &[String], group: &str) -> Result<(), String> {
        self.group_mut(group)?;
        let first = self.groups[0].id.clone();
        for panel in panels {
            if group == first {
                self.panel_groups.remove(panel);
            } else {
                self.panel_groups.insert(panel.clone(), group.into());
            }
        }
        Ok(())
    }

    /// Add a size named `name` after size `after` (between it and the next), or as the
    /// largest size. Its growths are interpolated from its neighbours. Returns its id.
    pub fn add_size(
        &mut self,
        group: &str,
        name: &str,
        after: Option<&str>,
    ) -> Result<String, String> {
        let g = self
            .group(group)
            .ok_or_else(|| format!("No size group {group}"))?;
        let rank = match after {
            Some(a) => g.rank(a).ok_or("No such size")? + 1,
            None => g.sizes.len(),
        };
        self.insert_size(group, name, rank)
    }
    /// Add a size at `rank` in the size order (0: a new smallest size). Its growths are
    /// interpolated from its neighbours, or carried on past the ends. Returns its id.
    pub fn insert_size(&mut self, group: &str, name: &str, rank: usize) -> Result<String, String> {
        let g = self.group_mut(group)?;
        if g.sizes.len() >= MAX_SIZES {
            return Err(format!("At most {MAX_SIZES} sizes in a group"));
        }
        if rank > g.sizes.len() {
            return Err("No such place in the size list".into());
        }
        let old = g.clone();
        let id = g.fresh_size_id();
        g.sizes.insert(
            rank,
            Size {
                id: id.clone(),
                name: name.trim().into(),
                hidden: false,
            },
        );
        if let Err(e) = self.validate() {
            self.group_mut(group)?.sizes.remove(rank);
            return Err(e);
        }
        let g = self.group(group).expect("checked").clone();
        let in_group = |panel: &str, grading: &Grading| {
            grading.group_of(panel).is_some_and(|pg| pg.id == g.id)
        };
        let panels: Vec<String> = self
            .panels
            .keys()
            .filter(|p| in_group(p, self))
            .cloned()
            .collect();
        let end = rank == 0 || rank + 1 == g.sizes.len();
        for panel in panels {
            let rules = self.panels.get_mut(&panel).expect("listed");
            for rule in rules.points.values_mut().chain(rules.curves.values_mut()) {
                // An equal step carries on past either end; a size put in between keeps
                // the others where they were and takes the middle.
                if rule.step.is_some() && !end {
                    *rule = rule.spelled(&old);
                }
                if rule.step.is_none() {
                    interpolate(&mut rule.deltas, &g, rank, DVec2::ZERO);
                }
            }
            for notch in &mut rules.notches {
                interpolate(&mut notch.deltas, &g, rank, 0.);
            }
        }
        for rule in self.graphics.values_mut() {
            interpolate(&mut rule.offsets, &g, rank, DVec2::ZERO);
            let mut shifted: BTreeMap<String, f64> = rule
                .scales
                .iter()
                .map(|(k, v)| (k.clone(), v - 1.))
                .collect();
            interpolate(&mut shifted, &g, rank, 0.);
            if let Some(v) = shifted.get(&id) {
                rule.scales.insert(id.clone(), (v + 1.).clamp(0.05, 20.));
            }
        }
        Ok(id)
    }
    pub fn rename_size(&mut self, group: &str, size: &str, name: &str) -> Result<(), String> {
        let g = self.group_mut(group)?;
        let entry = g
            .sizes
            .iter_mut()
            .find(|s| s.id == size)
            .ok_or("No such size")?;
        let old = std::mem::replace(&mut entry.name, name.trim().into());
        if let Err(e) = self.validate() {
            let g = self.group_mut(group)?;
            g.sizes
                .iter_mut()
                .find(|s| s.id == size)
                .expect("found")
                .name = old;
            return Err(e);
        }
        Ok(())
    }
    /// Hide or show a size. The base size and the size on show cannot be hidden.
    pub fn set_hidden(&mut self, group: &str, size: &str, hidden: bool) -> Result<(), String> {
        let g = self.group_mut(group)?;
        if hidden && (g.is_base(size) || g.active() == size) {
            return Err("The base size and the size on show cannot be hidden".into());
        }
        g.sizes
            .iter_mut()
            .find(|s| s.id == size)
            .ok_or("No such size")?
            .hidden = hidden;
        Ok(())
    }
    /// Remove a size and its growths. The base size cannot be removed.
    pub fn remove_size(&mut self, group: &str, size: &str) -> Result<(), String> {
        let g = self.group_mut(group)?;
        if g.is_base(size) {
            return Err("The base size cannot be removed".into());
        }
        let rank = g.rank(size).ok_or("No such size")?;
        g.sizes.remove(rank);
        if g.active.as_deref() == Some(size) {
            g.active = None;
        }
        for rules in self.panels.values_mut() {
            for rule in rules.points.values_mut().chain(rules.curves.values_mut()) {
                rule.deltas.remove(size);
            }
            for notch in &mut rules.notches {
                notch.deltas.remove(size);
            }
        }
        for rule in self.graphics.values_mut() {
            rule.offsets.remove(size);
            rule.scales.remove(size);
        }
        Ok(())
    }
    /// Show and simulate `size`.
    pub fn set_active(&mut self, group: &str, size: &str) -> Result<(), String> {
        let g = self.group_mut(group)?;
        let entry = g.size(size).ok_or("No such size")?;
        if entry.hidden {
            return Err("Show the size before choosing it".into());
        }
        g.active = (!g.is_base(size)).then(|| size.to_string());
        Ok(())
    }

    // --- Rules -------------------------------------------------------------------------

    pub fn rule(&self, target: &Target) -> Option<&Rule> {
        let rules = self.panels.get(target.panel())?;
        match target {
            Target::Point { edge, .. } => rules.points.get(edge),
            Target::Curve { edge, .. } => rules.curves.get(edge),
        }
    }
    fn rule_mut(&mut self, target: &Target) -> &mut Rule {
        let rules = self.panels.entry(target.panel().to_string()).or_default();
        match target {
            Target::Point { edge, .. } => rules.points.entry(edge.clone()).or_default(),
            Target::Curve { edge, .. } => rules.curves.entry(edge.clone()).or_default(),
        }
    }
    fn tidy(&mut self, panel: &str) {
        if let Some(rules) = self.panels.get_mut(panel) {
            rules.points.retain(|_, r| !r.is_zero());
            rules.curves.retain(|_, r| !r.is_zero());
            if rules.is_empty() {
                self.panels.remove(panel);
            }
        }
    }
    fn panel_group(&self, panel: &str) -> Result<SizeGroup, String> {
        self.group_of(panel)
            .cloned()
            .ok_or_else(|| "No size group".into())
    }
    /// Set the growth of `target` in `size`; a stepped rule is written out size by size first.
    pub fn set_delta(&mut self, target: &Target, size: &str, delta: DVec2) -> Result<(), String> {
        let group = self.panel_group(target.panel())?;
        if group.rank(size).is_none() {
            return Err("No such size in the panel's group".into());
        }
        if group.is_base(size) {
            return Err("The base size does not grow: change the pattern itself".into());
        }
        if let Target::Point { panel, edge } = target {
            self.set_grade_point(panel, edge, true);
        }
        let rule = self.rule_mut(target);
        if rule.step.is_some() {
            *rule = rule.spelled(&group);
        }
        if delta == DVec2::ZERO {
            rule.deltas.remove(size);
        } else {
            rule.deltas.insert(size.into(), delta);
        }
        rule.validate()?;
        self.tidy(target.panel());
        Ok(())
    }
    /// Grow `target` by an equal `step` between consecutive sizes.
    pub fn set_step(&mut self, target: &Target, step: DVec2) -> Result<(), String> {
        if let Target::Point { panel, edge } = target {
            self.set_grade_point(panel, edge, true);
        }
        let rule = self.rule_mut(target);
        *rule = Rule {
            deltas: BTreeMap::new(),
            step: (step != DVec2::ZERO).then_some(step),
        };
        rule.validate()?;
        self.tidy(target.panel());
        Ok(())
    }
    /// Replace the whole rule of `target`.
    pub fn set_rule(&mut self, target: &Target, rule: Rule) -> Result<(), String> {
        rule.validate()?;
        if let Target::Point { panel, edge } = target {
            self.set_grade_point(panel, edge, true);
        }
        *self.rule_mut(target) = rule;
        self.tidy(target.panel());
        Ok(())
    }
    /// Make a contour point a grade point (keeping any rule) or a proportional point (its
    /// rule dropped: it follows its neighbours).
    pub fn set_grade_point(&mut self, panel: &str, edge: &str, grade: bool) {
        let rules = self.panels.entry(panel.to_string()).or_default();
        if grade {
            rules.proportional.remove(edge);
        } else {
            rules.points.remove(edge);
            rules.proportional.insert(edge.to_string());
        }
        self.tidy(panel);
    }
    pub fn is_grade_point(&self, panel: &str, edge: &str) -> bool {
        self.panels
            .get(panel)
            .is_none_or(|r| !r.proportional.contains(edge))
    }
    /// Set (or, with the default rule, clear) the grading of the notch at `at` on `edge`.
    pub fn set_notch_rule(&mut self, panel: &str, rule: NotchRule) -> Result<(), String> {
        if !open_unit(rule.at) {
            return Err("A notch rule needs the notch's place on its boundary".into());
        }
        let rules = self.panels.entry(panel.to_string()).or_default();
        rules
            .notches
            .retain(|n| !(n.edge == rule.edge && (n.at - rule.at).abs() <= NOTCH_MATCH));
        if rule.mode != NotchMode::Proportional || rule.deltas.values().any(|d| *d != 0.) {
            rules.notches.push(rule);
            rules
                .notches
                .sort_by(|a, b| a.edge.cmp(&b.edge).then(a.at.total_cmp(&b.at)));
        }
        self.tidy(panel);
        self.validate()
    }
    pub fn set_graphic_rule(&mut self, graphic: &str, rule: GraphicRule) -> Result<(), String> {
        if rule.offsets.is_empty() && rule.scales.is_empty() {
            self.graphics.remove(graphic);
        } else {
            self.graphics.insert(graphic.into(), rule);
        }
        self.validate()
    }
}

/// Evaluate a millimetre field: a number, or a sum such as `10+2`, `12.5 - 3*2` or `(4+2)/2`.
/// Commas count as decimal points.
pub fn eval_mm(text: &str) -> Option<f64> {
    struct P<'a> {
        s: &'a [u8],
        i: usize,
    }
    impl P<'_> {
        fn ws(&mut self) {
            while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
                self.i += 1;
            }
        }
        fn eat(&mut self, c: u8) -> bool {
            self.ws();
            if self.s.get(self.i) == Some(&c) {
                self.i += 1;
                true
            } else {
                false
            }
        }
        fn sum(&mut self) -> Option<f64> {
            let mut v = self.product()?;
            loop {
                if self.eat(b'+') {
                    v += self.product()?;
                } else if self.eat(b'-') {
                    v -= self.product()?;
                } else {
                    return Some(v);
                }
            }
        }
        fn product(&mut self) -> Option<f64> {
            let mut v = self.unary()?;
            loop {
                if self.eat(b'*') {
                    v *= self.unary()?;
                } else if self.eat(b'/') {
                    let d = self.unary()?;
                    if d == 0. {
                        return None;
                    }
                    v /= d;
                } else {
                    return Some(v);
                }
            }
        }
        fn unary(&mut self) -> Option<f64> {
            if self.eat(b'-') {
                return Some(-self.unary()?);
            }
            if self.eat(b'+') {
                return self.unary();
            }
            if self.eat(b'(') {
                let v = self.sum()?;
                return self.eat(b')').then_some(v);
            }
            self.ws();
            let start = self.i;
            while self.i < self.s.len()
                && (self.s[self.i].is_ascii_digit() || self.s[self.i] == b'.')
            {
                self.i += 1;
            }
            std::str::from_utf8(&self.s[start..self.i])
                .ok()?
                .parse()
                .ok()
        }
    }
    let text = text.replace(',', ".");
    let mut p = P {
        s: text.as_bytes(),
        i: 0,
    };
    let v = p.sum()?;
    p.ws();
    (p.i == p.s.len() && v.is_finite()).then_some(v)
}
