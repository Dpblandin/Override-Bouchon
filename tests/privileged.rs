use std::ffi::OsString;
use std::path::PathBuf;

use bouchonneur::privileged::{PrivilegedCommand, parse_privileged_command};

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
