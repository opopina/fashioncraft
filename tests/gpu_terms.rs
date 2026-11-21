//! The GPU solver holds at most four vertices per term: stitches with more (a fastened button
//! joins two points inside triangles) are reduced so they still sum to zero.
#![cfg(feature = "desktop")]
use fashioncraft::gpu_vbd::four_terms;

#[test]
fn a_tack_between_two_triangles_fits_in_four_terms_and_still_sums_to_zero() {
    // A point inside triangle (0, 1, 2) tied to one inside (7, 8, 9).
    let tack = [
        (0, 0.5),
        (1, 0.3),
        (2, 0.2),
        (7, -0.6),
        (8, -0.25),
        (9, -0.15),
    ];
    let terms = four_terms(&tack);
    assert_eq!(terms.len(), 4);
    let sum: f64 = terms.iter().map(|(_, w)| w).sum();
    assert!(sum.abs() < 1e-12);
    // Each side keeps its two heaviest vertices and its own total.
    let positive: Vec<usize> = terms
        .iter()
        .filter(|(_, w)| *w > 0.)
        .map(|(v, _)| *v)
        .collect();
    let negative: Vec<usize> = terms
        .iter()
        .filter(|(_, w)| *w < 0.)
        .map(|(v, _)| *v)
        .collect();
    assert_eq!(positive, vec![0, 1]);
    assert_eq!(negative, vec![7, 8]);
    let total: f64 = terms.iter().filter(|(_, w)| *w > 0.).map(|(_, w)| w).sum();
    assert!((total - 1.).abs() < 1e-12);
}

#[test]
fn an_ordinary_seam_sample_is_left_as_it_is() {
    let sample = [(3, 0.7), (4, 0.3), (10, -0.4), (11, -0.6)];
    assert_eq!(four_terms(&sample), sample.to_vec());
}
