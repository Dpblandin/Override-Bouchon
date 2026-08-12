use std::fs;
use std::io;
use std::path::PathBuf;

use bouchonneur::deployer::{
    DeployError, TARGET_NAME, delete_existing_bouchons, deploy_bouchon, list_bouchons,
};
use tempfile::tempdir;

#[test]
fn lists_only_regular_files_in_stable_order() {
    let root = tempdir().expect("temp directory");
    fs::write(root.path().join("z.json"), "z").expect("write z");
    fs::write(root.path().join("A.xml"), "a").expect("write a");
    fs::create_dir(root.path().join("nested")).expect("create nested directory");

    let names: Vec<_> = list_bouchons(root.path())
        .expect("list bouchons")
        .into_iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();

    assert_eq!(names, ["A.xml", "z.json"]);
}

#[test]
fn deploys_selected_file_and_replaces_every_do_file() {
    let source_directory = tempdir().expect("source directory");
    let target_directory = tempdir().expect("target directory");
    let source = source_directory.path().join("bouchon.json");
    fs::write(&source, "nouveau bouchon").expect("write source");
    fs::write(target_directory.path().join("ancien.do"), "ancien").expect("write old do");
    fs::write(target_directory.path().join("garder.txt"), "garder").expect("write retained file");

    let outcome = deploy_bouchon(&source, target_directory.path()).expect("deploy bouchon");

    assert_eq!(outcome.replaced_files, 1);
    assert_eq!(
        fs::read_to_string(target_directory.path().join(TARGET_NAME)).expect("read target"),
        "nouveau bouchon"
    );
    assert!(!target_directory.path().join("ancien.do").exists());
    assert!(target_directory.path().join("garder.txt").exists());
}

#[test]
fn invalid_source_leaves_existing_bouchon_untouched() {
    let source_directory = tempdir().expect("source directory");
    let target_directory = tempdir().expect("target directory");
    let existing = target_directory.path().join(TARGET_NAME);
    fs::write(&existing, "ancien").expect("write existing bouchon");

    let result = deploy_bouchon(
        &source_directory.path().join("absent.json"),
        target_directory.path(),
    );

    assert!(result.is_err());
    assert_eq!(
        fs::read_to_string(existing).expect("read existing"),
        "ancien"
    );
}

#[test]
fn explicitly_deletes_all_do_files_only() {
    let target_directory = tempdir().expect("target directory");
    fs::write(target_directory.path().join("one.do"), "one").expect("write one");
    fs::write(target_directory.path().join("two.DO"), "two").expect("write two");
    fs::write(target_directory.path().join("keep.json"), "keep").expect("write keep");

    let deleted = delete_existing_bouchons(target_directory.path()).expect("delete bouchons");

    assert_eq!(deleted, 2);
    assert!(!target_directory.path().join("one.do").exists());
    assert!(!target_directory.path().join("two.DO").exists());
    assert!(target_directory.path().join("keep.json").exists());
}

#[test]
fn identifies_permission_denied_errors_for_targeted_elevation() {
    let error = DeployError::Io {
        action: "écrire",
        path: PathBuf::from("/protected/directory"),
        source: io::Error::from(io::ErrorKind::PermissionDenied),
    };

    assert!(error.is_permission_denied());
}
