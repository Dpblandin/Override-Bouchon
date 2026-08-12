use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::deployer::{
    DeployError, delete_existing_bouchons, deploy_bouchon, restore_latest_history,
};

const DEPLOY_ARGUMENT: &str = "--privileged-deploy";
const DELETE_ARGUMENT: &str = "--privileged-delete";
const RESTORE_ARGUMENT: &str = "--privileged-restore";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivilegedCommand {
    Deploy {
        source: PathBuf,
        target_directory: PathBuf,
    },
    Delete {
        target_directory: PathBuf,
    },
    Restore {
        history_directory: PathBuf,
        target_directory: PathBuf,
    },
}

impl PrivilegedCommand {
    pub fn to_cli_arguments(&self) -> Vec<OsString> {
        match self {
            Self::Deploy {
                source,
                target_directory,
            } => vec![
                OsString::from(DEPLOY_ARGUMENT),
                source.as_os_str().to_owned(),
                target_directory.as_os_str().to_owned(),
            ],
            Self::Delete { target_directory } => vec![
                OsString::from(DELETE_ARGUMENT),
                target_directory.as_os_str().to_owned(),
            ],
            Self::Restore {
                history_directory,
                target_directory,
            } => vec![
                OsString::from(RESTORE_ARGUMENT),
                history_directory.as_os_str().to_owned(),
                target_directory.as_os_str().to_owned(),
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivilegedInvocation {
    pub command: PrivilegedCommand,
    pub result_file: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivilegedResponse {
    Success(usize),
    Error(String),
}

pub fn parse_privileged_command(
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<Option<PrivilegedCommand>, String> {
    parse_privileged_invocation(arguments)
        .map(|invocation| invocation.map(|invocation| invocation.command))
}

pub fn parse_privileged_invocation(
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<Option<PrivilegedInvocation>, String> {
    let mut arguments = arguments.into_iter();
    let Some(command) = arguments.next() else {
        return Ok(None);
    };

    let parsed = if command == OsStr::new(DEPLOY_ARGUMENT) {
        PrivilegedCommand::Deploy {
            source: required_path(&mut arguments, "fichier source")?,
            target_directory: required_path(&mut arguments, "répertoire cible")?,
        }
    } else if command == OsStr::new(DELETE_ARGUMENT) {
        PrivilegedCommand::Delete {
            target_directory: required_path(&mut arguments, "répertoire cible")?,
        }
    } else if command == OsStr::new(RESTORE_ARGUMENT) {
        PrivilegedCommand::Restore {
            history_directory: required_path(&mut arguments, "répertoire d'historique")?,
            target_directory: required_path(&mut arguments, "répertoire cible")?,
        }
    } else {
        return Ok(None);
    };

    let result_file = match arguments.next() {
        Some(argument) if argument == OsStr::new("--result-file") => {
            Some(required_path(&mut arguments, "fichier de résultat")?)
        }
        Some(_) => return Err("Trop d'arguments pour l'opération privilégiée.".to_owned()),
        None => None,
    };

    if arguments.next().is_some() {
        return Err("Trop d'arguments pour l'opération privilégiée.".to_owned());
    }

    Ok(Some(PrivilegedInvocation {
        command: parsed,
        result_file,
    }))
}

pub fn write_privileged_response(path: &Path, response: &PrivilegedResponse) -> io::Result<()> {
    let contents = match response {
        PrivilegedResponse::Success(affected_files) => format!("success\n{affected_files}"),
        PrivilegedResponse::Error(message) => format!("error\n{message}"),
    };
    fs::write(path, contents)
}

pub fn read_privileged_response(path: &Path) -> Result<PrivilegedResponse, String> {
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("Impossible de lire le résultat administrateur : {error}"))?;
    let (status, payload) = contents
        .split_once('\n')
        .ok_or_else(|| "Réponse administrateur incomplète.".to_owned())?;

    match status {
        "success" => payload
            .trim()
            .parse()
            .map(PrivilegedResponse::Success)
            .map_err(|_| format!("Nombre de fichiers invalide : {payload}")),
        "error" => Ok(PrivilegedResponse::Error(payload.to_owned())),
        _ => Err(format!("Statut administrateur inconnu : {status}")),
    }
}

pub fn execute_privileged_command(command: PrivilegedCommand) -> Result<usize, DeployError> {
    match command {
        PrivilegedCommand::Deploy {
            source,
            target_directory,
        } => deploy_bouchon(&source, &target_directory).map(|outcome| outcome.replaced_files),
        PrivilegedCommand::Delete { target_directory } => {
            delete_existing_bouchons(&target_directory)
        }
        PrivilegedCommand::Restore {
            history_directory,
            target_directory,
        } => restore_latest_history(&target_directory, &history_directory)
            .map(|outcome| outcome.restored_files),
    }
}

fn required_path(
    arguments: &mut impl Iterator<Item = OsString>,
    name: &str,
) -> Result<PathBuf, String> {
    arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| format!("Argument manquant : {name}."))
}
