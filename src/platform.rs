use std::io;
use std::path::Path;
use std::process::Command;

#[cfg(target_os = "macos")]
use std::env;
#[cfg(target_os = "macos")]
use thiserror::Error;

#[cfg(target_os = "macos")]
const ADMINISTRATOR_SCRIPT: &str = include_str!("../assets/macos/elevate.applescript");

#[cfg(target_os = "macos")]
#[derive(Debug, Error)]
pub enum ElevationError {
    #[error("L'authentification administrateur a été annulée.")]
    Cancelled,

    #[error("Impossible de préparer ou lancer l'opération administrateur : {0}")]
    Io(#[from] io::Error),

    #[error("L'opération administrateur a échoué : {0}")]
    Failed(String),

    #[error("La réponse de l'opération administrateur est invalide : {0}")]
    InvalidOutput(String),
}

pub fn open_path(path: &Path) -> io::Result<()> {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", ""]);
        command.arg(path);
        command
    };

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(path);
        command
    };

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        command
    };

    command.spawn().map(|_| ())
}

#[cfg(target_os = "macos")]
pub fn deploy_with_administrator_privileges(
    source: &Path,
    target_directory: &Path,
) -> Result<usize, ElevationError> {
    run_with_administrator_privileges("deploy", Some(source), target_directory)
}

#[cfg(target_os = "macos")]
pub fn delete_with_administrator_privileges(
    target_directory: &Path,
) -> Result<usize, ElevationError> {
    run_with_administrator_privileges("delete", None, target_directory)
}

#[cfg(target_os = "macos")]
fn run_with_administrator_privileges(
    operation: &str,
    source: Option<&Path>,
    target_directory: &Path,
) -> Result<usize, ElevationError> {
    let executable = env::current_exe()?.canonicalize()?;
    let target_directory = target_directory.canonicalize()?;
    let source = source.map(Path::canonicalize).transpose()?;

    let output = Command::new("/usr/bin/osascript")
        .args(["-e", ADMINISTRATOR_SCRIPT])
        .env("BOUCHONNEUR_EXECUTABLE", executable.as_os_str())
        .env("BOUCHONNEUR_OPERATION", operation)
        .env(
            "BOUCHONNEUR_SOURCE",
            source.as_deref().unwrap_or_else(|| Path::new("")),
        )
        .env("BOUCHONNEUR_TARGET", target_directory.as_os_str())
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if output.status.success() {
        return stdout
            .parse()
            .map_err(|_| ElevationError::InvalidOutput(stdout));
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if stderr.contains("(-128)") {
        Err(ElevationError::Cancelled)
    } else {
        Err(ElevationError::Failed(stderr))
    }
}
