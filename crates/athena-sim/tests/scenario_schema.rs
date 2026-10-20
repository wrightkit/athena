//! `docs/scenario.schema.json` accepts the valid example and rejects every
//! invalid example.

use std::fs;
use std::path::PathBuf;

fn docs() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs")
}

fn read(path: PathBuf) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(&path).unwrap())
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn schema_accepts_valid_and_rejects_invalid_examples() {
    let schema = jsonschema::draft202012::new(&read(docs().join("scenario.schema.json")))
        .expect("schema is valid draft 2020-12");

    let valid = docs().join("scenario-examples/practice-range.json");
    let errors: Vec<_> = schema
        .iter_errors(&read(valid))
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "valid example rejected: {errors:?}");

    let mut invalid = 0;
    for entry in fs::read_dir(docs().join("scenario-examples/invalid")).unwrap() {
        let path = entry.unwrap().path();
        assert!(
            !schema.is_valid(&read(path.clone())),
            "{} passed",
            path.display()
        );
        invalid += 1;
    }
    assert!(invalid >= 3, "expected at least three invalid examples");
}
