//! Editable garment templates compile into a fixed, CPU-only physical model.
//! No mesh indices are persisted in the authoring format.
pub mod appearance;
pub mod arrangement;
pub mod auto_sew;
pub mod curvefit;
pub mod curves;
pub mod diagnostics;
pub mod domain;
pub mod dxf_aama;
pub mod dxf_import;
pub mod embedding;
pub mod export;
pub mod freehand;
pub mod grading;
pub mod graphics;
pub mod lines2d;
pub mod markings;
pub mod meshing;
pub mod pins;
pub mod pleats;
pub mod polygon;
pub mod presets;
pub mod print_pdf;
pub mod production_metrics;
pub mod seam;
pub mod seam_break;
pub mod shapes;
pub mod shapes2d;
pub mod steam;
pub mod svg_import;
pub mod template;
pub mod texture_set;
pub mod transfer;
pub mod trims;
pub mod uv_layout;

pub use template::{CompiledGarment, FabricValues, GarmentTemplate};
