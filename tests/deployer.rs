use std::fs;
use std::io;
use std::path::PathBuf;

use bouchonneur::catalog::BouchonCatalog;
use bouchonneur::deployer::{
    DeployError, TARGET_NAME, create_history_entry, delete_existing_bouchons, deploy_bouchon,
    detect_active_bouchon, discard_history_entry, finalize_history_entry, list_history_entries,
    restore_latest_history,
};
use tempfile::tempdir;

#[test]
fn lists_only_regular_files_in_stable_order() {
    let root = tempdir().expect("temp directory");
    fs::write(root.path().join("z.json"), "z").expect("write z");
    fs::write(root.path().join("A.xml"), "a").expect("write a");
    fs::create_dir(root.path().join("nested")).expect("create nested directory");

    let names: Vec<_> = BouchonCatalog::load(root.path())
        .expect("load catalog")
        .entries()
        .iter()
        .map(|entry| entry.name().to_owned())
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

#[test]
fn identifies_the_active_bouchon_by_its_content() {
    let target = tempdir().expect("target directory");
    let library = tempdir().expect("bouchon library");
    let known = library.path().join("cas_nominal.json");
    fs::write(&known, "known response").expect("known bouchon");
    fs::write(target.path().join(TARGET_NAME), "known response").expect("active bouchon");

    let catalog = BouchonCatalog::load(library.path()).expect("catalog");
    let active = detect_active_bouchon(target.path(), &catalog)
        .expect("active detection")
        .expect("active bouchon");

    assert_eq!(active.source_name.as_deref(), Some("cas_nominal.json"));
    assert_eq!(active.do_file_count, 1);
}

#[test]
fn saves_and_lists_the_current_bouchon_in_history() {
    let target = tempdir().expect("target directory");
    let history = tempdir().expect("history directory");
    let library = tempdir().expect("bouchon library");
    let known = library.path().join("previous.xml");
    fs::write(&known, "previous response").expect("known bouchon");
    fs::write(target.path().join(TARGET_NAME), "previous response").expect("active bouchon");

    let catalog = BouchonCatalog::load(library.path()).expect("catalog");
    create_history_entry(target.path(), history.path(), &catalog)
        .expect("history creation")
        .expect("history entry");
    let entries =
        list_history_entries(target.path(), history.path(), &catalog).expect("history listing");

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].file_count, 1);
    assert_eq!(
        fs::read_to_string(entries[0].directory.join(TARGET_NAME)).expect("saved content"),
        "previous response"
    );
}

#[test]
fn restores_and_consumes_the_latest_history_entry() {
    let target = tempdir().expect("target directory");
    let history = tempdir().expect("history directory");
    let active = target.path().join(TARGET_NAME);
    fs::write(&active, "previous response").expect("previous bouchon");
    let catalog = BouchonCatalog::new("");
    create_history_entry(target.path(), history.path(), &catalog)
        .expect("history creation")
        .expect("history entry");
    fs::write(&active, "current response").expect("current bouchon");

    let outcome = restore_latest_history(target.path(), history.path()).expect("restoration");

    assert_eq!(outcome.restored_files, 1);
    assert_eq!(
        fs::read_to_string(&active).expect("restored content"),
        "previous response"
    );
    assert!(
        list_history_entries(target.path(), history.path(), &catalog)
            .expect("history listing")
            .is_empty()
    );
}

#[test]
fn prunes_history_only_after_an_operation_is_finalized() {
    let target = tempdir().expect("target directory");
    let history = tempdir().expect("history directory");
    let active = target.path().join(TARGET_NAME);
    let catalog = BouchonCatalog::new("");

    for version in 0..20 {
        fs::write(&active, format!("version {version}")).expect("active bouchon");
        create_history_entry(target.path(), history.path(), &catalog)
            .expect("history creation")
            .expect("history entry");
    }

    fs::write(&active, "provisional version").expect("provisional bouchon");
    let provisional = create_history_entry(target.path(), history.path(), &catalog)
        .expect("provisional history creation")
        .expect("provisional history entry");
    assert_eq!(
        list_history_entries(target.path(), history.path(), &catalog)
            .expect("history before cancellation")
            .len(),
        21
    );

    discard_history_entry(&provisional).expect("discard provisional history");
    assert_eq!(
        list_history_entries(target.path(), history.path(), &catalog)
            .expect("history after cancellation")
            .len(),
        20
    );

    let completed = create_history_entry(target.path(), history.path(), &catalog)
        .expect("completed history creation")
        .expect("completed history entry");
    finalize_history_entry(&completed).expect("finalize completed history");
    assert_eq!(
        list_history_entries(target.path(), history.path(), &catalog)
            .expect("history after completion")
            .len(),
        20
    );
    assert!(completed.directory.exists());
}
