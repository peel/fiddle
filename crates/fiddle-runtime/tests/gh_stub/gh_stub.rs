//! A scripted `gh` for the deterministic suite.
//!
//! It answers from a scripted directory and records every request, so a test can
//! assert both what was asked and what the world became. It is a test fixture
//! reached through the product's own `cli.program` seam — the one that exists
//! for operators who must pin or wrap `gh` — and it is declared with
//! `required-features`, so `cargo build --release` never produces it. Nothing
//! here is compiled into the product.
//!
//! # Why the scratch directory arrives in `argv` and not in the environment
//!
//! The plan's sketch read `GH_STUB_DIR` from the environment. It cannot: the
//! adapter under test runs `env_clear()` and then sets exactly five names, so
//! no sixth variable can reach this process — and widening that set to let the
//! fixture work would delete the property the fixture exists to prove. It comes
//! through `cli.args` instead, which is the same operator seam the stub is
//! substituted at. That the environment is unusable *for the test's own
//! plumbing* is the first piece of evidence that it is pinned.

use std::io::Write;
use std::path::{Path, PathBuf};

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let dir = take_stub_dir(&mut args).expect("--stub-dir <path> must be passed through cli.args");

    let mut body_in = String::new();
    if args.iter().any(|a| a == "--input") {
        use std::io::Read;
        let _ = std::io::stdin().read_to_string(&mut body_in);
    }

    // Record the request in arrival order — argv, the request body, and the
    // environment. The environment matters as much as the arguments: the
    // five-name assertion is made against what the child actually received, and
    // a stub that recorded only argv could not support it.
    let requests = dir.join("requests");
    std::fs::create_dir_all(&requests).unwrap();
    let n = std::fs::read_dir(&requests).unwrap().count();
    let env: Vec<String> = std::env::vars().map(|(k, v)| format!("{k}={v}")).collect();
    std::fs::write(
        requests.join(format!("{n:04}.json")),
        serde_json::json!({ "argv": args, "body": body_in, "env": env }).to_string(),
    )
    .unwrap();

    // A `gh` that never comes back, so the runtime's own deadline is the only
    // thing that can end it. Both markers are written *after* the sleep: a test
    // that finds neither has proof the children were actually killed rather than
    // merely abandoned by a parent that stopped waiting.
    //
    // The descendant is the half that distinguishes the two mechanisms.
    // `kill_on_drop` reaps this process alone, so a runtime relying on it would
    // still leave the `sh` below running — and a `gh` is entirely free to fork.
    // Only a kill aimed at the whole process group reaches it.
    if let Ok(ms) = std::fs::read_to_string(dir.join("sleep_ms")) {
        let ms: u64 = ms.trim().parse().unwrap_or(0);
        let marker = dir.join("descendant_survived_the_deadline");
        let _ = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(format!(
                "sleep {}; : > '{}'",
                ms as f64 / 1000.0,
                marker.display()
            ))
            .spawn();
        std::thread::sleep(std::time::Duration::from_millis(ms));
        std::fs::write(dir.join("survived_the_deadline"), "yes").unwrap();
    }

    // The stub is *stateful*: a GET is answered from the world previous writes
    // built, not from a fixed script. A static script could not express the only
    // question that matters here — "after the write landed and the answer was
    // lost, what does the next process see?" — because that answer differs
    // between the first call and the second.
    let key = script_key(&args);
    if key.starts_with("GET") {
        let (status, body) = world_answer(&dir, &key);
        print!("HTTP/2.0 {status} \r\n\r\n{body}");
        std::process::exit(if status < 400 { 0 } else { 1 });
    }

    // A write consults the script only for how it should *end*, never for what
    // the world becomes. Format: `<status> <exit> <mode>`.
    let spec = std::fs::read_to_string(dir.join("script").join(&key))
        .unwrap_or_else(|_| "201 0 normal".to_string());
    let mut parts = spec.split_whitespace();
    let status: u16 = parts.next().unwrap().parse().unwrap();
    let exit: i32 = parts.next().unwrap().parse().unwrap();
    let mode = parts.next().unwrap_or("normal");

    // The ambiguous write, and the reason this whole stub exists: the effect
    // really lands and then the answer is really lost. Note the order — a stub
    // that exited first would be testing a failed write, which proves nothing.
    if let Some(death) = mode.strip_prefix("commit_then_") {
        apply_effect(&dir, &key, &body_in);
        match death {
            // 128 + SIGKILL, the shell's spelling of a killed child, which some
            // wrappers pass on as their own exit code.
            "die" => std::process::exit(137),
            // A real signal death, so `ExitStatus::code()` is `None`. The
            // adapter must classify both as `Unknown`, and this is the pair that
            // proves it does not depend on which one it got.
            "abort" => std::process::abort(),
            other => panic!("unknown death mode {other}"),
        }
    }

    // Not a response at all — the shape of `cli.program` pointing at something
    // that is not `gh`. The credential goes to stderr on purpose, because that
    // is the one stream the adapter quotes and so the one place a redaction
    // could be forgotten.
    if mode == "garbage" {
        let token = std::env::var("GH_TOKEN").unwrap_or_default();
        print!("this is not an HTTP response\n\nneither is this");
        eprint!("gh: could not authenticate with {token}");
        std::process::exit(exit);
    }

    if status < 400 {
        apply_effect(&dir, &key, &body_in);
    }
    print!(
        "HTTP/2.0 {status} \r\n{}\r\n{}",
        response_headers(mode),
        response_body(status, mode)
    );
    std::process::exit(exit);
}

