use std::io::Write;
use std::path::PathBuf;

const HEAD_SHA: &str = "0123456789abcdef0123456789abcdef01234567";

const FOREVER: std::time::Duration = std::time::Duration::from_secs(120);

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let subcommand = args.first().cloned().unwrap_or_default();
    let dir: PathBuf = std::env::current_dir().expect("the adapter runs its git in the worktree");
    let mode = std::fs::read_to_string(dir.join("mode"))
        .unwrap_or_else(|_| "accepted".to_string())
        .trim()
        .to_string();

    let env: Vec<String> = std::env::vars()
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    std::fs::write(
        dir.join(format!("{subcommand}.json")),
        serde_json::json!({ "argv": args, "env": env }).to_string(),
    )
    .unwrap();

    if subcommand == "push" {
        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("pushes"))
            .unwrap();
        writeln!(log, "{}", args.join(" ")).unwrap();
    }

    if subcommand == "rev-parse" {
        match delegating(&mode) {
            true => delegate(&args),
            false => println!("{HEAD_SHA}"),
        }
        return;
    }

    match mode.as_str() {
        "accepted" => print!("To stub\n*\tHEAD:refs/heads/fiddle/abc\t[new branch]\nDone\n"),
        "leaks_the_header" => {
            eprintln!(
                "fatal: unable to access remote: {}",
                std::env::var("GIT_CONFIG_VALUE_0").unwrap_or_default()
            );
            std::process::exit(128);
        }
        "push_then_killed" => {
            delegate(&args);
            std::fs::write(dir.join("pushed_then_died"), "yes").unwrap();
            std::process::abort();
        }
        "push_then_waits" => {
            delegate(&args);
            std::fs::write(dir.join("pushed_then_waited"), "yes").unwrap();
            std::thread::sleep(FOREVER);
            std::process::exit(1);
        }
        "delegated" => delegate(&args),
        "never_answers" => std::thread::sleep(FOREVER),
        other => panic!("unknown mode {other}"),
    }
}

fn delegating(mode: &str) -> bool {
    matches!(
        mode,
        "push_then_killed" | "push_then_waits" | "never_answers" | "delegated"
    )
}

fn delegate(args: &[String]) {
    let status = std::process::Command::new("git")
        .args(args)
        .status()
        .expect("git is on the PATH the adapter passed through");
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}
