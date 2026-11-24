//! Knowledge on demand for agents: the `guide` tool. Short notes on pattern making and
//! sewing, and on how FashionCraft does each thing with its tools, so the shared prompt stays
//! short and an agent reads a topic when it needs it. Longer explanations live in `docs/`
//! (named at the end of each topic).

/// Every topic: its name, a one-line summary and its text.
pub const TOPICS: &[(&str, &str, &str)] = &[
    (
        "workflow",
        "The order that works: look, draft, edit, sew, simulate, check, fix",
        "1. scene_summary (and piece_detail before editing a piece).\n\
         2. Start from draft_garment when the garment is one of its bases; otherwise \
         create_piece, place_piece and sew.\n\
         3. Change shapes with the exact tools (set_segment_length, bend_segment, add_dart, \
         add_pleats, add_detail…) rather than moving raw points; use edit for anything else.\n\
         4. simulate for 5–10 s, then check_garment. Fix what it reports (open seams, strain, \
         cloth inside the body), simulate again and check again until it says ok.\n\
         5. Look: camera with a view and screenshot true, or render.\n\
         Every change is on Edit › Undo; tell the person what you changed.",
    ),
    (
        "grain",
        "Grain lines: warp, weft and bias, and how a piece's grain is set",
        "The warp (lengthwise grain) runs along the selvage and is the fabric's strongest, \
         least stretchy direction; the weft (crosswise) gives a little more; the bias (45°) \
         stretches most and drapes in soft folds. Most pieces are cut with the warp running \
         down the body (centre front, centre back, down a sleeve or a trouser leg). Bias-cut \
         skirts and dresses cling and flare; cutting a piece off grain makes it twist.\n\
         In FashionCraft: piece_edit set_grain sets a piece's warp angle (degrees, \
         counter-clockwise from the board's x axis; 90° is vertical). Stretch along warp and \
         weft is set per fabric with set_fabric_parameters. docs/fabrics.md.",
    ),
    (
        "ease",
        "Wearing ease, design ease and sewing ease",
        "Wearing ease is the room between body and garment needed to move and breathe: about \
         5–10 cm at the bust for a fitted woven top, 10–20 cm for a loose shirt, 0 or negative \
         for knits that stretch (negative ease: the garment is smaller than the body). Design \
         ease is extra fullness for the style. Sewing ease is when one seam side is a little \
         longer and is eased in without gathers (a sleeve cap, 1–4 cm over the armhole).\n\
         In FashionCraft: compare the pattern's widths (scene_summary sizes) to the avatar's \
         measurements; after simulating, check_garment's strain and pressure show where ease \
         is missing. A seam whose sides differ needs ease: sew with ease true, or seam_edit \
         allow_ease.",
    ),
    (
        "seam_allowance",
        "Seam allowance: what it is and that FashionCraft draws it but does not simulate it",
        "The seam allowance is the margin outside the stitching line that is folded inside \
         the garment: commonly 1–1.5 cm on side and shoulder seams, 0.6 cm on curves such as \
         necklines and armholes, 2–4 cm on hems. Patterns for cutting include it; patterns \
         for fitting are the stitching lines.\n\
         In FashionCraft the outline is the stitching line, as in 3D garment tools; \
         piece_edit set_seam_allowance sets the allowance of a segment for printing and \
         export (DXF-AAMA), and it never changes the simulation. docs/drafting-tools.md.",
    ),
    (
        "notches",
        "Notches: matching marks that make seams meet where they should",
        "Notches are small cuts or marks on the edge that line up two pieces when sewing: at \
         the side of a sleeve cap (one notch front, two back), at the waist on side seams, at \
         a gathered stretch's ends. They prevent a seam from being sewn twisted or shifted.\n\
         In FashionCraft: notches sit on a segment at a share of its length (piece_detail \
         lists them; edit SetNotch adds one). When sewing, the sew tool's from_cm/to_cm sew \
         one stretch to another, which is what notches mark. docs/markings.md.",
    ),
    (
        "darts",
        "Darts: taking in fullness to shape flat cloth around the body",
        "A dart is a wedge (or a diamond) of cloth stitched away so the flat piece follows a \
         curve: bust darts point to the bust point and stop 2–3 cm short of it; waist darts on \
         skirts and trousers run 9–14 cm down from the waist, 1.5–3 cm wide; diamond darts \
         shape a dress at the waist (2–3 cm wide, 20–30 cm tall). A dart's two legs must be \
         equal so they sew together.\n\
         In FashionCraft: add_dart with a segment cuts and sews an edge dart (width_cm at the \
         edge, depth_cm into the piece, at_cm along the segment); without a segment it makes \
         a diamond dart at centre_cm. The dart's legs become a seam. docs/drafting-tools.md.",
    ),
    (
        "set_in_sleeves",
        "Set-in sleeves: cap height, cap ease and sewing the sleeve into the armhole",
        "A set-in sleeve is sewn into a separate armhole. Its cap is 1–4 cm longer than the \
         armhole (cap ease, eased in without gathers); a taller cap (14–17 cm) fits close \
         with arms down, a lower one (8–12 cm) lets the arm lift. The underarm seam joins the \
         sleeve's sides; the cap's front half goes to the front armhole and the back half to \
         the back.\n\
         In FashionCraft: draft_garment shirt makes front, back and two set-in sleeves with \
         each cap half eased into the armhole it faces. To sew one yourself: sew the cap's \
         halves to the armholes with ease true, then the underarm. Raise the cap by moving its \
         top point (move_points) or bend_segment to round it. docs/garment-drafting (shirt).",
    ),
    (
        "sewing",
        "How sewing works in FashionCraft: segments, M:N, stretches, direction, ease",
        "A seam joins side a to side b. Each side is one segment or several in order (M:N, \
         for example two back halves to one collar), whole or a stretch (from_cm/to_cm). \
         Both sides must have the same length unless ease is allowed. A seam sewn with its \
         ends crossed twists the cloth: the sew tool picks the direction from where the \
         pieces are placed unless reverse is given. Internal lines can be sewn too (a patch \
         pocket is sewn to an internal line on the front). After simulating, scene_summary \
         and check_garment give each seam's gap: under 6 mm counts as closed.\n\
         Tools: sew, seam_edit, add_detail (sews what it makes), edit SewChain / Sew for \
         anything unusual. docs/documents-and-sewing.md, docs/sewing-audit.md.",
    ),
    (
        "simulation",
        "Simulating and judging a drape",
        "simulate runs the cloth for wall-clock seconds and pauses. A first drape needs 5–10 \
         s; a garment that started far from the body or a heavy fabric needs more. Judge only \
         a settled drape: check_garment says whether it still moves. If seams stay open, the \
         pieces may be too far apart, sewn reversed (twisted), or of unequal length. Cloth \
         inside the body usually means a piece started through it: place_piece or \
         transform_piece in 3D, then simulate again. simulation_settings changes quality \
         (fast, normal, accurate), GPU, friction, gravity and wind; the drape keeps going \
         from where it is. docs/simulation-speed.md.",
    ),
    (
        "fabrics",
        "Fabric presets, physical parameters and measuring a real fabric",
        "set_fabric applies a preset (cotton, jersey, wool, denim, leather, silk, linen, \
         nylon, stretch): indicative values, not lab measurements. set_fabric_parameters sets \
         weight (g/m²), thickness (mm), stretch along warp and weft (% under 5 N on a 5 cm \
         strip), shear, bending and friction. For a real fabric, measure_fabric fits the \
         parameters to simple tests (strip stretch under weights, cantilever overhang, slide \
         angle): this is how to get a fabric right for manufacturing. Heavier, stiffer \
         fabrics stand away from the body; light, soft ones cling and fold finely. \
         docs/fabrics.md, docs/fabric-kit.md.",
    ),
    (
        "fit_checks",
        "Reading check_garment: gaps, strain, pressure, cloth inside the body, settling",
        "Open seam: a gap over 6 mm after simulating. Strain: how far the cloth is stretched \
         beyond its rest length; woven cloth over ~8 % is pulled hard (too tight, or a seam \
         pulling); knits tolerate more. Pressure: what the garment presses on the body; \
         everyday clothes stay under ~1 kPa, compression wear 2–4 kPa; over 2 kPa feels \
         tight. Inside the body: cloth deeper than 3 mm inside the avatar, usually from a bad \
         start position. Settling: the drape still moves; simulate longer. Each problem says \
         where: the piece, its area (top/middle/bottom × left/centre/right on the board), the \
         nearest segment and the height above the floor.",
    ),
    (
        "details",
        "Pockets, collars and plackets from parameters",
        "add_detail makes them exactly, sewn and laid on the drape: patch_pocket (piece, \
         width_cm, height_cm, centre_cm on the piece; stitched on three sides, open at the \
         top; a shirt pocket is about 11–13 × 13–15 cm, a jeans back pocket 13–15 × 14–16 cm), \
         band_collar (the neckline's segments in order and height_cm, 2.5–4 cm for a band; it \
         is as long as the neckline), placket (piece, segment and width_cm, 2.5–3.5 cm on a \
         shirt front). Then add_buttons for buttons and buttonholes (shirt buttons 1–1.2 cm, \
         7–9 cm apart).",
    ),
    (
        "grading",
        "Sizes, grade rules and points of measure",
        "Grading makes the other sizes from the base pattern by moving outline points by a \
         step per size (grade rules). In FashionCraft: grading create (sizes smallest to \
         largest and the base), add_size, set_step (dx/dy mm per size at a segment's start \
         point), show (a size in 3D). points_of_measure adds the measurements a size chart \
         lists (a distance between two outline points or the length of segments) with a \
         tolerance; size_chart reports every one in every size. Typical steps: 4–5 cm of \
         girth per size, split over the pieces that make the girth. docs/grading.md.",
    ),
    (
        "avatar",
        "Avatars: library bodies, measurements, the avatar maker and poses",
        "set_avatar takes a body from the library or reshapes the current one to height, \
         bust, waist, hip, arm and inseam in cm. avatar_maker builds a new body from Anny's \
         settings (gender, age, muscle, weight, height, proportions, cup size, ancestry) and \
         twenty local measures. set_pose chooses a preset pose; pose_joints turns single \
         joints in degrees. Changing the body or the pose restarts the drape: simulate \
         again, and redraft pieces drafted for another body. docs/avatar-import.md, \
         docs/anny-rig.md.",
    ),
    (
        "pleats_and_folds",
        "Pleats, folds and gathers",
        "Knife pleats fold one way, box pleats fold away from a centre, inverted box pleats \
         toward it; each pleat takes twice its depth of fabric. add_pleats lays them along a \
         segment and tacks them at the edge. A fold line is an internal line with a fold \
         angle (edit SetLineFold). Gathers: sew a longer side to a shorter one with ease \
         true. docs/drafting-tools.md.",
    ),
];

