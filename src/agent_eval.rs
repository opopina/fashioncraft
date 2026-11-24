//! Evaluation of the Assistant: typical requests, each with checks on what the model did (the
//! tools it called), on the garment afterwards (scene_summary before and after) and on its
//! reply. The cases live in `evals/assistant/cases.json`; the window runs them against the
//! configured model (`FASHIONCRAFT_ASSISTANT_EVAL`, see docs/agents.md) and this module
//! scores each run.
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The cases file.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Suite {
    pub cases: Vec<Case>,
}

/// One request and what a good answer to it does.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Case {
    pub id: String,
    /// The garment it starts from: a draft_garment kind drafted on the avatar, or "current".
    #[serde(default = "default_start")]
    pub start: String,
    pub prompt: String,
    pub checks: Vec<Check>,
}
fn default_start() -> String {
    "tshirt".into()
}

/// One thing a good answer does.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Check {
    /// A call to `tool` whose arguments include `with` (strings compared without case, a
    /// string expected inside a longer one counts).
    Called {
        tool: String,
        #[serde(default)]
        with: Option<Value>,
    },
    /// Any of these tools was called.
    CalledAny {
        tools: Vec<String>,
    },
    NotCalled {
        tool: String,
    },
    /// How many values `path` reaches in the summary afterwards (an array counts its items).
    Count {
        path: String,
        #[serde(default)]
        min: Option<f64>,
        #[serde(default)]
        max: Option<f64>,
    },
    /// Some value `path` reaches afterwards matches.
    Any {
        path: String,
        #[serde(flatten)]
        test: Test,
    },
    /// The first number (or array length) at `path` moved by this much from before to after.
    Changed {
        path: String,
        #[serde(default)]
        min: Option<f64>,
        #[serde(default)]
        max: Option<f64>,
    },
    /// The reply contains one of these, ignoring case.
    ReplyContains {
        any: Vec<String>,
    },
    /// The reply is in this language ("es" or "en").
    Language {
        lang: String,
    },
    /// At most this many tool calls failed.
    MaxErrors {
        count: usize,
    },
    /// At most this many tool calls in all.
    MaxCalls {
        count: usize,
    },
}

/// A test of one value.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Test {
    #[serde(default)]
    pub equals: Option<Value>,
    #[serde(default)]
    pub contains: Option<String>,
    #[serde(default)]
    pub below: Option<f64>,
    #[serde(default)]
    pub above: Option<f64>,
}

impl Test {
    fn passes(&self, v: &Value) -> bool {
        self.equals.as_ref().is_none_or(|e| loosely_equal(e, v))
            && self.contains.as_ref().is_none_or(|c| {
                v.as_str()
                    .is_some_and(|s| s.to_lowercase().contains(&c.to_lowercase()))
            })
            && self.below.is_none_or(|b| v.as_f64().is_some_and(|x| x < b))
            && self.above.is_none_or(|a| v.as_f64().is_some_and(|x| x > a))
    }
}

/// One tool call of a run.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Call {
    pub name: String,
    pub arguments: Value,
    pub is_error: bool,
}

/// What the model did in one case.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Transcript {
    pub calls: Vec<Call>,
    /// Everything the model wrote, joined.
    pub reply: String,
    /// A failure of the turn itself (no key, the model could not be reached…).
    pub failure: Option<String>,
}

/// A check's result.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Outcome {
    pub check: String,
    pub passed: bool,
    pub detail: String,
}

/// Every value `path` reaches: `/pieces/*/name` walks every piece; `*` also walks an object's
/// values.
pub fn values_at<'a>(root: &'a Value, path: &str) -> Vec<&'a Value> {
    let mut at = vec![root];
    for part in path.split('/').filter(|p| !p.is_empty()) {
        at = at
            .into_iter()
            .flat_map(|v| -> Vec<&Value> {
                match (part, v) {
                    ("*", Value::Array(items)) => items.iter().collect(),
                    ("*", Value::Object(map)) => map.values().collect(),
                    (key, Value::Array(items)) => key
                        .parse::<usize>()
                        .ok()
                        .and_then(|i| items.get(i))
                        .into_iter()
                        .collect(),
                    (key, Value::Object(map)) => map.get(key).into_iter().collect(),
                    _ => vec![],
                }
            })
            .collect();
    }
    at
}

