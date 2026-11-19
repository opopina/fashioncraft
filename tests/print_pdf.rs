//! Patterns at 1:1 as a PDF: the print sheet's layout on one page, or tiled over framed pages
//! of A4, Letter or A3, at 72/25.4 points per millimetre.
use fashioncraft::{
    fitting::FittingProject,
    garment::{
        export,
        presets::GarmentKind,
        print_pdf::{PAGE_MARGIN_MM, Paper, print_pattern_pdf},
    },
};
use std::path::PathBuf;

const PT: f64 = 72. / 25.4;

fn project() -> FittingProject {
    FittingProject::basic(GarmentKind::Tshirt, 1, PathBuf::from("unused.obj"))
}

/// The print SVG's sheet size, mm.
fn sheet(template: &fashioncraft::garment::GarmentTemplate) -> (f64, f64) {
    let svg = export::print_pattern_svg(template).unwrap();
    let number = |key: &str| -> f64 {
        let at = svg.find(key).unwrap() + key.len();
        svg[at..at + svg[at..].find("mm").unwrap()].parse().unwrap()
    };
    (number("width=\""), number("height=\""))
}

/// Every object of the file, checked against the cross-reference table.
fn objects(pdf: &[u8]) -> Vec<String> {
    // One character per byte, so offsets stay byte offsets.
    let text: String = pdf
        .iter()
        .map(|b| if *b < 0x80 { *b as char } else { '?' })
        .collect();
    assert!(text.starts_with("%PDF-1.4\n"));
    assert!(text.ends_with("%%EOF\n"));
    let start: usize = text[text.rfind("startxref\n").unwrap() + 10..]
        .lines()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!(text[start..].starts_with("xref\n0 "));
    let table: Vec<&str> = text[start..].lines().collect();
    let count: usize = table[1].split(' ').nth(1).unwrap().parse().unwrap();
    let mut out = vec![];
    for k in 1..count {
        let line = table[2 + k];
        assert_eq!(line.len(), 19, "{line:?}");
        let offset: usize = line[..10].parse().unwrap();
        let header = format!("{k} 0 obj\n");
        assert!(text[offset..].starts_with(&header), "object {k}");
        let end = text[offset..].find("\nendobj\n").unwrap();
        out.push(text[offset + header.len()..offset + end].to_string());
    }
    out
}

/// The pages' media boxes and content streams.
fn pages(objects: &[String]) -> Vec<((f64, f64), String)> {
    let mut out = vec![];
    for (k, o) in objects.iter().enumerate() {
        if let Some(at) = o.find("/MediaBox [0 0 ") {
            let sizes: Vec<f64> = o[at + 15..o[at..].find(']').unwrap() + at]
                .split(' ')
                .map(|v| v.parse().unwrap())
                .collect();
            let stream = &objects[k + 1];
            let body =
                &stream[stream.find("stream\n").unwrap() + 7..stream.rfind("\nendstream").unwrap()];
            let length: usize = stream
                [stream.find("/Length ").unwrap() + 8..stream.find(" >>").unwrap()]
                .parse()
                .unwrap();
            assert_eq!(body.len(), length);
            out.push(((sizes[0], sizes[1]), body.to_string()));
        }
    }
    out
}

