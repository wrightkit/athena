//! Every enum-valued value read the catalog lists is either modeled and
//! compares equal to its enum literal through `Compare`, or is declared
//! unmodeled and ends the run before producing a value (athena#142).
//! `Hero Of` and `Team Of` once returned the scenario's authored
//! spelling, so the comparison answered false (athena#55, athena#61).
//!
//! The read set is the `workshop-rs` catalog's value entries whose
//! return type is an enum domain. A read in neither table below fails
//! the test, so a newly cataloged read is added deliberately, and a
//! read that becomes modeled fails the unmodeled check.

use athena_sim::{Outcome, Val, run_scenario};
use workshop_rs::catalog::{Catalog, Kind};

/// The scenario spellings below differ from the catalog member ids
/// (`ana` vs `ANA`, `team1` vs `TEAM_1`, `king's row` vs `KINGS_ROW`).
const SCENARIO: &str = r#"{
    "scenarioVersion": 1,
    "seconds": 1,
    "map": "king's row",
    "gameMode": "skirmish",
    "players": [{ "id": "p1", "hero": "ana", "team": "team1", "host": true }]
}"#;

/// Catalog value id, then the read and the literal it must equal.
const MODELED: &[(&str, &str, &str)] = &[
    ("hero", "Hero(Ana)", "Hero(Ana)"),
    ("getHero", "Hero Of(Host Player)", "Hero(Ana)"),
    ("teamOf", "Team Of(Host Player)", "Team(Team 1)"),
    ("currentMap", "Current Map", "Map(King's Row)"),
    (
        "getCurrentGamemode",
        "Current Game Mode",
        "Game Mode(Skirmish)",
    ),
    ("gameMode", "Game Mode(Skirmish)", "Game Mode(Skirmish)"),
    (
        "createWorkshopSettingHero",
        r#"Workshop Setting Hero(Custom String("a"), Custom String("b"), Hero(Ana), 0)"#,
        "Hero(Ana)",
    ),
];

/// Reads the simulator does not model: the run must not complete.
const UNMODELED: &[(&str, &str, &str)] = &[
    ("oppositeTeamOf", "Opposite Team Of(Team 1)", "Team(Team 2)"),
    (
        "getHeroOfDuplication",
        "Hero Being Duplicated(Host Player)",
        "Hero(Ana)",
    ),
    (
        "getControlScoringTeam",
        "Control Mode Scoring Team",
        "Team(Team 1)",
    ),
    ("eventAbility", "Event Ability", "Button(Ability 1)"),
    ("customColor", "Custom Color(1, 2, 3, 4)", "Color(Red)"),
];

fn program(read: &str, literal: &str) -> String {
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
        Set Global Variable(g, Compare({read}, ==, {literal}));
    }}
}}
"#
    )
}

fn run(read: &str, literal: &str) -> athena_sim::Trace {
    run_scenario(&program(read, literal), SCENARIO).unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn every_enum_valued_read_equals_its_enum_literal() {
    let catalog = Catalog::builtin_ref().unwrap_or_else(|error| panic!("{error}"));
    // `Vector` is an enum domain of directional constants, but its reads
    // yield numeric vectors, not member ids.
    let reads: Vec<&str> = catalog
        .entries_of(Kind::Value)
        .filter(|entry| {
            entry
                .return_type()
                .is_some_and(|ty| ty != "Vector" && catalog.enum_domain(ty).is_some())
        })
        .map(|entry| entry.id.as_str())
        .collect();

    for id in &reads {
        if let Some((_, read, literal)) = MODELED.iter().find(|(name, ..)| name == id) {
            let trace = run(read, literal);
            assert_eq!(trace.outcome, Outcome::Complete, "{id}: {read}");
            assert_eq!(trace.globals["g"], Val::Bool(true), "{id}: {read}");
        } else if let Some((_, read, literal)) = UNMODELED.iter().find(|(name, ..)| name == id) {
            let trace = run(read, literal);
            assert_ne!(trace.outcome, Outcome::Complete, "{id} is now modeled");
        } else {
            panic!("enum-valued read `{id}` is in neither MODELED nor UNMODELED");
        }
    }
    for id in MODELED
        .iter()
        .map(|(id, ..)| id)
        .chain(UNMODELED.iter().map(|(id, ..)| id))
    {
        assert!(
            reads.contains(id),
            "`{id}` is not an enum-valued catalog read"
        );
    }
}