fn count_at(root: &Value, path: &str) -> usize {
    let found = values_at(root, path);
    match found.as_slice() {
        [Value::Array(items)] => items.len(),
        _ => found.len(),
    }
}

fn number_at(root: &Value, path: &str) -> Option<f64> {
    match values_at(root, path).first()? {
        Value::Array(items) => Some(items.len() as f64),
        v => v.as_f64(),
    }
}

/// Equal, with strings compared without case and numbers as numbers.
fn loosely_equal(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::String(a), Value::String(b)) => a.eq_ignore_ascii_case(b),
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        _ => expected == actual,
    }
}

/// Whether `arguments` include everything in `with`: object fields recursively, a string
/// inside a longer one, an array item anywhere in an array.
fn includes(arguments: &Value, with: &Value) -> bool {
    match (with, arguments) {
        (Value::Object(want), Value::Object(have)) => want
            .iter()
            .all(|(k, w)| have.get(k).is_some_and(|h| includes(h, w))),
        (Value::String(w), Value::String(h)) => h.to_lowercase().contains(&w.to_lowercase()),
        (w, Value::Array(items)) if !w.is_array() => items.iter().any(|h| includes(h, w)),
        (w, h) => loosely_equal(w, h),
    }
}

const SPANISH: [&str; 14] = [
    " el ", " la ", " de ", " que ", " los ", " las ", " una ", " para ", " con ", " del ",
    " está", " cm de", "ñ", "ó",
];
const ENGLISH: [&str; 12] = [
    " the ", " and ", " of ", " is ", " to ", " with ", " for ", " it ", " this ", " was ",
    " your ", " you ",
];

/// "es" or "en", whichever's common words the text has more of.
pub fn language(text: &str) -> &'static str {
    let t = format!(" {} ", text.to_lowercase());
    let score = |words: &[&str]| -> usize { words.iter().map(|w| t.matches(w).count()).sum() };
    if score(&SPANISH) > score(&ENGLISH) {
        "es"
    } else {
        "en"
    }
}

/// Scores one run: `before` and `after` are scene_summary's output around it.
pub fn evaluate(case: &Case, before: &Value, after: &Value, run: &Transcript) -> Vec<Outcome> {
    let mut out = vec![];
    if let Some(failure) = &run.failure {
        out.push(Outcome {
            check: "the turn ran".into(),
            passed: false,
            detail: failure.clone(),
        });
    }
    let names: Vec<&str> = run.calls.iter().map(|c| c.name.as_str()).collect();
    for check in &case.checks {
        let (label, passed, detail) = match check {
            Check::Called { tool, with } => {
                let hit = run.calls.iter().any(|c| {
                    c.name == *tool && with.as_ref().is_none_or(|w| includes(&c.arguments, w))
                });
                (
                    format!(
                        "called {tool}{}",
                        with.as_ref()
                            .map(|w| format!(" with {w}"))
                            .unwrap_or_default()
                    ),
                    hit,
                    format!("calls: {}", names.join(", ")),
                )
            }
            Check::CalledAny { tools } => (
                format!("called one of {}", tools.join(", ")),
                tools.iter().any(|t| names.contains(&t.as_str())),
                format!("calls: {}", names.join(", ")),
            ),
            Check::NotCalled { tool } => (
                format!("did not call {tool}"),
                !names.contains(&tool.as_str()),
                format!("calls: {}", names.join(", ")),
            ),
            Check::Count { path, min, max } => {
                let n = count_at(after, path) as f64;
                (
                    format!("count {path} in [{min:?}, {max:?}]"),
                    min.is_none_or(|m| n >= m) && max.is_none_or(|m| n <= m),
                    format!("{n}"),
                )
            }
            Check::Any { path, test } => {
                let found = values_at(after, path);
                (
                    format!("some {path} matches {test:?}"),
                    found.iter().any(|v| test.passes(v)),
                    format!(
                        "{}",
                        Value::Array(found.into_iter().take(8).cloned().collect())
                    ),
                )
            }
            Check::Changed { path, min, max } => {
                match (number_at(before, path), number_at(after, path)) {
                    (Some(a), Some(b)) => {
                        let d = b - a;
                        (
                            format!("{path} changed by [{min:?}, {max:?}]"),
                            min.is_none_or(|m| d >= m) && max.is_none_or(|m| d <= m),
                            format!("{a} → {b} ({d:+})"),
                        )
                    }
                    _ => (
                        format!("{path} changed"),
                        false,
                        "no number there before or after".into(),
                    ),
                }
            }
            Check::ReplyContains { any } => {
                let reply = run.reply.to_lowercase();
                (
                    format!("reply mentions one of {any:?}"),
                    any.iter().any(|w| reply.contains(&w.to_lowercase())),
                    run.reply.chars().take(160).collect(),
                )
            }
            Check::Language { lang } => {
                let found = language(&run.reply);
                (
                    format!("reply in {lang}"),
                    !run.reply.trim().is_empty() && found == lang,
                    format!("looks {found}"),
                )
            }
            Check::MaxErrors { count } => {
                let errors = run.calls.iter().filter(|c| c.is_error).count();
                (
                    format!("at most {count} failed calls"),
                    errors <= *count,
                    format!("{errors}"),
                )
            }
            Check::MaxCalls { count } => (
                format!("at most {count} calls"),
                run.calls.len() <= *count,
                format!("{}", run.calls.len()),
            ),
        };
        out.push(Outcome {
            check: label,
            passed,
            detail,
        });
    }
    out
}

