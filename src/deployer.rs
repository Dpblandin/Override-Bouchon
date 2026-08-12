use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use tempfile::{Builder, NamedTempFile};
use thiserror::Error;

pub const TARGET_NAME: &str = "overrideinsianswer.do";

#[derive(Debug, Error)]
pub enum DeployError {
    #[error("Le fichier bouchon n'existe pas ou n'est pas un fichier : {0}")]
    InvalidSource(PathBuf),

    #[error("Le répertoire DmpConnect-JS2 est invalide : {0}")]
    InvalidTargetDirectory(PathBuf),

    #[error("Impossible de {action} '{path}' : {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeploymentOutcome {
    pub target_path: PathBuf,
    pub replaced_files: usize,
}

pub fn resolve_bouchon_dir() -> PathBuf {
    if let Some(configured) = env::var_os("BOUCHON_DIR") {
        return PathBuf::from(configured);
    }

    let executable_dir = env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));

    #[cfg(debug_assertions)]
    {
        let development_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bouchons");
        if development_dir.is_dir() {
            return development_dir;
        }
    }

    executable_dir
        .unwrap_or_else(|| PathBuf::from("."))
        .join("bouchons")
}

pub fn detect_dmpconnect_dir() -> Option<PathBuf> {
    dmpconnect_candidates()
        .into_iter()
        .find(|path| path.is_dir())
}

pub fn list_bouchons(directory: &Path) -> Result<Vec<PathBuf>, DeployError> {
    if !directory.is_dir() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(directory).map_err(|source| io_error("lire", directory, source))?;
    let mut files = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|source| io_error("lire", directory, source))?;
        let path = entry.path();
        if path.is_file() {
            files.push(path);
        }
    }

    files.sort_by_cached_key(|path| {
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase()
    });
    Ok(files)
}

pub fn deploy_bouchon(
    source: &Path,
    target_directory: &Path,
) -> Result<DeploymentOutcome, DeployError> {
    validate_source(source)?;
    validate_target_directory(target_directory)?;

    let staged = stage_source(source, target_directory)?;
    let existing_files = list_do_files(target_directory)?;
    let backup = Builder::new()
        .prefix(".bouchonneur-backup-")
        .tempdir_in(target_directory)
        .map_err(|source| io_error("préparer la sauvegarde dans", target_directory, source))?;

    let moved_files = move_to_backup(&existing_files, backup.path())?;
    let target_path = target_directory.join(TARGET_NAME);

    if let Err(error) = staged.persist(&target_path) {
        let persist_error = error.error;
        restore_backups(&moved_files);
        return Err(io_error("installer", &target_path, persist_error));
    }

    drop(backup);

    Ok(DeploymentOutcome {
        target_path,
        replaced_files: existing_files.len(),
    })
}

pub fn delete_existing_bouchons(target_directory: &Path) -> Result<usize, DeployError> {
    validate_target_directory(target_directory)?;
    let existing_files = list_do_files(target_directory)?;

    if existing_files.is_empty() {
        return Ok(0);
    }

    let trash = Builder::new()
        .prefix(".bouchonneur-delete-")
        .tempdir_in(target_directory)
        .map_err(|source| io_error("préparer la suppression dans", target_directory, source))?;
    let moved_files = move_to_backup(&existing_files, trash.path())?;

    if let Err(source) = trash.close() {
        restore_backups(&moved_files);
        return Err(io_error(
            "supprimer les anciens bouchons dans",
            target_directory,
            source,
        ));
    }

    Ok(existing_files.len())
}

fn validate_source(source: &Path) -> Result<(), DeployError> {
    if source.is_file() {
        Ok(())
    } else {
        Err(DeployError::InvalidSource(source.to_path_buf()))
    }
}

fn validate_target_directory(target_directory: &Path) -> Result<(), DeployError> {
    if target_directory.is_dir() {
        Ok(())
    } else {
        Err(DeployError::InvalidTargetDirectory(
            target_directory.to_path_buf(),
        ))
    }
}

fn stage_source(source: &Path, target_directory: &Path) -> Result<NamedTempFile, DeployError> {
    let staged = Builder::new()
        .prefix(".bouchonneur-stage-")
        .suffix(".tmp")
        .tempfile_in(target_directory)
        .map_err(|source| io_error("créer un fichier temporaire dans", target_directory, source))?;

    fs::copy(source, staged.path()).map_err(|error| io_error("copier", staged.path(), error))?;
    staged
        .as_file()
        .sync_all()
        .map_err(|source| io_error("synchroniser", staged.path(), source))?;

    Ok(staged)
}

fn list_do_files(directory: &Path) -> Result<Vec<PathBuf>, DeployError> {
    let entries = fs::read_dir(directory).map_err(|source| io_error("lire", directory, source))?;
    let mut files = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|source| io_error("lire", directory, source))?;
        let path = entry.path();
        let is_do_file = path.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("do"));

        if is_do_file {
            files.push(path);
        }
    }

    Ok(files)
}

fn move_to_backup(
    files: &[PathBuf],
    backup_directory: &Path,
) -> Result<Vec<(PathBuf, PathBuf)>, DeployError> {
    let mut moved = Vec::new();

    for original in files {
        let backup_path = backup_directory.join(original.file_name().unwrap_or_default());
        if let Err(source) = fs::rename(original, &backup_path) {
            restore_backups(&moved);
            return Err(io_error("mettre de côté", original, source));
        }
        moved.push((original.clone(), backup_path));
    }

    Ok(moved)
}

fn restore_backups(moved_files: &[(PathBuf, PathBuf)]) {
    for (original, backup) in moved_files.iter().rev() {
        let _ = fs::rename(backup, original);
    }
}

fn io_error(action: &'static str, path: &Path, source: io::Error) -> DeployError {
    DeployError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}

fn dmpconnect_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    #[cfg(target_os = "windows")]
    {
        for variable in ["LOCALAPPDATA", "ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(root) = env::var_os(variable) {
                candidates.push(PathBuf::from(root).join("DmpConnect-JS2"));
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = env::var_os("HOME") {
            candidates.push(
                PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
                    .join("DmpConnect-JS2"),
            );
        }
        candidates.push(PathBuf::from("/Applications/DmpConnect-JS2"));
        candidates.push(PathBuf::from("/usr/local/DmpConnect-JS2"));
    }

    candidates
}
