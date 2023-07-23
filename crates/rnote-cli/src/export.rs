use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Context;
use clap::{ArgAction, Subcommand, ValueEnum};
use rnote_compose::helpers::SplitOrder;
use rnote_engine::{
    engine::{
        export::{
            DocExportFormat, DocExportPrefs, DocPagesExportFormat, DocPagesExportPrefs,
            SelectionExportFormat, SelectionExportPrefs,
        },
        EngineSnapshot,
    },
    RnoteEngine,
};
use smol::{
    fs::File,
    io::{AsyncReadExt, AsyncWriteExt},
};

use crate::cli::OnConflict;
use crate::validators;

#[derive(Subcommand, Clone, Debug)]
pub(crate) enum ExportCommands {
    /// Export entire document
    Doc {
        /// the export output file. Only allows for one input file. Exclusive with output-format.
        #[arg(short = 'o', long, conflicts_with("output_file"), required(true))]
        output_file: Option<PathBuf>,
        /// the export output format. Exclusive with output-file.
        #[arg(short = 'f', long, conflicts_with("output_file"), required(true))]
        output_format: Option<DocOutputFormat>,
        /// if pagaes are exported horizontal or vertical first
        #[arg(short = 'P', long)]
        page_order: Option<PageOrder>,
    },
    /// Export pages of document individually
    DocPages {
        /// the folder the pages get exported to
        #[arg(short = 'o', long, value_parser = validators::path_is_dir)]
        output_dir: PathBuf,
        /// the folder the pages get exported to
        #[arg(short = 's', long)]
        output_file_stem: Option<String>,
        /// the export output format. Exclusive with output-file.
        #[arg(short = 'f', long)]
        output_format: DocPagesOutputFormat,
        /// if pagaes are exported horizontal or vertical first
        #[arg(short = 'P', long)]
        page_order: Option<PageOrder>,
        /// bitmap scale factor in relation to the actual size on the document
        #[arg(long)]
        bitmap_scalefactor: Option<f64>,
        /// quality of the jpeg image
        #[arg(long)]
        jpeg_quality: Option<u8>,
    },
    /// Export selection
    Selection {
        /// the export output file. Only allows for one input file. Exclusive with output-format.
        #[arg(short = 'o', long, conflicts_with("output_file"), required(true))]
        output_file: Option<PathBuf>,
        /// the export output format. Exclusive with output-file.
        #[arg(short = 'f', long, conflicts_with("output_file"), required(true))]
        output_format: Option<SelectionOutputFormat>,
        /// export all strokes
        #[arg(short = 'a', long, action = ArgAction::SetTrue)]
        all: bool,
        /// bitmap scale factor in relation to the actual size on the document
        #[arg(long)]
        bitmap_scalefactor: Option<f64>,
        /// quality of the jpeg image
        #[arg(long)]
        jpeg_quality: Option<u8>,
        /// margin around the document
        #[arg(long)]
        margin: Option<f64>,
    },
}

#[derive(ValueEnum, Clone, Debug)]
pub(crate) enum DocOutputFormat {
    Pdf,
    Xopp,
    Svg,
}

impl From<&DocOutputFormat> for DocExportFormat {
    fn from(val: &DocOutputFormat) -> Self {
        match val {
            DocOutputFormat::Pdf => DocExportFormat::Pdf,
            DocOutputFormat::Xopp => DocExportFormat::Xopp,
            DocOutputFormat::Svg => DocExportFormat::Svg,
        }
    }
}

#[derive(ValueEnum, Clone, Debug)]
pub(crate) enum SelectionOutputFormat {
    Svg,
    Png,
    Jpeg,
}

impl From<&SelectionOutputFormat> for SelectionExportFormat {
    fn from(val: &SelectionOutputFormat) -> Self {
        match val {
            SelectionOutputFormat::Svg => SelectionExportFormat::Svg,
            SelectionOutputFormat::Png => SelectionExportFormat::Png,
            SelectionOutputFormat::Jpeg => SelectionExportFormat::Jpeg,
        }
    }
}

#[derive(ValueEnum, Clone, Debug)]
pub(crate) enum DocPagesOutputFormat {
    Svg,
    Png,
    Jpeg,
}

