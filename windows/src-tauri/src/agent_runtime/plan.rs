//! Pure planning: never mutates the filesystem. Targets are observations, not approval.
use serde::Serialize;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

pub fn category(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "pdf" | "doc" | "docx" | "txt" | "csv" | "xls" | "xlsx" | "ppt" | "pptx" => "Documentos",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" => "Imagens",
        "mp4" | "mkv" | "mov" | "avi" => "Videos",
        "mp3" | "wav" | "flac" | "ogg" => "Audio",
        "zip" | "7z" | "rar" | "tar" | "gz" => "Arquivos",
        _ => "Outros",
    }
}
pub fn identity(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").to_lowercase()
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedFile {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub conflict: bool,
}
pub fn plan_files(
    root: &Path,
    files: Vec<PathBuf>,
    occupied: &HashSet<String>,
) -> Vec<PlannedFile> {
    let mut reserved = occupied.clone();
    files
        .into_iter()
        .filter_map(|source| {
            let destination = root.join(category(&source)).join(source.file_name()?);
            let key = identity(&destination);
            let conflict = !reserved.insert(key);
            Some(PlannedFile {
                source,
                destination,
                conflict,
            })
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conflicts_are_case_insensitive_and_never_renamed_or_overwritten() {
        let root = Path::new("test-root");
        let occupied = HashSet::from([identity(&root.join("Documentos/report.PDF"))]);
        let plan = plan_files(
            root,
            vec![
                root.join("report.pdf"),
                root.join("photo.PNG"),
                root.join("PHOTO.png"),
            ],
            &occupied,
        );
        assert!(plan[0].conflict);
        assert!(!plan[1].conflict);
        assert!(plan[2].conflict);
        assert_eq!(plan[0].destination, root.join("Documentos/report.pdf"));
    }
}
