use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

use bouchonneur::deployer::{TARGET_NAME, create_history_entry};
use bouchonneur::privileged::{
    PrivilegedCommand, execute_privileged_command, parse_privileged_command,
};
use tempfile::tempdir;

#[test]
fn parses_privileged_deploy_command() {
    let command = parse_privileged_command([
        OsString::from("--privileged-deploy"),
        OsString::from("/tmp/source with spaces.json"),
        OsString::from("/tmp/target with spaces"),
    ])
    .expect("valid command")
    .expect("privileged command");

    assert_eq!(
        command,
        PrivilegedCommand::Deploy {
            source: PathBuf::from("/tmp/source with spaces.json"),
            target_directory: PathBuf::from("/tmp/target with spaces"),
        }
    );
}

#[test]
fn parses_privileged_delete_command() {
    let command = parse_privileged_command([
        OsString::from("--privileged-delete"),
        OsString::from("/tmp/target"),
    ])
    .expect("valid command")
    .expect("privileged command");

    assert_eq!(
        command,
        PrivilegedCommand::Delete {
            target_directory: PathBuf::from("/tmp/target"),
        }
    );
}

#[test]
fn parses_privileged_restore_command() {
    let command = parse_privileged_command([
        OsString::from("--privileged-restore"),
        OsString::from("/tmp/history with spaces"),
        OsString::from("/tmp/target"),
    ])
    .expect("valid command")
    .expect("privileged command");

    assert_eq!(
        command,
        PrivilegedCommand::Restore {
            history_directory: PathBuf::from("/tmp/history with spaces"),
            target_directory: PathBuf::from("/tmp/target"),
        }
    );
}

#[test]
fn rejects_incomplete_privileged_command() {
    let error = parse_privileged_command([OsString::from("--privileged-deploy")])
        .expect_err("missing arguments must fail");

    assert!(error.contains("fichier source"));
}

#[test]
fn ignores_regular_application_arguments() {
    assert_eq!(
        parse_privileged_command([OsString::from("--some-eframe-argument")])
            .expect("regular argument"),
        None
    );
}

#[test]
fn executes_privileged_restore_command() {
    let target = tempdir().expect("target directory");
    let history = tempdir().expect("history directory");
    let active = target.path().join(TARGET_NAME);
    fs::write(&active, "previous response").expect("previous bouchon");
    create_history_entry(target.path(), history.path(), &[])
        .expect("history creation")
        .expect("history entry");
    fs::write(&active, "current response").expect("current bouchon");

    let restored_files = execute_privileged_command(PrivilegedCommand::Restore {
        history_directory: history.path().to_path_buf(),
        target_directory: target.path().to_path_buf(),
    })
    .expect("privileged restoration");

    assert_eq!(restored_files, 1);
    assert_eq!(
        fs::read_to_string(active).expect("restored content"),
        "previous response"
    );
}