impl From<&DocPagesOutputFormat> for DocPagesExportFormat {
    fn from(val: &DocPagesOutputFormat) -> Self {
        match val {
            DocPagesOutputFormat::Svg => DocPagesExportFormat::Svg,
            DocPagesOutputFormat::Png => DocPagesExportFormat::Png,
            DocPagesOutputFormat::Jpeg => DocPagesExportFormat::Jpeg,
        }
    }
}

#[derive(ValueEnum, Clone, Debug)]
pub(crate) enum PageOrder {
    Horizontal,
    Vertical,
}

impl From<&PageOrder> for SplitOrder {
    fn from(val: &PageOrder) -> Self {
        match val {
            PageOrder::Horizontal => SplitOrder::RowMajor,
            PageOrder::Vertical => SplitOrder::ColumnMajor,
        }
    }
}

pub(crate) async fn run_export(
    export_commands: ExportCommands,
    engine: &mut RnoteEngine,
    rnote_files: Vec<PathBuf>,
    without_background: bool,
    without_pattern: bool,
    on_conflict: OnConflict,
) -> anyhow::Result<()> {
    let output_file: Option<PathBuf>;
    // apply given arguments to export prefs
    match &export_commands {
        ExportCommands::Doc {
            output_file: out_file,
            page_order,
            output_format,
        } => {
            output_file = out_file.clone();
            engine.export_prefs.doc_export_prefs = create_doc_export_prefs_from_args(
                output_file.as_deref(),
                output_format.as_ref(),
                without_background,
                without_pattern,
                page_order.as_ref(),
            )?
        }
        ExportCommands::DocPages {
            output_format,
            page_order,
            bitmap_scalefactor,
            jpeg_quality,
            ..
        } => {
            engine.export_prefs.doc_pages_export_prefs = create_doc_pages_export_prefs_from_args(
                output_format,
                page_order.as_ref(),
                without_background,
                without_pattern,
                *bitmap_scalefactor,
                *jpeg_quality,
            )?;
            output_file = None
        }
        ExportCommands::Selection {
            output_file: out_file,
            bitmap_scalefactor,
            jpeg_quality,
            margin,
            output_format,
            ..
        } => {
            output_file = out_file.clone();
            engine.export_prefs.selection_export_prefs = create_selection_export_prefs_from_args(
                output_file.as_deref(),
                output_format.as_ref(),
                without_background,
                without_pattern,
                *bitmap_scalefactor,
                *jpeg_quality,
                *margin,
            )?
        }
    }
    match output_file {
        Some(ref output_file) => {
            match rnote_files.get(0) {
                Some(rnote_file) => {
                    let new_path = check_file_conflict(output_file, &on_conflict)?;
                    // Replace output file if suffix added
                    let output_file = new_path.as_ref().unwrap_or(output_file);

                    if rnote_files.len() > 1 {
                        return Err(anyhow::anyhow!("Was expecting only 1 file. Use --output-format when exporting multiple files."));
                    }

                    let rnote_file_disp = rnote_file.display().to_string();
                    let output_file_disp = output_file.display().to_string();
                    let pb = indicatif::ProgressBar::new_spinner().with_message(format!(
                        "Exporting \"{rnote_file_disp}\" to: \"{output_file_disp}\""
                    ));
                    pb.set_draw_target(indicatif::ProgressDrawTarget::stdout());
                    pb.enable_steady_tick(Duration::from_millis(8));

                    // export
                    if let Err(e) = export_to_file(
                        engine,
                        rnote_file,
                        output_file,
                        &export_commands,
                        &on_conflict,
                    )
                    .await
                    {
                        let msg = format!("Export \"{rnote_file_disp}\" to: \"{output_file_disp}\" failed, Err {e:?}");
                        if pb.is_hidden() {
                            println!("{msg}")
                        }
                        pb.abandon_with_message(msg);
                        return Err(e);
                    } else {
                        let msg = format!(
                            "Export \"{rnote_file_disp}\" to: \"{output_file_disp}\" succeeded"
                        );
                        if pb.is_hidden() {
                            println!("{msg}")
                        }
                        pb.finish_with_message(msg);
                    }
                }
                None => return Err(anyhow::anyhow!("Failed to get filename from rnote_files")),
            }
        }

        None => {
            let doc_pages = matches!(export_commands, ExportCommands::DocPages { .. });
            let output_files = rnote_files
                .iter()
                .map(|file| {
                    let mut output = file.clone();
                    output.set_extension(match &export_commands {
                        ExportCommands::Doc { .. } => engine
                            .export_prefs
                            .doc_export_prefs
                            .export_format
                            .file_ext(),
                        ExportCommands::DocPages { .. } => engine
                            .export_prefs
                            .doc_pages_export_prefs
                            .export_format
                            .file_ext(),
                        ExportCommands::Selection { .. } => engine
                            .export_prefs
                            .selection_export_prefs
                            .export_format
                            .file_ext(),
                    });
                    output
                })
                .collect::<Vec<PathBuf>>();

            for (rnote_file, output_file) in rnote_files.iter().zip(output_files.iter()) {
                let new_path = match check_file_conflict(output_file, &on_conflict) {
                    Ok(r) => r,
                    Err(e) => {
                        println!("{e}");
                        continue;
                    }
                };
                let output_file = new_path.as_ref().unwrap_or(output_file);

                let rnote_file_disp = rnote_file.display().to_string();
                let output_file_disp = output_file.display().to_string();
                let pb = indicatif::ProgressBar::new_spinner();
                pb.set_draw_target(indicatif::ProgressDrawTarget::stdout());
                pb.enable_steady_tick(Duration::from_millis(8));
                pb.set_message(match doc_pages {
                    true => format!("Exporting \"{rnote_file_disp}\""),
                    false => format!("Exporting \"{rnote_file_disp}\" to: \"{output_file_disp}\""),
                });

                // export
                if let Err(e) = export_to_file(
                    engine,
                    &rnote_file,
                    output_file,
                    &export_commands,
                    &on_conflict,
                )
                .await
                {
                    let msg = format!(
                        "Export \"{rnote_file_disp}\" to: \"{output_file_disp}\" failed, Err {e:?}"
                    );
                    if pb.is_hidden() {
                        println!("{msg}")
                    }
                    pb.abandon_with_message(msg);
                    return Err(e);
                } else {
                    let msg = match doc_pages {
                        false => format!(
                            "Export \"{rnote_file_disp}\" to: \"{output_file_disp}\" succeeded"
                        ),
                        true => format!("Export \"{rnote_file_disp}\" succeeded"),
                    };
                    if pb.is_hidden() {
                        println!("{msg}")
                    }
                    pb.finish_with_message(msg);
                }
            }
        }
    }

    println!("Export Finished!");
    Ok(())
}