/// The text of `topic` (or of the topic whose name contains it), or the list of topics.
pub fn guide(topic: &str) -> String {
    let wanted = topic.trim().to_lowercase().replace([' ', '-'], "_");
    let found = TOPICS
        .iter()
        .find(|(name, ..)| *name == wanted)
        .or_else(|| {
            TOPICS.iter().find(|(name, summary, _)| {
                !wanted.is_empty()
                    && (name.contains(&wanted) || summary.to_lowercase().contains(&wanted))
            })
        });
    match found {
        Some((name, summary, text)) => format!("{name}: {summary}\n\n{text}"),
        None => {
            let list: Vec<String> = TOPICS
                .iter()
                .map(|(name, summary, _)| format!("- {name}: {summary}"))
                .collect();
            format!("Topics:\n{}", list.join("\n"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topics_are_found_by_name_or_word_and_name_real_tools() {
        assert!(guide("darts").contains("add_dart"));
        assert!(guide("Set-in sleeves").contains("cap"));
        assert!(guide("pocket").starts_with("details"));
        assert!(guide("").starts_with("Topics:"));
        let tools: Vec<&str> = crate::agent::tools().iter().map(|t| t.name).collect();
        for (name, _, text) in TOPICS {
            for word in text.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
                if word.contains('_') && word.chars().all(|c| c.is_lowercase() || c == '_') {
                    let known = tools.contains(&word)
                        || [
                            "set_grain",
                            "set_seam_allowance",
                            "allow_ease",
                            "from_cm",
                            "to_cm",
                            "width_cm",
                            "depth_cm",
                            "at_cm",
                            "centre_cm",
                            "height_cm",
                            "add_size",
                            "set_step",
                            "patch_pocket",
                            "band_collar",
                        ]
                        .contains(&word);
                    assert!(known, "topic {name} names '{word}', which is no tool");
                }
            }
        }
    }
}
