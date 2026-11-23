//! A CPU-only frontend; never imports eframe/wgpu or opens a window.
use clap::{Parser, Subcommand};
use fashioncraft::{
    garment::{GarmentTemplate, dxf_aama, export, grading, production_metrics},
    solver::{ReferenceSolver, SolverSettings},
};
use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::PathBuf,
};

#[derive(Parser)]
#[command(
    about = "Validate, compile and assemble garment templates entirely on CPU",
    version
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Write a small editable example. Refuses to overwrite an existing file.
    Example {
        output: PathBuf,
    },
    Validate {
        template: PathBuf,
    },
    /// Report net rest-mesh area and mass per panel and fabric (not nesting consumption).
    Metrics {
        template: PathBuf,
    },
    /// Diagnose authored sewing without running simulation. Invalid sewing exits nonzero after JSON output.
    AuditSewing {
        template: PathBuf,
        /// Read the template embedded in a fitting-project v1 JSON; ignore avatar and draped state.
        #[arg(long)]
        project: bool,
    },
    /// Produce rest OBJ, preview and measured SVG outlines, and fiber frames.
    Bake {
        template: PathBuf,
        #[arg(long, default_value = "output/template-bake")]
        output: PathBuf,
    },
    /// Write the pieces as DXF-AAMA or DXF-ASTM (D6673), for pattern CAD, plotters and cutters.
    DxfExport {
        template: PathBuf,
        output: PathBuf,
        /// aama or astm.
        #[arg(long, default_value = "aama")]
        dialect: String,
        /// mm (metric) or in (english).
        #[arg(long, default_value = "mm")]
        units: String,
        /// The size the pieces are drawn in.
        #[arg(long, default_value = "")]
        size: String,
        /// Read the template embedded in a fitting-project v1 JSON.
        #[arg(long)]
        project: bool,
    },
    /// Read a DXF-AAMA or DXF-ASTM file: its dialect, units, pieces and what would not be
    /// imported, as JSON. A rule table (`.rul`) beside it is read too.
    DxfInspect {
        file: PathBuf,
    },
    /// Write the garment in one size of its grading, as an ungraded template.
    GradeSize {
        template: PathBuf,
        output: PathBuf,
        /// The size's name.
        #[arg(long)]
        size: String,
        /// The size group's name; the first group when absent.
        #[arg(long)]
        group: Option<String>,
    },
    /// The points of measure in every size, as JSON (millimetres), or CSV with --csv.
    Pom {
        template: PathBuf,
        #[arg(long)]
        csv: bool,
    },
    Simulate {
        template: PathBuf,
        #[arg(long, default_value = "output/template-simulation")]
        output: PathBuf,
        #[arg(long, default_value_t = 30)]
        steps: usize,
        #[arg(long,default_value_t=1.0/60.0)]
        dt: f64,
        #[arg(long, default_value_t = 2)]
        threads: usize,
    },
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    match Args::parse().command {
        Command::Example { output } => {
            let t = GarmentTemplate::two_panel_demo();
            t.compile()?;
            let mut file = File::create_new(output)?;
            file.write_all(serde_json::to_string_pretty(&t)?.as_bytes())?;
        }
        Command::Validate { template } => {
            let t = GarmentTemplate::load(&template)?;
            let g = t.compile()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                "name":t.name,"panels":g.panels.len(),"vertices":g.mesh.uv.len(),"triangles":g.mesh.triangles.len(),
                "seams":g.seams.len(),"initial_gap_m":g.max_seam_gap(&g.initial_state.x),"warnings":g.warnings}))?
            );
        }
        Command::Metrics { template } => {
            let template = GarmentTemplate::load(&template)?;
            let report = production_metrics::report(&template)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Command::AuditSewing { template, project } => {
            // The regular loader compiles first and would hide all but the first
            // sewing error. Geometry validation remains inside the audit.
            let json = fs::read_to_string(template)?;
            let template: GarmentTemplate = if project {
                let project: fashioncraft::fitting::FittingProject = serde_json::from_str(&json)?;
                if project.version != 1 {
                    return Err("Unsupported fitting project version".into());
                }
                project.template
            } else {
                serde_json::from_str(&json)?
            };
            let report = fashioncraft::garment::seam::audit::report(&template)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            if !report.valid {
                return Err("Invalid authored sewing; inspect the JSON diagnostics".into());
            }
        }
        Command::Bake { template, output } => {
            let t = GarmentTemplate::load(&template)?;
            let g = t.compile()?;
            export::write_artifacts(&t, &g, &g.initial_state, &output)?;
            println!("CPU artifacts written to {}", output.display());
        }
        Command::DxfExport {
            template,
            output,
            dialect,
            units,
            size,
            project,
        } => {
            let t = if project {
                let p: fashioncraft::fitting::FittingProject =
                    serde_json::from_str(&fs::read_to_string(&template)?)?;
                p.template
            } else {
                GarmentTemplate::load(&template)?
            };
            let dialect = match dialect.to_lowercase().as_str() {
                "aama" => dxf_aama::Dialect::Aama,
                "astm" => dxf_aama::Dialect::Astm,
                other => return Err(format!("Unknown dialect {other}: use aama or astm").into()),
            };
            let units = match units.to_lowercase().as_str() {
                "mm" | "metric" => dxf_aama::Units::Millimetres,
                "in" | "inch" | "inches" | "english" => dxf_aama::Units::Inches,
                other => return Err(format!("Unknown units {other}: use mm or in").into()),
            };
            // A graded garment goes out in its base size, with its rule table beside it.
            let grading::dxf::Export { pieces, table } = grading::dxf::export(
                &t,
                &dxf_aama::template::PieceOptions {
                    size: size.clone(),
                    ..Default::default()
                },
                units,
                dialect,
            )?;
            let mut table = table;
            let rul_path = output.with_extension("rul");
            if let Some(table) = &mut table {
                table.name = grading::rul::size_word(
                    &output
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_uppercase())
                        .unwrap_or_default(),
                );
            }
            let bytes = dxf_aama::write(
                &pieces,
                &dxf_aama::WriteOptions {
                    dialect,
                    units,
                    style: t.name.clone(),
                    sample_size: table.as_ref().map_or(size, |t| t.sample.clone()),
                    author: dxf_aama::template::author(),
                    created: Some(dxf_aama::template::now_utc()),
                    grade_rule_table: table.as_ref().map(|t| t.name.clone()).unwrap_or_default(),
                },
            )?;
            fs::write(&output, bytes)?;
            println!(
                "{} pieces written to {} as {}",
                pieces.len(),
                output.display(),
                dialect.label()
            );
            if let Some(table) = &table {
                fs::write(&rul_path, grading::rul::write(table)?)?;
                println!(
                    "{} grade rules for sizes {} (sample {}) written to {}",
                    table.rules.len(),
                    table.sizes.join(" "),
                    table.sample,
                    rul_path.display()
                );
            }
        }
        Command::DxfInspect { file } => {
            let text = dxf_aama::load(&file)?;
            let pattern = dxf_aama::read(&text, Default::default())?;
            let pieces: Vec<_> = pattern
                .pieces
                .iter()
                .map(|p| {
                    let converted = dxf_aama::template::convert(p);
                    serde_json::json!({
                        "name": p.name,
                        "size": p.size,
                        "quantity": format!("{},{}", p.quantity.right, p.quantity.left),
                        "material": p.material,
                        "sew_line": p.sew.is_some(),
                        "notches": p.notches.len(),
                        "cutouts": p.cutouts.len(),
                        "internal_lines": p.internal.len(),
                        "grain_degrees": converted.as_ref().ok().map(|c| c.grain_degrees),
                        "boundaries": converted.as_ref().ok().map(|c| c.polygon.points.len()),
                        "size_mm": converted.as_ref().ok().map(|c| [c.size.x * 1000., c.size.y * 1000.]),
                        "notes": converted.as_ref().map(|c| c.notes.clone()).unwrap_or_else(|e| vec![e.clone()]),
                        "grade_points": p.grade_points.len(),
                    })
                })
                .collect();
            let issues: Vec<_> = pattern
                .issues
                .iter()
                .map(|i| {
                    serde_json::json!({"piece": i.piece, "layer": i.layer, "entity": i.entity,
                        "reason": i.reason, "count": i.count})
                })
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "dialect": pattern.dialect.label(),
                    "dialect_reason": pattern.dialect_reason,
                    "version": pattern.version,
                    "units": pattern.units.map(|u| u.label()),
                    "units_source": pattern.units_source,
                    "style": pattern.style,
                    "sample_size": pattern.sample_size,
                    "author": pattern.author,
                    "pieces": pieces,
                    "not_imported": issues,
                    "rule_table": grading::rul::beside(&file).map(|path| {
                        match fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| grading::rul::read(&t)) {
                            Ok(t) => serde_json::json!({"file": path.display().to_string(),
                                "name": t.name, "sizes": t.sizes, "sample": t.sample,
                                "rules": t.rules.len(), "notes": t.notes}),
                            Err(e) => serde_json::json!({"file": path.display().to_string(), "error": e}),
                        }
                    }),
                }))?
            );
        }
        Command::GradeSize {
            template,
            output,
            size,
            group,
        } => {
            let t = GarmentTemplate::load(&template)?;
            let g = t.grading.as_ref().ok_or("The garment is not graded")?;
            let group = match &group {
                Some(name) => g
                    .groups
                    .iter()
                    .find(|x| x.name.eq_ignore_ascii_case(name) || &x.id == name)
                    .ok_or_else(|| format!("No size group {name}"))?,
                None => &g.groups[0],
            };
            let id = group
                .sizes
                .iter()
                .find(|s| s.name.eq_ignore_ascii_case(&size) || s.id == size)
                .ok_or_else(|| format!("No size {size} in {}", group.name))?
                .id
                .clone();
            let sized = grading::apply::template_in_size(&t, &group.id, &id)?;
            sized.compile()?;
            fs::write(&output, serde_json::to_string_pretty(&sized)?)?;
            println!("{} in size {size} written to {}", t.name, output.display());
        }
        Command::Pom { template, csv } => {
            let t = GarmentTemplate::load(&template)?;
            if csv {
                print!("{}", grading::pom::csv(&t));
                return Ok(());
            }
            let g = t.grading.as_ref().ok_or("The garment is not graded")?;
            let rows: Vec<_> = grading::pom::table(&t)
                .into_iter()
                .map(|row| {
                    let group = g.group(&row.group).expect("listed");
                    let values: serde_json::Map<String, serde_json::Value> = row
                        .values
                        .iter()
                        .map(|(s, v)| {
                            let value = match v {
                                Ok(v) => serde_json::json!((v * 1e4).round() / 10.),
                                Err(e) => serde_json::json!(e),
                            };
                            (group.name_of(s).to_string(), value)
                        })
                        .collect();
                    serde_json::json!({"name": row.name, "group": group.name,
                        "base": group.base_name(), "tolerance_mm": row.tolerance_m * 1000.,
                        "values_mm": values})
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&rows)?);
        }
        Command::Simulate {
            template,
            output,
            steps,
            dt,
            threads,
        } => {
            if steps == 0 || !(1..=64).contains(&threads) {
                return Err("Require steps > 0 and 1..64 CPU threads".into());
            }
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build_global()?;
            let settings = SolverSettings {
                dt,
                ..Default::default()
            };
            settings.validate()?;
            let t = GarmentTemplate::load(&template)?;
            let g = t.compile()?;
            let mut state = g.initial_state.clone();
            let initial_gap = g.max_seam_gap(&state.x);
            fs::create_dir_all(&output)?;
            fs::write(output.join("initial.obj"), export::obj(&g, &state))?;
            let mut log = BufWriter::new(File::create(output.join("metrics.jsonl"))?);
            let mut converged = 0;
            let mut last = None;
            for _ in 0..steps {
                let metrics = ReferenceSolver::step_garment(&g, &mut state, &settings, None)?;
                converged += usize::from(metrics.converged);
                serde_json::to_writer(&mut log, &metrics)?;
                writeln!(log)?;
                last = Some(metrics);
            }
            log.flush()?;
            export::write_artifacts(&t, &g, &state, &output)?;
            let report = serde_json::json!({"backend":"cpu-f64","gpu_initialized":false,"threads":threads,"solver":settings,
                "steps":steps,"converged_steps":converged,"initial_seam_gap_m":initial_gap,"final_seam_gap_m":g.max_seam_gap(&state.x),
                "last_step":last,"warnings":g.warnings});
            fs::write(
                output.join("run.json"),
                serde_json::to_string_pretty(&report)?,
            )?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            if converged != steps {
                return Err(
                    "Some steps did not converge; inspect metrics.jsonl before using the result"
                        .into(),
                );
            }
        }
    }
    Ok(())
}