pub(crate) fn create_doc_export_prefs_from_args(
    output_file: Option<impl AsRef<Path>>,
    output_format: Option<&DocOutputFormat>,
    without_background: bool,
    without_pattern: bool,
    page_order: Option<&PageOrder>,
) -> anyhow::Result<DocExportPrefs> {
    let format = match (output_file, output_format) {
        (Some(file), None) => match file.as_ref().extension().and_then(|ext| ext.to_str()) {
            Some(extension) => get_doc_export_format(extension),
            None => {
                return Err(anyhow::anyhow!(
                    "Output file needs to have an extension to determine the file type"
                ))
            }
        },
        (None, Some(out_format)) => Ok(out_format.into()),
        // unreachable because they are exclusive (conflicts_with)
        (Some(_), Some(_)) => {
            return Err(anyhow::anyhow!(
                "--output-file and --output-format are mutually exclusive."
            ))
        }
        // unreachable because they are required
        (None, None) => {
            return Err(anyhow::anyhow!(
                "--output-file or --output-format is required."
            ))
        }
    }?;

    let mut prefs = DocExportPrefs {
        export_format: format,
        with_background: !without_background,
        with_pattern: !without_pattern,
        ..Default::default()
    };

    if let Some(page_order) = page_order {
        prefs.page_order = page_order.into();
    }
    Ok(prefs)
}

