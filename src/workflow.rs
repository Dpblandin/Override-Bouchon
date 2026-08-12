use std::path::Path;

use thiserror::Error;

use crate::catalog::BouchonCatalog;
use crate::deployer::{
    DeployError, DeploymentOutcome, HistoryEntry, RestorationOutcome, TARGET_NAME,
    create_history_entry, delete_existing_bouchons, deploy_bouchon, discard_history_entry,
    restore_latest_history,
};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use crate::platform::{self, ElevationError};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use crate::privileged::PrivilegedCommand;
use crate::validation::{ValidationError, validate_bouchon};

#[derive(Debug, Error)]
pub enum WorkflowError {
    #[error("{0}")]
    Validation(#[from] ValidationError),

    #[error("{0}")]
    Deployment(#[from] DeployError),

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[error("{0}")]
    Elevation(#[from] ElevationError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowOutcome<T> {
    Completed(T),
    Cancelled,
}

pub struct BouchonWorkflow<'a> {
    history_directory: &'a Path,
    catalog: &'a BouchonCatalog,
}

impl<'a> BouchonWorkflow<'a> {
    pub fn new(history_directory: &'a Path, catalog: &'a BouchonCatalog) -> Self {
        Self {
            history_directory,
            catalog,
        }
    }

    pub fn deploy(
        &self,
        source: &Path,
        target_directory: &Path,
    ) -> Result<WorkflowOutcome<DeploymentOutcome>, WorkflowError> {
        validate_bouchon(source)?;
        let history_entry =
            create_history_entry(target_directory, self.history_directory, self.catalog)?;

        match deploy_bouchon(source, target_directory) {
            Ok(outcome) => Ok(WorkflowOutcome::Completed(outcome)),
            Err(error) => {
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                if error.is_permission_denied() {
                    let command = PrivilegedCommand::Deploy {
                        source: source.to_path_buf(),
                        target_directory: target_directory.to_path_buf(),
                    };
                    return match platform::run_elevated(&command) {
                        Ok(replaced_files) => Ok(WorkflowOutcome::Completed(DeploymentOutcome {
                            target_path: target_directory.join(TARGET_NAME),
                            replaced_files,
                        })),
                        Err(ElevationError::Cancelled) => {
                            discard_history(history_entry.as_ref());
                            Ok(WorkflowOutcome::Cancelled)
                        }
                        Err(error) => {
                            discard_history(history_entry.as_ref());
                            Err(error.into())
                        }
                    };
                }

                discard_history(history_entry.as_ref());
                Err(error.into())
            }
        }
    }

    pub fn delete(&self, target_directory: &Path) -> Result<WorkflowOutcome<usize>, WorkflowError> {
        let history_entry =
            create_history_entry(target_directory, self.history_directory, self.catalog)?;

        match delete_existing_bouchons(target_directory) {
            Ok(deleted_files) => Ok(WorkflowOutcome::Completed(deleted_files)),
            Err(error) => {
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                if error.is_permission_denied() {
                    let command = PrivilegedCommand::Delete {
                        target_directory: target_directory.to_path_buf(),
                    };
                    return match platform::run_elevated(&command) {
                        Ok(deleted_files) => Ok(WorkflowOutcome::Completed(deleted_files)),
                        Err(ElevationError::Cancelled) => {
                            discard_history(history_entry.as_ref());
                            Ok(WorkflowOutcome::Cancelled)
                        }
                        Err(error) => {
                            discard_history(history_entry.as_ref());
                            Err(error.into())
                        }
                    };
                }

                discard_history(history_entry.as_ref());
                Err(error.into())
            }
        }
    }

    pub fn restore(
        &self,
        target_directory: &Path,
    ) -> Result<WorkflowOutcome<RestorationOutcome>, WorkflowError> {
        match restore_latest_history(target_directory, self.history_directory) {
            Ok(outcome) => Ok(WorkflowOutcome::Completed(outcome)),
            Err(error) => {
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                if error.is_permission_denied() {
                    let command = PrivilegedCommand::Restore {
                        history_directory: self.history_directory.to_path_buf(),
                        target_directory: target_directory.to_path_buf(),
                    };
                    return match platform::run_elevated(&command) {
                        Ok(restored_files) => Ok(WorkflowOutcome::Completed(RestorationOutcome {
                            restored_files,
                        })),
                        Err(ElevationError::Cancelled) => Ok(WorkflowOutcome::Cancelled),
                        Err(error) => Err(error.into()),
                    };
                }

                Err(error.into())
            }
        }
    }
}

fn discard_history(entry: Option<&HistoryEntry>) {
    if let Some(entry) = entry {
        let _ = discard_history_entry(entry);
    }
}
