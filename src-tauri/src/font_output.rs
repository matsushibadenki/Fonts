//! Preserve source basenames when exporting an existing font.
use std::path::Path;
pub fn stem(source: &Path) -> String {
    source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Font".into())
}
pub fn filename(source: &Path, extension: &str) -> String {
    format!("{}.{}", stem(source), extension)
}
pub fn same_source(destination: &Path, source: &Path) -> bool {
    destination
        .canonicalize()
        .ok()
        .zip(source.canonicalize().ok())
        .is_some_and(|(a, b)| a == b)
}