fn get_doc_export_format(format: &str) -> anyhow::Result<DocExportFormat> {
    match format {
        "svg" => Ok(DocExportFormat::Svg),
        "xopp" => Ok(DocExportFormat::Xopp),
        "pdf" => Ok(DocExportFormat::Pdf),
        ext => Err(anyhow::anyhow!(
            "Could not create doc export prefs, unsupported export file extension `{ext}`"
        )),
    }
}
pub(crate) fn create_selection_export_prefs_from_args(
    output_file: Option<impl AsRef<Path>>,
    output_format: Option<&SelectionOutputFormat>,
    without_background: bool,
    without_pattern: bool,
    bitmap_scalefactor: Option<f64>,
    jpeg_quality: Option<u8>,
    margin: Option<f64>,
) -> anyhow::Result<SelectionExportPrefs> {
    let format = match (output_file, output_format) {
        (Some(file), None) => match file.as_ref().extension().and_then(|ext| ext.to_str()) {
            Some(extension) => get_selection_export_format(extension),
            None => {
                return Err(anyhow::anyhow!(
                    "Output file needs to have an extension to determine the file type"
                ))
            }
        },
        (None, Some(out_format)) => Ok(out_format.into()),
        // unreachable because they are exclusive (conflicts_with)
        (Some(_), Some(_)) => {
            return Err(anyhow::anyhow!(
                "--output-file and --output-format are mutually exclusive."
            ))
        }
        // unreachable because they are required
        (None, None) => {
            return Err(anyhow::anyhow!(
                "--output-file or --output-format is required."
            ))
        }
    }?;

    let mut prefs = SelectionExportPrefs {
        export_format: format,
        with_background: !without_background,
        with_pattern: !without_pattern,
        ..Default::default()
    };

    if let Some(bitmap_scalefactor) = bitmap_scalefactor {
        prefs.bitmap_scalefactor = bitmap_scalefactor.clamp(0.1, 10.0);
    }
    if let Some(jpeg_quality) = jpeg_quality {
        prefs.jpeg_quality = jpeg_quality.clamp(1, 100);
    }
    if let Some(margin) = margin {
        prefs.margin = margin.clamp(0.0, 1000.0);
    }

    Ok(prefs)
}

fn get_selection_export_format(format: &str) -> anyhow::Result<SelectionExportFormat> {
    match format {
        "svg" => Ok(SelectionExportFormat::Svg),
        "png" => Ok(SelectionExportFormat::Png),
        "jpg" | "jpeg" => Ok(SelectionExportFormat::Jpeg),
        ext => Err(anyhow::anyhow!(
            "Could not create selection export prefs, unsupported export file extension `{ext}`"
        )),
    }
}

pub(crate) fn create_doc_pages_export_prefs_from_args(
    output_format: &DocPagesOutputFormat,
    page_order: Option<&PageOrder>,
    without_background: bool,
    without_pattern: bool,
    bitmap_scalefactor: Option<f64>,
    jpeg_quality: Option<u8>,
) -> anyhow::Result<DocPagesExportPrefs> {
    let mut prefs = DocPagesExportPrefs {
        export_format: output_format.into(),
        with_background: !without_background,
        with_pattern: !without_pattern,
        ..Default::default()
    };

    if let Some(page_order) = page_order {
        prefs.page_order = page_order.into();
    }
    if let Some(bitmap_scalefactor) = bitmap_scalefactor {
        prefs.bitmap_scalefactor = bitmap_scalefactor.clamp(0.1, 10.0);
    }
    if let Some(jpeg_quality) = jpeg_quality {
        prefs.jpeg_quality = jpeg_quality.clamp(1, 100);
    }

    Ok(prefs)
}

pub(crate) fn check_file_conflict(
    output_file: &Path,
    mut on_conflict: &OnConflict,
) -> anyhow::Result<Option<PathBuf>> {
    if !output_file.exists() {
        return Ok(None);
    }
    while matches!(on_conflict, OnConflict::Ask) {
        let options = &[
            OnConflict::Ask,
            OnConflict::Overwrite,
            OnConflict::Skip,
            OnConflict::Suffix,
        ];
        match dialoguer::Select::new()
            .with_prompt(format!("File {} already exits:", output_file.display()))
            .items(options)
            .interact()
        {
            Ok(0) => {
                if let Err(e) = open::that(output_file) {
                    println!(
                        "Failed to open {} with default program, {e}",
                        output_file.display()
                    );
                }
            }
            Ok(c) => on_conflict = &options[c],
            Err(e) => {
                return Err(anyhow::anyhow!(
                "Failed to show select promt, retry or select an behavior with --on-conflict, {e}"
            ))
            }
        };
    }
    match on_conflict {
        OnConflict::Ask => Err(anyhow::anyhow!("Failed to save user choice!")),
        OnConflict::Overwrite => Ok(None),
        OnConflict::Skip => Err(anyhow::anyhow!("Skipped {}", output_file.display())),
        OnConflict::Suffix => {
            let mut i = 0;
            let mut new_path = output_file.to_path_buf();
            let Some(file_stem) = new_path.file_stem().map(|s| s.to_string_lossy().to_string()) else {
                return Err(anyhow::anyhow!("Failed to get file stem"));
            };
            let ext = new_path
                .extension()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or(String::new());
            while new_path.exists() {
                i += 1;
                new_path.set_file_name(format!("{file_stem}_{i}.{ext}"))
            }
            Ok(Some(new_path))
        }
    }
}

