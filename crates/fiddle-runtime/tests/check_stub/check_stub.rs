use std::path::{Path, PathBuf};

const RECORD: &str = "--record";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if let Some(path) = flag(&args, RECORD) {
        record(Path::new(&path));
    }

    if flag(&args, "--hang").is_some() {
        std::thread::sleep(std::time::Duration::from_secs(600));
    }

    if let Some(text) = flag(&args, "--say") {
        println!("{text}");
    }
    if let Some(text) = flag(&args, "--warn") {
        eprintln!("{text}");
    }

    if let Some(code) = flag(&args, "--exit") {
        std::process::exit(code.parse().expect("--exit takes a number"));
    }
}

fn flag(args: &[String], name: &str) -> Option<String> {
    let at = args.iter().position(|arg| arg == name)?;
    match args.get(at + 1) {
        Some(value) => Some(value.clone()),
        None => panic!("{name} takes a value"),
    }
}

fn record(path: &Path) {
    let cwd = std::env::current_dir().expect("a working directory");
    let mut entries: Vec<String> = std::fs::read_dir(&cwd)
        .unwrap_or_else(|source| panic!("could not list {}: {source}", cwd.display()))
        .map(|entry| {
            entry
                .expect("a directory entry")
                .file_name()
                .to_string_lossy()
                .to_string()
        })
        .collect();
    entries.sort();

    let argv: Vec<String> = std::env::args().collect();
    let env: Vec<String> = std::env::vars()
        .map(|(name, value)| format!("{name}={value}"))
        .collect();

    let record = serde_json::json!({
        "argv": argv,
        "cwd": cwd.to_string_lossy(),
        "entries": entries,
        "env": env,
    });
    write(path.to_path_buf(), record.to_string());
}

fn write(path: PathBuf, body: String) {
    std::fs::write(&path, body)
        .unwrap_or_else(|source| panic!("could not write {}: {source}", path.display()));
}
