use std::path::{Path, PathBuf};

const RECORD: &str = "--record";

const RELOCK: &str = "--relock";

const VERIFY: &str = "--verify";

const SUBSTITUTE: &str = "--substitute";

const LOCK_SUFFIX: &str = ".lock";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if let Some(path) = flag(&args, RECORD) {
        record(Path::new(&path));
    }

    if let Some(path) = flag(&args, RELOCK) {
        relock(Path::new(&path));
    }

    if let Some(path) = flag(&args, VERIFY) {
        verify(Path::new(&path));
    }

    if let Some(path) = flag(&args, SUBSTITUTE) {
        let from = flag(&args, "--from").expect("--substitute needs --from");
        let to = flag(&args, "--to").expect("--substitute needs --to");
        substitute(Path::new(&path), &from, &to);
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

fn substitute(path: &Path, from: &str, to: &str) {
    let held = std::fs::read_to_string(path)
        .unwrap_or_else(|source| panic!("could not read {}: {source}", path.display()));
    write(path.to_path_buf(), held.replace(from, to));
    std::process::exit(0);
}

fn relock(source: &Path) {
    write(lock_of(source), locked(source));
}

fn verify(source: &Path) {
    let expected = locked(source);
    let held = std::fs::read_to_string(lock_of(source)).unwrap_or_default();
    if held != expected {
        eprintln!("the lock does not describe the source it is derived from");
        std::process::exit(1);
    }
    println!("the lock describes the source");
    std::process::exit(0);
}

fn lock_of(source: &Path) -> PathBuf {
    let mut name = source.as_os_str().to_owned();
    name.push(LOCK_SUFFIX);
    PathBuf::from(name)
}

fn locked(source: &Path) -> String {
    let body = std::fs::read(source).unwrap_or_else(|source_error| {
        panic!("could not read {}: {source_error}", source.display())
    });
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in &body {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{} {hash:016x}\n", source.display())
}

fn write(path: PathBuf, body: String) {
    std::fs::write(&path, body)
        .unwrap_or_else(|source| panic!("could not write {}: {source}", path.display()));
}
