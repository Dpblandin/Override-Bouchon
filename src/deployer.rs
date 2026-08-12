use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tempfile::{Builder, NamedTempFile};
use thiserror::Error;

pub const TARGET_NAME: &str = "overrideinsianswer.do";
const MAX_HISTORY_ENTRIES: usize = 20;

#[derive(Debug, Error)]
pub enum DeployError {
    #[error("Le fichier bouchon n'existe pas ou n'est pas un fichier : {0}")]
    InvalidSource(PathBuf),

    #[error("Le répertoire DmpConnect-JS2 est invalide : {0}")]
    InvalidTargetDirectory(PathBuf),

    #[error("Aucune version précédente n'est disponible pour : {0}")]
    NoHistory(PathBuf),

    #[error("La sauvegarde de bouchon est invalide : {0}")]
    InvalidHistory(PathBuf),

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveBouchon {
    pub deployed_path: PathBuf,
    pub source_name: Option<String>,
    pub modified_at: Option<SystemTime>,
    pub do_file_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub directory: PathBuf,
    pub created_at: SystemTime,
    pub source_name: Option<String>,
    pub file_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestorationOutcome {
    pub restored_files: usize,
}

impl DeployError {
    pub fn is_permission_denied(&self) -> bool {
        matches!(
            self,
            Self::Io { source, .. } if source.kind() == io::ErrorKind::PermissionDenied
        )
    }
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

pub fn resolve_history_dir() -> PathBuf {
    if let Some(configured) = env::var_os("BOUCHONNEUR_HISTORY_DIR") {
        return PathBuf::from(configured);
    }

    #[cfg(target_os = "windows")]
    if let Some(root) = env::var_os("LOCALAPPDATA") {
        return PathBuf::from(root).join("Bouchonneur").join("history");
    }

    #[cfg(target_os = "macos")]
    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("Bouchonneur")
            .join("history");
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Some(root) = env::var_os("XDG_DATA_HOME") {
            return PathBuf::from(root).join("Bouchonneur").join("history");
        }
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("Bouchonneur")
                .join("history");
        }
    }

    env::temp_dir().join("Bouchonneur").join("history")
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

pub fn detect_active_bouchon(
    target_directory: &Path,
    known_bouchons: &[PathBuf],
) -> Result<Option<ActiveBouchon>, DeployError> {
    validate_target_directory(target_directory)?;
    let files = list_do_files(target_directory)?;
    let Some(deployed_path) = preferred_do_file(&files) else {
        return Ok(None);
    };

    Ok(Some(ActiveBouchon {
        source_name: find_matching_bouchon_name(deployed_path, known_bouchons),
        modified_at: fs::metadata(deployed_path)
            .and_then(|metadata| metadata.modified())
            .ok(),
        deployed_path: deployed_path.to_path_buf(),
        do_file_count: files.len(),
    }))
}

pub fn create_history_entry(
    target_directory: &Path,
    history_root: &Path,
    known_bouchons: &[PathBuf],
) -> Result<Option<HistoryEntry>, DeployError> {
    validate_target_directory(target_directory)?;
    let files = list_do_files(target_directory)?;
    if files.is_empty() {
        return Ok(None);
    }

    let target_history = target_history_directory(history_root, target_directory);
    fs::create_dir_all(&target_history)
        .map_err(|source| io_error("créer l'historique dans", &target_history, source))?;
    let (entry_directory, created_at) = create_timestamped_directory(&target_history)?;

    for file in &files {
        let destination = entry_directory.join(file.file_name().unwrap_or_default());
        if let Err(source) = fs::copy(file, &destination) {
            let _ = fs::remove_dir_all(&entry_directory);
            return Err(io_error("sauvegarder", file, source));
        }
    }

    let source_name =
        preferred_do_file(&files).and_then(|path| find_matching_bouchon_name(path, known_bouchons));
    let entry = HistoryEntry {
        directory: entry_directory,
        created_at,
        source_name,
        file_count: files.len(),
    };

    let _ = prune_history(&target_history);
    Ok(Some(entry))
}

pub fn list_history_entries(
    target_directory: &Path,
    history_root: &Path,
    known_bouchons: &[PathBuf],
) -> Result<Vec<HistoryEntry>, DeployError> {
    let target_history = target_history_directory(history_root, target_directory);
    if !target_history.is_dir() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(&target_history)
        .map_err(|source| io_error("lire l'historique dans", &target_history, source))?;
    let mut history = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|source| io_error("lire", &target_history, source))?;
        let directory = entry.path();
        if !directory.is_dir() {
            continue;
        }
        let Some(created_at) = timestamp_from_directory(&directory) else {
            continue;
        };
        let files = list_do_files(&directory)?;
        if files.is_empty() {
            continue;
        }
        let source_name = preferred_do_file(&files)
            .and_then(|path| find_matching_bouchon_name(path, known_bouchons));
        history.push(HistoryEntry {
            directory,
            created_at,
            source_name,
            file_count: files.len(),
        });
    }

