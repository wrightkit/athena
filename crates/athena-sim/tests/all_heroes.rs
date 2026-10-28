//! `All Heroes` yielding the catalog's `Hero` members in catalog order
//! (athena#69): the elements are the member-id texts `Hero Of` and a
//! `Hero` literal yield, so they compare equal to both. The client's
//! element order is unmeasured; the catalog order is the modeled order.

use athena_sim::{Outcome, Val, run_scenario, run_text};

fn run(text: &str) -> athena_sim::Trace {
    run_text(text, athena_sim::Options { seconds: 1.0 }).unwrap_or_else(|error| panic!("{error}"))
}

/// One `Ongoing - Global` rule that writes `g` once at tick 0.
fn set_global(value: &str) -> String {
    format!(
        r#"
variables {{
    global:
        0: g
}}

rule ("probe") {{
    event {{
        Ongoing - Global;
    }}
    actions {{
        Set Global Variable(g, {value});
    }}
}}
"#
    )
}

fn global(trace: &athena_sim::Trace) -> &Val {
    assert_eq!(trace.outcome, Outcome::Complete);
    &trace.globals["g"]
}

/// The catalog's `Hero` member ids in catalog order.
fn member_ids() -> Vec<Val> {
    workshop_rs::catalog::Catalog::builtin_ref()
        .unwrap_or_else(|error| panic!("{error}"))
        .enum_domain("Hero")
        .unwrap_or_else(|| panic!("no `Hero` domain"))
        .members
        .iter()
        .map(|member| Val::Text(member.member.clone()))
        .collect()
}

#[test]
fn all_heroes_yields_the_hero_members_in_catalog_order() {
    let trace = run(&set_global("All Heroes()"));
    assert_eq!(global(&trace), &Val::Array(member_ids()));
}

/// The boundary elements: the first member is the catalog's first, the
/// last is its last, and the count is the member count.
#[test]
fn the_boundary_elements_are_the_catalog_ends() {
    let members = member_ids();
    let trace = run(&set_global("First Of(All Heroes())"));
    assert_eq!(global(&trace), members.first().unwrap());
    let trace = run(&set_global(
        "Value In Array(All Heroes(), Count Of(All Heroes()) - 1)",
    ));
    assert_eq!(global(&trace), members.last().unwrap());
    let trace = run(&set_global("Count Of(All Heroes())"));
    assert_eq!(global(&trace), &Val::number(members.len() as f64));
}

/// A member id compares equal to the `Hero` literal and to `Hero Of`'s
/// yield, so `Array Contains`/`Index Of Array Value` find both.
#[test]
fn the_elements_compare_equal_to_hero_literals_and_hero_of() {
    let trace = run(&set_global("Array Contains(All Heroes(), Hero(Ana))"));
    assert_eq!(global(&trace), &Val::Bool(true));

    let index = member_ids()
        .iter()
        .position(|member| *member == Val::Text("ANA".to_string()))
        .unwrap() as f64;
    let trace = run(&set_global("Index Of Array Value(All Heroes(), Hero(Ana))"));
    assert_eq!(global(&trace), &Val::number(index));

    // `Hero Of` on a scenario player yields the member id `All Heroes`
    // lists, so the read holds.
    let scenario = r#"{
        "scenarioVersion": 1, "seconds": 1,
        "players": [{ "id": "p1", "hero": "Ana" }]
    }"#;
    let text = r#"
variables {
    global:
        0: g
}

rule ("probe") {
    event {
        Ongoing - Each Player;
        All;
        All;
    }
    actions {
        Set Global Variable(g, Array Contains(All Heroes(), Hero Of(Event Player)));
    }
}
"#;
    let trace = run_scenario(text, scenario).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(trace.outcome, Outcome::Complete);
    assert_eq!(trace.globals["g"], Val::Bool(true));
}
