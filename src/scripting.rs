//! Python scripts (File › Import › Python script): run a script with Python 3 on the garment
//! that is open. The script reads the garment as plain data through the small `fashioncraft`
//! module written next to it, changes it, and saves it; the app then applies the saved garment
//! as one undoable edit. Nothing changes unless the script saves.
//!
//! The script runs as its own process, from its own folder, with a time limit; what it prints
//! is kept for the app to show.
use crate::garment::GarmentTemplate;
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// The `fashioncraft` module a script imports.
pub const HELPER: &str = r#""""FashionCraft's script helper: the garment open in FashionCraft, as plain data.

    import fashioncraft
    garment = fashioncraft.garment()
    for piece in fashioncraft.pieces(garment):
        print(piece["id"], piece["name"])
    fashioncraft.scale_piece(garment, "Front", 1.05)
    fashioncraft.save(garment)

Lengths are in metres. FashionCraft applies the saved garment as one undoable edit; nothing
changes unless the script calls save().
"""
import json
import os

__all__ = [
    "garment", "save", "pieces", "piece", "fabrics", "seams", "rename_piece", "scale_piece",
]


def garment():
    """The garment open in FashionCraft, as a dict."""
    with open(os.environ["FASHIONCRAFT_GARMENT"], encoding="utf-8") as f:
        return json.load(f)


def save(garment):
    """Hand the changed garment back to FashionCraft."""
    with open(os.environ["FASHIONCRAFT_RESULT"], "w", encoding="utf-8") as f:
        json.dump(garment, f)


def pieces(garment):
    """The garment's pieces."""
    return garment.get("panels", [])


def piece(garment, name_or_id):
    """The piece with this id or name."""
    for p in pieces(garment):
        if p.get("id") == name_or_id or p.get("name") == name_or_id:
            return p
    raise KeyError("No piece named %r" % (name_or_id,))


def fabrics(garment):
    """The garment's fabrics."""
    return garment.get("fabrics", [])


def seams(garment):
    """The garment's seams."""
    return garment.get("seams", [])


def rename_piece(garment, name_or_id, name):
    piece(garment, name_or_id)["name"] = str(name)


def scale_piece(garment, name_or_id, sx, sy=None):
    """Scale a drafted piece about the middle of its outline: sx across, sy along (sx if not
    given). Its outline, curves, holes and internal lines follow; the seams along it take up
    the change in length (easing)."""
    sy = sx if sy is None else sy
    target = piece(garment, name_or_id)
    polygon = target.get("polygon")
    if not polygon:
        raise ValueError("Only drafted pieces can be scaled")
    xs = [p[0] for p in polygon["points"]]
    ys = [p[1] for p in polygon["points"]]
    cx, cy = (min(xs) + max(xs)) / 2, (min(ys) + max(ys)) / 2

    def scaled(p):
        return [cx + (p[0] - cx) * sx, cy + (p[1] - cy) * sy]

    polygon["points"] = [scaled(p) for p in polygon["points"]]
    polygon["curves"] = {k: [scaled(c) for c in v] for k, v in polygon.get("curves", {}).items()}
    for hole in polygon.get("holes", []):
        hole["points"] = [scaled(p) for p in hole["points"]]
    for line in polygon.get("lines", []):
        line["points"] = [scaled(p) for p in line["points"]]
        line["tangents"] = [[t[0] * sx, t[1] * sy] for t in line.get("tangents", [])]
    for seam in seams(garment):
        sides = [seam.get("a", {}), seam.get("b", {})]
        sides += seam.get("a_tail", []) + seam.get("b_tail", [])
        if any(side.get("panel") == target["id"] for side in sides):
            seam["allow_ease"] = True
"#;

/// Longest a script may run.
pub const TIME_LIMIT: Duration = Duration::from_secs(120);
/// Largest garment a script may hand back, bytes.
const MAX_RESULT_BYTES: u64 = 64 * 1024 * 1024;
/// Most output kept from a script, bytes.
const MAX_OUTPUT_BYTES: usize = 256 * 1024;

/// What a script run left.
#[derive(Debug)]
pub struct ScriptRun {
    /// What it printed, its errors after.
    pub output: String,
    /// Its exit code; `None` when it was stopped.
    pub code: Option<i32>,
    /// The garment it saved, if it saved one.
    pub garment: Option<GarmentTemplate>,
}

/// The Python 3 to run scripts with: `FASHIONCRAFT_PYTHON` when set, else `python3`.
pub fn python() -> PathBuf {
    std::env::var_os("FASHIONCRAFT_PYTHON").map_or_else(|| "python3".into(), PathBuf::from)
}

fn read_capped(mut from: impl Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = (&mut from)
            .take(MAX_OUTPUT_BYTES as u64)
            .read_to_end(&mut bytes);
        // Drain the rest so the script never blocks on a full pipe.
        let _ = std::io::copy(&mut from, &mut std::io::sink());
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

/// Run `script` on `garment`, with its helper files in `work`, for at most `limit`.
pub fn run(
    script: &Path,
    garment: &GarmentTemplate,
    work: &Path,
    limit: Duration,
) -> Result<ScriptRun, String> {
    let script = script
        .canonicalize()
        .map_err(|e| format!("{}: {e}", script.display()))?;
    std::fs::create_dir_all(work).map_err(|e| e.to_string())?;
    let helper_dir = work.join("python");
    std::fs::create_dir_all(&helper_dir).map_err(|e| e.to_string())?;
    std::fs::write(helper_dir.join("fashioncraft.py"), HELPER).map_err(|e| e.to_string())?;
    let input = work.join("garment.json");
    let result = work.join("result.json");
    std::fs::write(
        &input,
        serde_json::to_vec(garment).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&result);
    let bootstrap = "import runpy, sys\n\
         sys.path.insert(0, sys.argv[1])\n\
         sys.argv = sys.argv[2:]\n\
         runpy.run_path(sys.argv[0], run_name='__main__')\n";
    let mut child = Command::new(python())
        .arg("-B")
        .arg("-c")
        .arg(bootstrap)
        .arg(&helper_dir)
        .arg(&script)
        .current_dir(script.parent().unwrap_or(work))
        .env("FASHIONCRAFT_GARMENT", &input)
        .env("FASHIONCRAFT_RESULT", &result)
        .env("PYTHONIOENCODING", "utf-8")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            format!(
                "Python 3 could not be started ({e}); install it from python.org, or set FASHIONCRAFT_PYTHON to it"
            )
        })?;
    let out = read_capped(child.stdout.take().expect("piped"));
    let err = read_capped(child.stderr.take().expect("piped"));
    let start = Instant::now();
    let code = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status.code();
        }
        if start.elapsed() > limit {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let mut output = out.join().unwrap_or_default();
    let errors = err.join().unwrap_or_default();
    if !errors.is_empty() {
        if !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str(&errors);
    }
    if code.is_none() {
        output.push_str(&format!(
            "\nStopped after {} s.\n",
            limit.as_secs_f64().round()
        ));
    }
    let garment = if code == Some(0) && result.exists() {
        let mut bytes = Vec::new();
        std::fs::File::open(&result)
            .map_err(|e| e.to_string())?
            .take(MAX_RESULT_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_RESULT_BYTES {
            return Err("The garment the script saved is too large".into());
        }
        Some(
            serde_json::from_slice(&bytes)
                .map_err(|e| format!("The garment the script saved cannot be read: {e}"))?,
        )
    } else {
        None
    };
    Ok(ScriptRun {
        output,
        code,
        garment,
    })
}
