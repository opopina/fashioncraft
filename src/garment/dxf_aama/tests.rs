//! The small rules of the apparel DXF reader and writer. Whole files are tested in
//! `tests/dxf_aama.rs`.
use super::*;

#[test]
fn quantity_reads_a_count_or_right_and_left() {
    assert_eq!(Quantity::parse("3"), Some(Quantity { right: 3, left: 0 }));
    assert_eq!(Quantity::parse("1,1"), Some(Quantity { right: 1, left: 1 }));
    assert_eq!(
        Quantity::parse(" 3 , 2 "),
        Some(Quantity { right: 3, left: 2 })
    );
    assert_eq!(Quantity::parse("0,0"), None);
    assert_eq!(Quantity::parse("1,1,1"), None);
    assert_eq!(Quantity::parse("two"), None);
}

#[test]
fn texts_are_key_and_value_whatever_their_case() {
    assert_eq!(
        key_value("Piece Name: Front"),
        Some(("PIECE NAME".into(), "Front"))
    );
    assert_eq!(
        key_value("PIECE  NAME:FRONT"),
        Some(("PIECE NAME".into(), "FRONT"))
    );
    assert_eq!(
        key_value("Grade Rule Table:"),
        Some(("GRADE RULE TABLE".into(), ""))
    );
    assert_eq!(key_value("no key here"), None);
}

#[test]
fn written_text_is_seven_bit_ascii() {
    assert_eq!(write::ascii("Puño · Ñandú — 2×"), "Puno - Nandu - 2x");
    assert_eq!(write::ascii("鞋"), "?");
    assert_eq!(
        template::file_stem("Camisa Oxford, talla M"),
        "camisa-oxford-talla-m"
    );
    assert_eq!(template::file_stem("·"), "patterns");
}

#[test]
fn unicode_escapes_are_read() {
    assert_eq!(read::unescape("Pu\\U+00F1o"), "Puño");
    assert_eq!(read::unescape("bad \\U+ZZZZ"), "bad \\U+ZZZZ");
}

#[test]
fn creation_date_and_time_have_the_style_text_form() {
    let (date, time) = template::now_utc();
    let date: Vec<&str> = date.split('-').collect();
    assert_eq!(date.len(), 3);
    assert_eq!((date[0].len(), date[1].len(), date[2].len()), (2, 2, 4));
    assert_eq!(time.len(), 5);
    assert_eq!(&time[2..3], ":");
}

/// A cut line drawn as separate lines, one of them backwards, closes into one outline.
#[test]
fn boundary_lines_are_chained() {
    let block = |entities: &str| {
        format!(
            "0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n8\n0\n2\nP\n70\n0\n10\n0\n20\n0\n{entities}0\nENDBLK\n\
             8\n0\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nTEXT\n8\n1\n10\n0\n20\n0\n40\n5\n1\n\
             Units: METRIC\n0\nINSERT\n8\n1\n2\nP\n10\n0\n20\n0\n0\nENDSEC\n0\nEOF\n"
        )
    };
    let line = |a: (f64, f64), b: (f64, f64)| {
        format!(
            "0\nLINE\n8\n1\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n",
            a.0, a.1, b.0, b.1
        )
    };
    let text = block(
        &[
            line((0., 0.), (100., 0.)),
            line((100., 50.), (100., 0.)),
            line((100., 50.), (0., 50.)),
            line((0., 50.), (0., 0.)),
        ]
        .concat(),
    );
    assert!(is_apparel(&text));
    let file = read(&text, ReadOptions::default()).unwrap();
    assert_eq!(file.pieces.len(), 1);
    let cut = &file.pieces[0].cut;
    assert_eq!(cut.points.len(), 4);
    // No turn points in the file: the corners are found by angle.
    assert!(cut.turns.iter().all(|t| *t));
    assert!(file.issues.is_empty(), "{:?}", file.issues);
}

#[test]
fn block_names_are_short_plain_and_unique() {
    let taken = vec!["FRONT_M".to_string()];
    assert_eq!(write::block_name("Front", "M", &taken), "FRONT_M_2");
    assert_eq!(write::block_name("Espalda baja", "", &[]), "ESPALDA_BAJA");
    assert!(write::block_name(&"x".repeat(80), "XXL", &[]).len() <= 31);
}