/// One case's result, for the report.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CaseResult {
    pub id: String,
    pub prompt: String,
    pub outcomes: Vec<Outcome>,
    pub transcript: Transcript,
    pub seconds: f64,
}

impl CaseResult {
    pub fn passed(&self) -> bool {
        self.outcomes.iter().all(|o| o.passed)
    }
}

/// A short Markdown report: a line per case, then every failed check.
pub fn report(results: &[CaseResult], model: &str) -> String {
    let passed = results.iter().filter(|r| r.passed()).count();
    let checks: usize = results.iter().map(|r| r.outcomes.len()).sum();
    let checks_passed: usize = results
        .iter()
        .map(|r| r.outcomes.iter().filter(|o| o.passed).count())
        .sum();
    let mut out = format!(
        "# Assistant evaluation\n\nModel: {model}\n\n{passed}/{} cases passed, {checks_passed}/{checks} checks.\n\n| case | result | calls | errors | s |\n|---|---|---|---|---|\n",
        results.len()
    );
    for r in results {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {:.0} |\n",
            r.id,
            if r.passed() { "pass" } else { "FAIL" },
            r.transcript.calls.len(),
            r.transcript.calls.iter().filter(|c| c.is_error).count(),
            r.seconds
        ));
    }
    for r in results.iter().filter(|r| !r.passed()) {
        out.push_str(&format!("\n## {}\n\n> {}\n\n", r.id, r.prompt));
        for o in r.outcomes.iter().filter(|o| !o.passed) {
            out.push_str(&format!("- {}: {}\n", o.check, o.detail));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn suite() -> Suite {
        serde_json::from_str(include_str!("../evals/assistant/cases.json")).unwrap()
    }

    #[test]
    fn the_bundled_cases_read_and_cover_both_languages() {
        let s = suite();
        assert!(s.cases.len() >= 20, "{}", s.cases.len());
        let mut ids: Vec<&str> = s.cases.iter().map(|c| c.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), s.cases.len(), "case ids are unique");
        let langs: Vec<&str> = s.cases.iter().map(|c| language(&c.prompt)).collect();
        assert!(langs.iter().filter(|l| **l == "es").count() >= 8);
        assert!(langs.iter().filter(|l| **l == "en").count() >= 8);
        let tools: Vec<&str> = crate::agent::tools().iter().map(|t| t.name).collect();
        for case in &s.cases {
            assert!(!case.checks.is_empty(), "{}", case.id);
            assert!(
                case.start == "current"
                    || crate::agent::GARMENT_KINDS.contains(&case.start.as_str()),
                "{}",
                case.id
            );
            for check in &case.checks {
                if let Check::Called { tool, .. } | Check::NotCalled { tool } = check {
                    assert!(tools.contains(&tool.as_str()), "{}: {tool}", case.id);
                }
                if let Check::CalledAny { tools: any } = check {
                    for t in any {
                        assert!(tools.contains(&t.as_str()), "{}: {t}", case.id);
                    }
                }
            }
        }
    }

    #[test]
    fn checks_score_calls_summaries_and_replies() {
        let case = Case {
            id: "t".into(),
            start: "tshirt".into(),
            prompt: "Alarga la camiseta 5 cm".into(),
            checks: serde_json::from_value(json!([
                {"kind": "called", "tool": "set_segment_length", "with": {"piece": "front"}},
                {"kind": "called_any", "tools": ["move_points", "set_segment_length"]},
                {"kind": "not_called", "tool": "draft_garment"},
                {"kind": "count", "path": "/pieces", "min": 2, "max": 2},
                {"kind": "any", "path": "/pieces/*/name", "contains": "front"},
                {"kind": "changed", "path": "/pieces/0/size_cm/1", "min": 4.5, "max": 5.5},
                {"kind": "reply_contains", "any": ["5 cm"]},
                {"kind": "language", "lang": "es"},
                {"kind": "max_errors", "count": 0},
                {"kind": "max_calls", "count": 3}
            ]))
            .unwrap(),
        };
        let before = json!({"pieces": [{"name": "tshirt front", "size_cm": [50, 66]}, {"name": "tshirt back", "size_cm": [50, 66]}]});
        let after = json!({"pieces": [{"name": "tshirt front", "size_cm": [50, 71]}, {"name": "tshirt back", "size_cm": [50, 71]}]});
        let run = Transcript {
            calls: vec![
                Call {
                    name: "scene_summary".into(),
                    arguments: json!({}),
                    is_error: false,
                },
                Call {
                    name: "set_segment_length".into(),
                    arguments: json!({"piece": "Front", "segment": "side-right", "length_cm": 45}),
                    is_error: false,
                },
            ],
            reply: "He alargado el delantero y la espalda 5 cm; la costura del lado está cerrada."
                .into(),
            failure: None,
        };
        let outcomes = evaluate(&case, &before, &after, &run);
        for o in &outcomes {
            assert!(o.passed, "{} ({})", o.check, o.detail);
        }
        let wrong = Transcript {
            calls: vec![Call {
                name: "draft_garment".into(),
                arguments: json!({"kind": "tshirt"}),
                is_error: true,
            }],
            reply: "I made a new T-shirt for you.".into(),
            failure: None,
        };
        let failed = evaluate(&case, &before, &before, &wrong)
            .iter()
            .filter(|o| !o.passed)
            .count();
        assert_eq!(failed, 7);
        let results = vec![CaseResult {
            id: "t".into(),
            prompt: case.prompt.clone(),
            outcomes: evaluate(&case, &before, &before, &wrong),
            transcript: wrong,
            seconds: 3.,
        }];
        assert!(report(&results, "test").contains("0/1 cases passed"));
    }

    #[test]
    fn paths_walk_arrays_and_objects() {
        let v = json!({"a": [{"b": 1}, {"b": 2}], "m": {"x": {"b": 3}}});
        assert_eq!(values_at(&v, "/a/*/b").len(), 2);
        assert_eq!(values_at(&v, "/a/1/b"), vec![&json!(2)]);
        assert_eq!(values_at(&v, "/m/*/b"), vec![&json!(3)]);
        assert_eq!(count_at(&v, "/a"), 2);
        assert_eq!(language("The seam is closed and the fit is good"), "en");
        assert_eq!(
            language("La costura está cerrada y el ajuste es bueno"),
            "es"
        );
    }
}
