use std::fs;

use bouchonneur::catalog::{BouchonCatalog, BouchonValidation};
use tempfile::tempdir;

#[test]
fn loads_regular_files_with_validation_in_stable_order() {
    let library = tempdir().expect("library directory");
    fs::write(library.path().join("z.json"), r#"{"ok":true}"#).expect("json");
    fs::write(library.path().join("A.xml"), "<ok />").expect("xml");
    fs::create_dir(library.path().join("nested")).expect("nested directory");

    let catalog = BouchonCatalog::load(library.path()).expect("catalog");

    assert_eq!(
        catalog
            .entries()
            .iter()
            .map(|entry| entry.name())
            .collect::<Vec<_>>(),
        ["A.xml", "z.json"]
    );
    assert!(matches!(
        catalog.entries()[0].validation(),
        BouchonValidation::Valid(_)
    ));
}

#[test]
fn matching_is_case_insensitive_and_keeps_catalog_indices() {
    let library = tempdir().expect("library directory");
    fs::write(library.path().join("CasNominal.xml"), "<ok />").expect("nominal");
    fs::write(library.path().join("bouchon_test_3.json"), "{}").expect("test");
    fs::write(library.path().join("autre.pdf"), "opaque").expect("other");
    let catalog = BouchonCatalog::load(library.path()).expect("catalog");

    assert_eq!(catalog.matching_indices("TEST_3"), vec![1]);
    assert_eq!(catalog.matching_indices("nominal"), vec![2]);
    assert_eq!(catalog.matching_indices("  "), vec![0, 1, 2]);
}

#[test]
fn identifies_a_deployed_file_by_exact_content() {
    let library = tempdir().expect("library directory");
    let target = tempdir().expect("target directory");
    fs::write(library.path().join("known.json"), "known response").expect("known");
    fs::write(target.path().join("active.do"), "known response").expect("active");
    let catalog = BouchonCatalog::load(library.path()).expect("catalog");

    assert_eq!(
        catalog
            .identify_content(&target.path().join("active.do"))
            .as_deref(),
        Some("known.json")
    );
}
