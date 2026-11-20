use circuit_core::{project::*, svg::project_svg};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let p = if args.get(1).map(|s| s.as_str()) == Some("demo") {
        Project::from_json(&std::fs::read_to_string("assets/demo.json").unwrap()).unwrap()
    } else { Project::gallery() };
    println!("{}", project_svg(&p, None));
}
