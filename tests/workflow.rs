use std::fs;

use bouchonneur::catalog::BouchonCatalog;
use bouchonneur::deployer::{TARGET_NAME, create_history_entry, list_history_entries};
use bouchonneur::workflow::{BouchonWorkflow, WorkflowError, WorkflowOutcome};
use tempfile::tempdir;

#[test]
fn deploy_validates_saves_and_replaces_as_one_workflow() {
    let library = tempdir().expect("library directory");
    let target = tempdir().expect("target directory");
    let history = tempdir().expect("history directory");
    let previous = library.path().join("previous.json");
    let next = library.path().join("next.json");
    fs::write(&previous, r#"{"version":"previous"}"#).expect("previous bouchon");
    fs::write(&next, r#"{"version":"next"}"#).expect("next bouchon");
    fs::write(
        target.path().join(TARGET_NAME),
        fs::read(&previous).unwrap(),
    )
    .expect("active bouchon");

    let catalog = BouchonCatalog::load(library.path()).expect("catalog");
    let outcome = BouchonWorkflow::new(history.path(), &catalog)
        .deploy(&next, target.path())
        .expect("deployment workflow");

    assert!(matches!(outcome, WorkflowOutcome::Completed(_)));
    assert_eq!(
        fs::read_to_string(target.path().join(TARGET_NAME)).expect("deployed content"),
        r#"{"version":"next"}"#
    );
    assert_eq!(
        list_history_entries(target.path(), history.path(), &catalog)
            .expect("history")
            .len(),
        1
    );
}

#[test]
fn invalid_bouchon_stops_before_history_is_created() {
    let library = tempdir().expect("library directory");
    let target = tempdir().expect("target directory");
    let history = tempdir().expect("history directory");
    let invalid = library.path().join("invalid.json");
    fs::write(&invalid, r#"{"broken":}"#).expect("invalid bouchon");
    fs::write(target.path().join(TARGET_NAME), "previous").expect("active bouchon");

    let catalog = BouchonCatalog::load(library.path()).expect("catalog");
    let result = BouchonWorkflow::new(history.path(), &catalog).deploy(&invalid, target.path());

    assert!(matches!(result, Err(WorkflowError::Validation(_))));
    assert!(
        list_history_entries(target.path(), history.path(), &catalog)
            .expect("history")
            .is_empty()
    );
}

#[test]
fn delete_saves_the_active_bouchon_before_removing_it() {
    let target = tempdir().expect("target directory");
    let history = tempdir().expect("history directory");
    fs::write(target.path().join(TARGET_NAME), "active").expect("active bouchon");

    let catalog = BouchonCatalog::new("");
    let outcome = BouchonWorkflow::new(history.path(), &catalog)
        .delete(target.path())
        .expect("delete workflow");

    assert_eq!(outcome, WorkflowOutcome::Completed(1));
    assert!(!target.path().join(TARGET_NAME).exists());
    assert_eq!(
        list_history_entries(target.path(), history.path(), &catalog)
            .expect("history")
            .len(),
        1
    );
}

#[test]
fn restore_consumes_the_latest_history_entry() {
    let target = tempdir().expect("target directory");
    let history = tempdir().expect("history directory");
    let active = target.path().join(TARGET_NAME);
    fs::write(&active, "previous").expect("previous bouchon");
    let catalog = BouchonCatalog::new("");
    create_history_entry(target.path(), history.path(), &catalog)
        .expect("history creation")
        .expect("history entry");
    fs::write(&active, "current").expect("current bouchon");

    let outcome = BouchonWorkflow::new(history.path(), &catalog)
        .restore(target.path())
        .expect("restore workflow");

    assert_eq!(
        outcome,
        WorkflowOutcome::Completed(bouchonneur::deployer::RestorationOutcome { restored_files: 1 })
    );
    assert_eq!(
        fs::read_to_string(active).expect("restored content"),
        "previous"
    );
}
