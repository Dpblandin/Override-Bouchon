use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

use bouchonneur::catalog::BouchonCatalog;
use bouchonneur::deployer::{TARGET_NAME, create_history_entry};
use bouchonneur::privileged::{
    PrivilegedCommand, PrivilegedResponse, execute_privileged_command, parse_privileged_command,
    parse_privileged_invocation, read_privileged_response, write_privileged_response,
};
use tempfile::{NamedTempFile, tempdir};

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
fn typed_commands_round_trip_through_the_cli_codec() {
    let commands = [
        PrivilegedCommand::Deploy {
            source: PathBuf::from("/tmp/source with spaces.json"),
            target_directory: PathBuf::from("/tmp/target with spaces"),
        },
        PrivilegedCommand::Delete {
            target_directory: PathBuf::from("/tmp/target with spaces"),
        },
        PrivilegedCommand::Restore {
            history_directory: PathBuf::from("/tmp/history with spaces"),
            target_directory: PathBuf::from("/tmp/target with spaces"),
        },
    ];

    for command in commands {
        assert_eq!(
            parse_privileged_command(command.to_cli_arguments())
                .expect("valid arguments")
                .expect("privileged command"),
            command
        );
    }
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
fn parses_result_file_for_elevated_invocation() {
    let invocation = parse_privileged_invocation([
        OsString::from("--privileged-delete"),
        OsString::from("C:\\Program Files (x86)\\DmpConnect-JS2"),
        OsString::from("--result-file"),
        OsString::from("C:\\Users\\tester\\AppData\\Local\\Temp\\result.txt"),
    ])
    .expect("valid invocation")
    .expect("privileged invocation");

    assert_eq!(
        invocation.command,
        PrivilegedCommand::Delete {
            target_directory: PathBuf::from("C:\\Program Files (x86)\\DmpConnect-JS2"),
        }
    );
    assert_eq!(
        invocation.result_file,
        Some(PathBuf::from(
            "C:\\Users\\tester\\AppData\\Local\\Temp\\result.txt"
        ))
    );
}

#[test]
fn privileged_response_round_trips_success_and_multiline_errors() {
    let result_file = NamedTempFile::new().expect("result file");

    for response in [
        PrivilegedResponse::Success(3),
        PrivilegedResponse::Error("Accès refusé\nVérifiez les permissions".to_owned()),
    ] {
        write_privileged_response(result_file.path(), &response).expect("write response");
        assert_eq!(
            read_privileged_response(result_file.path()).expect("read response"),
            response
        );
    }
}

#[test]
fn executes_privileged_restore_command() {
    let target = tempdir().expect("target directory");
    let history = tempdir().expect("history directory");
    let active = target.path().join(TARGET_NAME);
    fs::write(&active, "previous response").expect("previous bouchon");
    create_history_entry(target.path(), history.path(), &BouchonCatalog::new(""))
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
