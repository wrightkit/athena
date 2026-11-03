//! `docs/scenario.schema.json` accepts every valid example and rejects every
//! invalid example.

use std::fs;
use std::path::PathBuf;

use athena_sim::{RunError, run_scenario};
use serde_json::{Value, json};

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

    for entry in fs::read_dir(docs().join("scenario-examples/valid")).unwrap() {
        let path = entry.unwrap().path();
        let errors: Vec<_> = schema
            .iter_errors(&read(path.clone()))
            .map(|e| e.to_string())
            .collect();
        assert!(errors.is_empty(), "{} rejected: {errors:?}", path.display());
    }

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

/// The schema fragments of one object, with `$ref` followed.
fn resolve<'a>(root: &'a Value, schema: &'a Value) -> &'a Value {
    match schema["$ref"].as_str() {
        Some(reference) => resolve(root, root.pointer(&reference[1..]).unwrap()),
        None => schema,
    }
}

/// The first value of `key` among the fragments.
fn first<'a>(fragments: &[&'a Value], key: &str) -> Option<&'a Value> {
    fragments.iter().find_map(|fragment| fragment.get(key))
}

/// Collect the paths of optional properties the schema types as an object or
/// an array. Array items are entered at index 0. The `if`/`then`/`else`
/// branches of `allOf` apply for `version`.
fn optional_structured(
    root: &Value,
    fragments: &[&Value],
    version: u64,
    path: &mut Vec<Value>,
    out: &mut Vec<Vec<Value>>,
) {
    let mut fragments = fragments.to_vec();
    for entry in fragments
        .iter()
        .flat_map(|f| f["allOf"].as_array().into_iter().flatten())
        .collect::<Vec<_>>()
    {
        let matches = entry["if"]["properties"]["scenarioVersion"]["const"] == version;
        let branch = if matches {
            &entry["then"]
        } else {
            &entry["else"]
        };
        fragments.push(branch);
    }
    let required: Vec<&str> = fragments
        .iter()
        .flat_map(|f| f["required"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .collect();
    let mut names: Vec<&String> = fragments
        .iter()
        .flat_map(|f| {
            f["properties"]
                .as_object()
                .into_iter()
                .flat_map(|p| p.keys())
        })
        .collect();
    names.sort();
    names.dedup();
    for name in names {
        let property: Vec<&Value> = fragments
            .iter()
            .filter_map(|f| f["properties"].get(name))
            .map(|schema| resolve(root, schema))
            .collect();
        let kind = first(&property, "type").and_then(Value::as_str);
        path.push(json!(name));
        match kind {
            Some("object") => {
                if !required.contains(&name.as_str()) {
                    out.push(path.clone());
                }
                optional_structured(root, &property, version, path, out);
            }
            Some("array") => {
                if !required.contains(&name.as_str()) {
                    out.push(path.clone());
                }
                if let Some(items) = first(&property, "items") {
                    path.push(json!(0));
                    optional_structured(root, &[resolve(root, items)], version, path, out);
                    path.pop();
                }
            }
            _ => {}
        }
        path.pop();
    }
}

/// Set the value at `path`, creating missing objects. An array on the path
/// must already hold its first element.
fn set_at(doc: &mut Value, path: &[Value], value: Value) {
    let mut node = doc;
    for (i, segment) in path.iter().enumerate() {
        let last = i + 1 == path.len();
        node = match segment {
            Value::String(key) if last => {
                node[key.as_str()] = value;
                return;
            }
            Value::String(key) => node
                .as_object_mut()
                .unwrap()
                .entry(key.as_str())
                .or_insert_with(|| json!({})),
            index => &mut node[index.as_u64().unwrap() as usize],
        };
    }
}

/// An explicit `null` on an optional object or array field is not the same
/// as leaving the field out: the loader refuses it (athena#126, #141). The
/// fields come from the schema, so a new one is covered without an edit.
#[test]
fn explicit_null_on_an_optional_structured_field_is_refused() {
    let schema = read(docs().join("scenario.schema.json"));
    let program = "rule (\"r\") { event { Ongoing - Global; } }";
    let mut checked = 0;
    for version in [1, 2] {
        // Version 2 renames the player variables from `state` to `vars`.
        let mut base = read(docs().join("scenario-examples/valid/practice-range.json"));
        if version == 2 {
            base["scenarioVersion"] = json!(2);
            let player = base["players"][0].as_object_mut().unwrap();
            let vars = player.remove("state").unwrap();
            player.insert("vars".into(), vars);
        }
        assert!(
            run_scenario(program, &base.to_string()).is_ok(),
            "version {version} base does not load"
        );
        let mut fields = Vec::new();
        optional_structured(&schema, &[&schema], version, &mut Vec::new(), &mut fields);
        for path in fields {
            let mut doc = base.clone();
            set_at(&mut doc, &path, Value::Null);
            match run_scenario(program, &doc.to_string()) {
                Err(RunError::Scenario(_)) => checked += 1,
                other => panic!(
                    "version {version}: {} set to null: {other:?}",
                    Value::Array(path)
                ),
            }
        }
    }
    assert!(checked >= 10, "the schema walk found only {checked} fields");
}