/// The header block, CRLF-terminated as a real `gh -i` writes it.
///
/// `rate_limited` reproduces the shape GitHub actually sends, taken from a probe
/// of the real binary — including `Access-Control-Expose-Headers`, whose *value*
/// lists `Retry-After` and `X-RateLimit-Remaining`. A parser that searched the
/// header block for those strings instead of reading header names would find
/// them there and report a retry delay nobody sent.
fn response_headers(mode: &str) -> String {
    match mode {
        "rate_limited" => concat!(
            "Access-Control-Expose-Headers: ETag, Retry-After, X-RateLimit-Remaining\r\n",
            "Content-Type: application/json; charset=utf-8\r\n",
            "Retry-After: 60\r\n",
            "X-RateLimit-Remaining: 0\r\n",
        )
        .to_string(),
        _ => String::new(),
    }
}

/// What a scripted write answers with.
///
/// `echo_token` is the adversarial one: it puts the credential into the
/// response body, which is precisely the shape of M1's published-key defect. A
/// client that carried a body into a diagnostic would leak it, and the sentinel
/// test would catch that rather than passing because nothing happened to be
/// echoed.
fn response_body(status: u16, mode: &str) -> String {
    if mode == "echo_token" {
        let token = std::env::var("GH_TOKEN").unwrap_or_default();
        return serde_json::json!({ "message": format!("Bad credentials: {token}") }).to_string();
    }
    match status >= 400 {
        true => serde_json::json!({ "message": format!("scripted {status}") }).to_string(),
        false => "{}".to_string(),
    }
}

/// Pull `--stub-dir <path>` off the front of the arguments.
///
/// Removed rather than skipped over, so [`script_key`] never sees a path that
/// starts with `/` and is not the API path.
fn take_stub_dir(args: &mut Vec<String>) -> Option<PathBuf> {
    let at = args.iter().position(|a| a == "--stub-dir")?;
    let dir = args.get(at + 1)?.clone();
    args.drain(at..=at + 1);
    Some(PathBuf::from(dir))
}

/// `--method POST /repos/o/r/pulls` becomes `POST_repos_o_r_pulls`. The query
/// string is kept, because a lookup by head and base is a different request from
/// a lookup by base alone and a script that conflated them would let a test pass
/// by answering the wrong question.
fn script_key(args: &[String]) -> String {
    let mut method = "GET".to_string();
    let mut path = String::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--method" | "-X" => method = it.next().cloned().unwrap_or_default(),
            "api" | "-i" | "--input" | "-" => {}
            other if other.starts_with('/') => path = other.to_string(),
            _ => {}
        }
    }
    format!(
        "{method}_{}",
        path.trim_start_matches('/')
            .replace(['/', '?', '&', '=', '%'], "_")
    )
}

/// The world is an append-only log of the mutations that actually landed, so a
/// test asserts against what happened rather than against what was answered.
fn apply_effect(dir: &Path, key: &str, body: &str) {
    if !key.starts_with("POST") && !key.starts_with("DELETE") {
        return; // a read changes nothing
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("world"))
        .unwrap();
    writeln!(f, "{}", serde_json::json!({ "key": key, "body": body })).unwrap();
}

/// Answer a read from the world the writes built. This is what makes the
/// exactly-once harness meaningful: after a `commit_then_*` mode, the object is
/// really there, so the fresh process's postcondition read really finds it.
fn world_answer(dir: &Path, key: &str) -> (u16, String) {
    let world = std::fs::read_to_string(dir.join("world")).unwrap_or_default();
    let landed: Vec<serde_json::Value> = world
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let landed_key = |w: &serde_json::Value, needle: &str| {
        w["key"].as_str().unwrap_or_default().contains(needle)
    };

    if key.starts_with("GET_repos") && key.contains("git_ref_heads") {
        let branch = key.rsplit('_').next().unwrap_or_default();
        return match landed.iter().any(|w| landed_key(w, "git_refs")) {
            true => (200, format!(r#"{{"object":{{"sha":"c0ffee{branch}"}}}}"#)),
            // An absent ref is a 404, which the adapter reads as knowledge.
            false => (404, r#"{"message":"Not Found"}"#.to_string()),
        };
    }
    if key.starts_with("GET_repos") && key.contains("pulls") {
        let prs: Vec<_> = landed
            .iter()
            .filter(|w| {
                w["key"].as_str().unwrap_or_default().starts_with("POST") && landed_key(w, "pulls")
            })
            .enumerate()
            .map(|(i, _)| serde_json::json!({ "number": 7 + i, "state": "open" }))
            .collect();
        return (200, serde_json::Value::Array(prs).to_string());
    }
    if key.starts_with("GET_repos") && key.contains("actions_workflows") {
        // Runs are located by run-name, because a workflow dispatch answers 204
        // with no run id and the runs listing does not expose dispatch inputs.
        let runs: Vec<_> = landed
            .iter()
            .filter(|w| landed_key(w, "dispatches"))
            .map(|w| {
                serde_json::json!({
                    "id": 1,
                    "name": format!("fiddle-{}", effect_id_in(&w["body"])),
                    "status": "queued",
                })
            })
            .collect();
        return (
            200,
            serde_json::json!({ "workflow_runs": runs }).to_string(),
        );
    }
    (404, r#"{"message":"Not Found"}"#.to_string())
}

fn effect_id_in(body: &serde_json::Value) -> String {
    serde_json::from_str::<serde_json::Value>(body.as_str().unwrap_or("{}"))
        .ok()
        .and_then(|b| b["inputs"]["fiddle_effect_id"].as_str().map(String::from))
        .unwrap_or_default()
}
