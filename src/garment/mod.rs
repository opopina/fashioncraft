//! Editable garment templates compile into a fixed, CPU-only physical model.
//! No mesh indices are persisted in the authoring format.
pub mod appearance;
pub mod arrangement;
pub mod curvefit;
pub mod curves;
pub mod diagnostics;
pub mod domain;
pub mod dxf_import;
pub mod embedding;
pub mod export;
pub mod freehand;
pub mod graphics;
pub mod lines2d;
pub mod markings;
pub mod meshing;
pub mod pins;
pub mod polygon;
pub mod presets;
pub mod production_metrics;
pub mod seam;
pub mod shapes;
pub mod shapes2d;
pub mod svg_import;
pub mod template;
pub mod transfer;
pub mod trims;

pub use template::{CompiledGarment, GarmentTemplate};