#[test]
fn a4_pages_tile_the_sheet_at_actual_size() {
    let project = project();
    let template = &project.template;
    let (width, height) = sheet(template);
    let (pdf, summary) = print_pattern_pdf(template, Paper::A4).unwrap();
    let tile = (210. - 2. * PAGE_MARGIN_MM, 297. - 2. * PAGE_MARGIN_MM);
    assert_eq!(summary.columns, (width / tile.0).ceil() as usize);
    assert_eq!(summary.rows, (height / tile.1).ceil() as usize);
    assert!(summary.pages >= 2 && summary.pages <= summary.rows * summary.columns);
    let objects = objects(&pdf);
    assert!(objects[1].contains(&format!("/Count {}", summary.pages)));
    assert!(objects[2].contains("/BaseFont /Helvetica"));
    let pages = pages(&objects);
    assert_eq!(pages.len(), summary.pages);
    let mut names = std::collections::BTreeSet::new();
    for (k, ((w, h), body)) in pages.iter().enumerate() {
        // A4 in points.
        assert!((w - 210. * PT).abs() < 1e-3 && (h - 297. * PT).abs() < 1e-3);
        // A frame to trim along, and the page's name.
        assert!(body.contains(" re S"));
        let label = &body[body.find("(").unwrap()..];
        assert!(label.contains(&format!("page {} of {}", k + 1, summary.pages)));
        let tile = &label[label.find("tile ").unwrap() + 5..];
        names.insert(tile[..tile.find(' ').unwrap()].to_string());
        // Millimetres drawn at 72/25.4 points each, y flipped.
        assert!(body.contains(&format!("{PT:.6} 0 0 {:.6}", -PT)));
    }
    assert_eq!(names.len(), summary.pages);
    assert!(names.contains("A1"));
    // The 100 mm calibration ruler, from 20 to 120 mm, is on a page.
    assert!(pages.iter().any(|(_, b)| {
        b.lines()
            .any(|l| l.starts_with("0 0 0 RG 0.3 w [] 0 d 20.000 ") && l.contains(" m 120.000 "))
    }));
}

#[test]
fn one_sheet_holds_every_piece_outline_and_letter_and_a3_tile_too() {
    let project = project();
    let template = &project.template;
    let (width, height) = sheet(template);
    let (pdf, summary) = print_pattern_pdf(template, Paper::Sheet).unwrap();
    assert_eq!((summary.pages, summary.rows, summary.columns), (1, 1, 1));
    let sheet_pages = pages(&objects(&pdf));
    let ((w, h), body) = &sheet_pages[0];
    assert!((w - width * PT).abs() < 1e-3 && (h - height * PT).abs() < 1e-3);
    // No frame on one sheet; each piece's closed sewing outline and its label.
    assert!(!body.contains(" re S"));
    let outlines = body
        .lines()
        .filter(|l| l.starts_with("0 0 0 RG 0.3 w [] 0 d") && l.ends_with(" h S"))
        .count();
    assert_eq!(outlines, template.panels.len());
    for panel in &template.panels {
        assert!(body.contains(&format!("({} ? ", panel.id)));
    }
    for (paper, size) in [(Paper::Letter, (215.9, 279.4)), (Paper::A3, (297., 420.))] {
        let (pdf, summary) = print_pattern_pdf(template, paper).unwrap();
        let pages = pages(&objects(&pdf));
        assert_eq!(pages.len(), summary.pages);
        let ((w, h), _) = pages[0];
        assert!((w - size.0 * PT).abs() < 1e-3 && (h - size.1 * PT).abs() < 1e-3);
    }
    // Fewer, larger pages on A3.
    assert!(
        print_pattern_pdf(template, Paper::A3).unwrap().1.pages
            < print_pattern_pdf(template, Paper::A4).unwrap().1.pages
    );
}

#[test]
fn labels_are_escaped_and_kept_in_the_pdf_alphabet() {
    let mut project = project();
    project.template.name = "Shirt (v2) \\ 東京 — test".into();
    let (pdf, _) = print_pattern_pdf(&project.template, Paper::Sheet).unwrap();
    let objects = objects(&pdf);
    // The title in the document information: parentheses and backslash escaped, the
    // characters Helvetica's encoding lacks replaced, the em dash in WinAnsi.
    let info = &objects[3];
    let expected: Vec<u8> = b"(Shirt \\(v2\\) \\\\ ?? \x97 test at 1:1)".to_vec();
    let raw = pdf
        .windows(expected.len())
        .any(|w| w == expected.as_slice());
    assert!(raw, "{info}");
}
