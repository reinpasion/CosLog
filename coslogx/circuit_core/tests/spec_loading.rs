use circuit_core::{netlist::{build, rnd}, project::Project};

#[test]
fn tolerance_vectors_match_project_schema() {
    // Vectors published in spec/project_schema.md (computed independently with BigInt in the web edition).
    let v = [(24301u64, 0usize, -0.92230253060562961f64), (24301, 1, -0.33439778252114039), (24301, 2, -0.27063628724372357),
             (24301, 7, -0.85517423226382672), (24301, 100, 0.20850133807570881), (1, 0, 0.13312315034456179), (1, 3, -0.11128156588845584)];
    for (s, i, want) in v { assert_eq!(rnd(s, i), want, "seed {s} index {i}"); }
}

const FILE: &str = r#"{"app":"CosLog","ver":1,"seed":"0x10","c":[
 {"t":"r","x":100,"y":100,"v":"1k","p":"5","n":""},
 {"t":"zzz","x":0,"y":0},
 {"t":"r","x":200,"y":100,"v":"1k","p":"5"},
 {"t":"nope","x":0,"y":0},{"t":"zzz","x":0,"y":0}
],"w":[]}"#;

#[test]
fn unknown_parts_are_skipped_with_one_warning_and_index_is_file_position() {
    let l = Project::load_json(FILE).unwrap();
    assert_eq!(l.project.c.len(), 2);
    assert_eq!(l.warnings, vec!["Skipped 3 unknown part(s): zzz, nope (made with a newer or different CosLog)".to_string()]);
    assert_eq!(l.project.seed(), 16);
    assert_eq!(l.project.c[0].n, None, "empty name counts as missing");
    assert_eq!(l.project.c.iter().map(|c| c.si).collect::<Vec<_>>(), vec![Some(0), Some(2)]);
    // second resistor hashes with its file index 2, not its position 1 among loaded parts
    let nl = build(&l.project, l.project.seed());
    let want = 1000.0 * (1.0 + 5.0 / 100.0 * rnd(16, 2));
    assert_eq!(nl.parts[1].val, want);
}

#[test]
fn seed_forms_and_failures() {
    let mk = |s: &str| format!(r#"{{"seed":{s},"c":[],"w":[]}}"#);
    assert_eq!(Project::load_json(&mk("24301")).unwrap().project.seed(), 24301);
    assert_eq!(Project::load_json(&mk(r#""24301""#)).unwrap().project.seed(), 24301);
    assert_eq!(Project::load_json(&mk(r#""0x5EED""#)).unwrap().project.seed(), 0x5EED);
    let bad = Project::load_json(&mk(r#""banana""#)).unwrap();
    assert_eq!(bad.project.seed(), 0x5EED); assert_eq!(bad.warnings.len(), 1);
    assert_eq!(Project::load_json(r#"{"c":[],"w":[]}"#).unwrap().project.seed(), 0x5EED);
    assert!(Project::load_json("not json").is_err());
    assert!(Project::load_json(r#"{"c":{},"w":[]}"#).is_err());
}

#[test]
fn seed_round_trips_and_default_is_not_written() {
    let mut p = Project::empty();
    assert!(!p.to_json().contains("seed"));
    p.seed = Some(0x5EED); assert!(!p.to_json().contains("seed"));
    p.seed = Some(7); assert!(p.to_json().contains("\"seed\": 7"));
    p.seed = Some(u64::MAX); assert!(p.to_json().contains("\"seed\": \"18446744073709551615\""));
    assert_eq!(Project::from_json(&p.to_json()).unwrap().seed(), u64::MAX);
    assert!(p.to_json().find("\"app\"").unwrap() < p.to_json().find("\"seed\"").unwrap());
}