pub(crate) async fn export_to_file(
    engine: &mut RnoteEngine,
    rnote_file: impl AsRef<Path>,
    output_file: impl AsRef<Path>,
    export_commands: &ExportCommands,
    on_conflict: &OnConflict,
) -> anyhow::Result<()> {
    match export_commands {
        ExportCommands::Doc { .. } | ExportCommands::Selection { .. } => {
            let Some(export_file_name) = output_file.as_ref().file_name().map(|s| s.to_string_lossy().to_string()) else {
                return Err(anyhow::anyhow!("Failed to get filename from output_file"));
            };

            let mut rnote_bytes = vec![];
            File::open(rnote_file)
                .await?
                .read_to_end(&mut rnote_bytes)
                .await?;

            let engine_snapshot = EngineSnapshot::load_from_rnote_bytes(rnote_bytes).await?;
            let _ = engine.load_snapshot(engine_snapshot);

            // We applied the prefs previously to the engine
            let export_bytes = match export_commands {
                ExportCommands::Selection { all, .. } => {
                    if *all {
                        let all_strokes = engine.store.stroke_keys_unordered();
                        if all_strokes.is_empty() {
                            return Err(anyhow::anyhow!(
                                "Cannot export empty document with --crop-to-content enabled"
                            ));
                        }
                        engine.store.set_selected_keys(&all_strokes, true);
                    }
                    engine
                        .export_selection(None)
                        .await??
                        .context("Failed to export selection")?
                }
                ExportCommands::Doc { .. } => engine.export_doc(export_file_name, None).await??,
                ExportCommands::DocPages { .. } => unreachable!(),
            };
            let mut fh = File::create(output_file).await?;
            fh.write_all(&export_bytes).await?;
            fh.sync_all().await?;
        }
        ExportCommands::DocPages {
            output_dir,
            output_file_stem,
            output_format,
            ..
        } => {
            // The output file cannnot be set here
            drop(output_file);
            let rnote_file = rnote_file.as_ref();
            let export_bytes = engine.export_doc_pages(None).await??;
            let out_ext = DocPagesExportFormat::from(output_format).file_ext();
            let output_file_stem = match output_file_stem {
                Some(o) => o.clone(),
                None => match rnote_file.file_stem() {
                    Some(stem) => stem.to_string_lossy().to_string(),
                    None => {
                        return Err(anyhow::anyhow!(
                            "Failed to generate output_file_stem from rnote_file"
                        ))
                    }
                },
            };
            for (i, bytes) in export_bytes.into_iter().enumerate() {
                export_doc_page(
                    i,
                    output_dir,
                    &output_file_stem,
                    &out_ext,
                    &bytes,
                    on_conflict,
                )
                .await
                .context(format!(
                    "Failed to export page {i} of document {}",
                    rnote_file.display()
                ))?
            }
        }
    };
    Ok(())
}

async fn export_doc_page(
    i: usize,
    output_dir: &Path,
    output_file_stem: &str,
    out_ext: &str,
    bytes: &[u8],
    on_conflict: &OnConflict,
) -> anyhow::Result<()> {
    let mut output_file = output_dir.join(format!(
        "{output_file_stem} - page {}{}.{out_ext}",
        if i + 1 < 10 { "0" } else { "" },
        i + 1,
    ));
    if let Some(new_out) = check_file_conflict(&output_file, on_conflict)? {
        output_file = new_out;
    }
    let mut fh = File::create(output_file).await?;
    fh.write_all(bytes).await?;
    fh.sync_all().await?;
    Ok(())
}