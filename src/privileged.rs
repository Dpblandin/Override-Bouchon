use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use crate::deployer::{DeployError, delete_existing_bouchons, deploy_bouchon};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivilegedCommand {
    Deploy {
        source: PathBuf,
        target_directory: PathBuf,
    },
    Delete {
        target_directory: PathBuf,
    },
}

pub fn parse_privileged_command(
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<Option<PrivilegedCommand>, String> {
    let mut arguments = arguments.into_iter();
    let Some(command) = arguments.next() else {
        return Ok(None);
    };

    let parsed = if command == OsStr::new("--privileged-deploy") {
        PrivilegedCommand::Deploy {
            source: required_path(&mut arguments, "fichier source")?,
            target_directory: required_path(&mut arguments, "répertoire cible")?,
        }
    } else if command == OsStr::new("--privileged-delete") {
        PrivilegedCommand::Delete {
            target_directory: required_path(&mut arguments, "répertoire cible")?,
        }
    } else {
        return Ok(None);
    };

    if arguments.next().is_some() {
        return Err("Trop d'arguments pour l'opération privilégiée.".to_owned());
    }

    Ok(Some(parsed))
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
