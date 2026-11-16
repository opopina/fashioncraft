//! CPU-only document I/O. Commit bytes in the destination directory before replacement.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    temporary
        .write_all(bytes)
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|e| e.to_string())?;
    temporary.persist(path).map_err(|e| e.to_string())?;
    // Directory sync is supported on Unix; Windows replacement is handled by tempfile.
    #[cfg(unix)]
    fs::File::open(parent)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Content-addressed OBJ next to the project. Existing asset names are never overwritten.
/// Copy the project and its `.assets` directory together to relocate it.
pub fn portable_avatar(source: &Path, project: &Path) -> Result<PathBuf, String> {
    portable_asset(source, project, "avatar", "obj")
}

/// Content-addressed reference image next to the project, keeping the source's lowercase
/// extension (PNG when it has none). Same rules as [`portable_avatar`].
pub fn portable_image(source: &Path, project: &Path) -> Result<PathBuf, String> {
    let extension = source
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .filter(|e| !e.is_empty() && e.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or_else(|| "png".into());
    portable_asset(source, project, "image", &extension)
}

fn portable_asset(
    source: &Path,
    project: &Path,
    kind: &str,
    extension: &str,
) -> Result<PathBuf, String> {
    let bytes =
        fs::read(source).map_err(|e| format!("Cannot read {kind} {}: {e}", source.display()))?;
    let hash = bytes.iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    });
    let stem = project
        .file_name()
        .ok_or("Project needs a filename")?
        .to_string_lossy();
    let relative =
        PathBuf::from(format!("{stem}.assets")).join(format!("{kind}-{hash:016x}.{extension}"));
    let destination = project.parent().unwrap_or(Path::new(".")).join(&relative);
    fs::create_dir_all(destination.parent().unwrap()).map_err(|e| e.to_string())?;
    if destination.exists() {
        if fs::read(&destination).map_err(|e| e.to_string())? != bytes {
            let mut label = kind.to_string();
            label[..1].make_ascii_uppercase();
            return Err(format!(
                "{label} asset hash collision; existing asset was preserved"
            ));
        }
    } else {
        atomic_write(&destination, &bytes)?;
    }
    Ok(relative)
}
