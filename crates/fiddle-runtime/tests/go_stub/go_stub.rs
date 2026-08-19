#[allow(dead_code)]
#[path = "../support/go_proxy.rs"]
mod go_proxy;

const CHILD_RECORD: &str = "child.json";

fn main() {
    let root = std::env::current_dir().expect("a working directory to be a module root in");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();

    record();

    let answer = go_proxy::run(&root, &borrowed);
    print!("{}", answer.stdout);
    eprint!("{}", answer.stderr);
    std::process::exit(answer.code);
}

fn record() {
    let home = std::env::var("HOME")
        .expect("the adapter gives its child a HOME; without one there is nowhere to record");
    let argv: Vec<String> = std::env::args().collect();
    let env: Vec<String> = std::env::vars()
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    let record = std::path::Path::new(&home).join(CHILD_RECORD);
    std::fs::write(
        &record,
        serde_json::json!({ "argv": argv, "env": env }).to_string(),
    )
    .unwrap_or_else(|source| panic!("could not write {}: {source}", record.display()));
}