    history.sort_by_key(|entry| std::cmp::Reverse(entry.created_at));
    Ok(history)
}

pub fn discard_history_entry(entry: &HistoryEntry) -> Result<(), DeployError> {
    if !entry.directory.exists() {
        return Ok(());
    }
    fs::remove_dir_all(&entry.directory)
        .map_err(|source| io_error("supprimer la sauvegarde", &entry.directory, source))
}

pub fn restore_latest_history(
    target_directory: &Path,
    history_root: &Path,
) -> Result<RestorationOutcome, DeployError> {
    validate_target_directory(target_directory)?;
    let history = list_history_entries(target_directory, history_root, &[])?;
    let entry = history
        .first()
        .ok_or_else(|| DeployError::NoHistory(target_directory.to_path_buf()))?;
    let history_files = list_do_files(&entry.directory)?;
    if history_files.is_empty() {
        return Err(DeployError::InvalidHistory(entry.directory.clone()));
    }

    let mut staged_files = Vec::new();
    for history_file in &history_files {
        let staged = stage_source(history_file, target_directory)?;
        staged_files.push((
            history_file.file_name().unwrap_or_default().to_owned(),
            staged,
        ));
    }

    let existing_files = list_do_files(target_directory)?;
    let backup = Builder::new()
        .prefix(".bouchonneur-restore-")
        .tempdir_in(target_directory)
        .map_err(|source| io_error("préparer la restauration dans", target_directory, source))?;
    let moved_files = move_to_backup(&existing_files, backup.path())?;
    let mut installed_files = Vec::new();

    for (filename, staged) in staged_files {
        let target_path = target_directory.join(filename);
        if let Err(error) = staged.persist(&target_path) {
            for installed in installed_files.iter().rev() {
                let _ = fs::remove_file(installed);
            }
            restore_backups(&moved_files);
            return Err(io_error("restaurer", &target_path, error.error));
        }
        installed_files.push(target_path);
    }

    drop(backup);
    let _ = discard_history_entry(entry);

    Ok(RestorationOutcome {
        restored_files: history_files.len(),
    })
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

    files.sort_by_cached_key(|path| {
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase()
    });
    Ok(files)
}

fn preferred_do_file(files: &[PathBuf]) -> Option<&Path> {
    files
        .iter()
        .find(|path| {
            path.file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case(TARGET_NAME))
        })
        .or_else(|| files.first())
        .map(PathBuf::as_path)
}

fn find_matching_bouchon_name(active_file: &Path, known_bouchons: &[PathBuf]) -> Option<String> {
    let active_metadata = fs::metadata(active_file).ok()?;
    let active_content = fs::read(active_file).ok()?;

    known_bouchons.iter().find_map(|candidate| {
        let same_length =
            fs::metadata(candidate).is_ok_and(|metadata| metadata.len() == active_metadata.len());
        if !same_length || fs::read(candidate).ok().as_deref() != Some(active_content.as_slice()) {
            return None;
        }
        candidate
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
    })
}

fn target_history_directory(history_root: &Path, target_directory: &Path) -> PathBuf {
    let canonical = target_directory
        .canonicalize()
        .unwrap_or_else(|_| target_directory.to_path_buf());
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in canonical.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    history_root.join(format!("{hash:016x}"))
}

fn create_timestamped_directory(
    target_history: &Path,
) -> Result<(PathBuf, SystemTime), DeployError> {
    let mut timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    loop {
        let directory = target_history.join(format!("{timestamp:020}"));
        match fs::create_dir(&directory) {
            Ok(()) => {
                return Ok((directory, UNIX_EPOCH + Duration::from_millis(timestamp)));
            }
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => timestamp += 1,
            Err(source) => return Err(io_error("créer la sauvegarde", &directory, source)),
        }
    }
}

fn timestamp_from_directory(directory: &Path) -> Option<SystemTime> {
    let milliseconds = directory.file_name()?.to_str()?.parse().ok()?;
    Some(UNIX_EPOCH + Duration::from_millis(milliseconds))
}

fn prune_history(target_history: &Path) -> Result<(), DeployError> {
    let mut directories = fs::read_dir(target_history)
        .map_err(|source| io_error("lire l'historique dans", target_history, source))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && timestamp_from_directory(path).is_some())
        .collect::<Vec<_>>();
    directories.sort();

    let obsolete_count = directories.len().saturating_sub(MAX_HISTORY_ENTRIES);
    for directory in directories.into_iter().take(obsolete_count) {
        fs::remove_dir_all(&directory)
            .map_err(|source| io_error("nettoyer l'historique", &directory, source))?;
    }
    Ok(())
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
        candidates.push(PathBuf::from("/usr/local/dmpconnectjs2"));
        candidates.push(PathBuf::from("/usr/local/DmpConnect-JS2"));
    }

    candidates
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn macos_candidates_include_the_standard_installation_directory() {
        assert!(dmpconnect_candidates().contains(&PathBuf::from("/usr/local/dmpconnectjs2")));
    }
}
