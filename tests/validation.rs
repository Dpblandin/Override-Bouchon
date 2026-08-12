use std::fs;

use bouchonneur::validation::{ValidatedFormat, ValidationOutcome, validate_bouchon};
use tempfile::tempdir;

#[test]
fn accepts_well_formed_json_and_xml() {
    let directory = tempdir().expect("temporary directory");
    let json = directory.path().join("response.JSON");
    let xml = directory.path().join("response.xml");
    fs::write(&json, br#"{"status":"ok"}"#).expect("write json");
    fs::write(&xml, "<response><status>ok</status></response>").expect("write xml");

    assert_eq!(
        validate_bouchon(&json).expect("valid json"),
        ValidationOutcome::Valid(ValidatedFormat::Json)
    );
    assert_eq!(
        validate_bouchon(&xml).expect("valid xml"),
        ValidationOutcome::Valid(ValidatedFormat::Xml)
    );
}

#[test]
fn rejects_empty_or_malformed_structured_files() {
    let directory = tempdir().expect("temporary directory");
    let empty = directory.path().join("empty.er7");
    let json = directory.path().join("invalid.json");
    let xml = directory.path().join("invalid.xml");
    fs::write(&empty, "  \n\t").expect("write empty file");
    fs::write(&json, br#"{"status":}"#).expect("write invalid json");
    fs::write(&xml, "<response><status></response>").expect("write invalid xml");

    assert!(
        validate_bouchon(&empty)
            .unwrap_err()
            .to_string()
            .contains("vide")
    );
    assert!(
        validate_bouchon(&json)
            .unwrap_err()
            .to_string()
            .contains("JSON est invalide")
    );
    assert!(
        validate_bouchon(&xml)
            .unwrap_err()
            .to_string()
            .contains("XML est invalide")
    );
}

#[test]
fn allows_non_empty_opaque_formats_without_claiming_validation() {
    let directory = tempdir().expect("temporary directory");
    let er7 = directory.path().join("message.er7");
    let extensionless = directory.path().join("response");
    fs::write(&er7, "MSH|^~\\&|APP").expect("write er7");
    fs::write(&extensionless, "opaque response").expect("write extensionless file");

    assert_eq!(
        validate_bouchon(&er7).expect("unchecked er7"),
        ValidationOutcome::Unchecked {
            extension: Some("er7".to_owned())
        }
    );
    assert_eq!(
        validate_bouchon(&extensionless).expect("unchecked extensionless file"),
        ValidationOutcome::Unchecked { extension: None }
    );
}
