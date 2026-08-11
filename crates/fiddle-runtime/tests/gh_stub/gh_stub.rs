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

/// Longer than any deadline a test sets, so a mode that waits to be cancelled is
/// ended by the cancellation and by nothing else. The same constant, for the same
/// reason, as `git_stub`'s.
const FOREVER: std::time::Duration = std::time::Duration::from_secs(600);

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

    // GraphQL is answered from its own script, because it cannot share the REST
    // one. A REST request is keyed by method and path and it has neither: every
    // call is `POST /graphql` and what was asked lives in a field. Its verdict is
    // in the body rather than on the status line, so the two halves of an answer
    // — the status and the body — have to be scriptable independently, which is
    // exactly what the `<status> <exit> <mode>` script cannot express.
    if endpoint(&args) == Some("graphql") {
        let (status, body, mode) = graphql_answer(&dir);
        let request = graphql_request(&args).to_string();

        // The ambiguous *mutation*, and this route's own half of what the
        // `commit_then_*` block below does for a REST write. Checked before the
        // answer is printed and applied before the ending runs, in that order and
        // for that reason: a route that ended first would be testing a mutation
        // that never landed.
        //
        // The ending rides in `graphql/{n}.json` rather than in the `<status>
        // <exit> <mode>` script because this route cannot reach that script and
        // must not. It returns above it deliberately — every GraphQL call is one
        // `POST /graphql`, so [`script_key`] derives one key for all of them and
        // could not tell a refusal from a lost answer, and the script's single
        // status field cannot express a verdict that lives in the body.
        if let Some(ending) = mode.strip_prefix("commit_then_") {
            apply_effect(&dir, GRAPHQL_KEY, &request, &mode);
            end_without_answering(&dir, ending);
        }

        // Exit 1 on a refusal and 0 on a mutation that landed, which is what the
        // real `gh` was measured doing: a refused mutation answers 200 and exits
        // 1. It is written this way so that an adapter which consulted the exit
        // code, or the status line, instead of the body would fail here rather
        // than pass on a stub that agreed with it.
        let refused = status >= 400
            || body["errors"]
                .as_array()
                .is_some_and(|errors| !errors.is_empty());
        // A mutation nothing refused really lands, which is this route's
        // equivalent of the `status < 400` test the REST write makes at the bottom
        // of this function — read off the *body*, because that is where a GraphQL
        // verdict is. Without this the route answered and changed nothing, so no
        // mutation through it could ever be observed to have happened and there
        // was no committed GraphQL path at all.
        //
        // A refusal deliberately lands nothing: `errors[]` against a world that
        // still shows a draft is what makes the refusal stand as the answer
        // rather than as a write that might be in flight.
        if !refused {
            apply_effect(&dir, GRAPHQL_KEY, &request, &mode);
        }
        print!("HTTP/2.0 {status} \r\n\r\n{body}");
        std::process::exit(i32::from(refused));
    }

    // The stub is *stateful*: a GET is answered from the world previous writes
    // built, not from a fixed script. A static script could not express the only
    // question that matters here — "after the write landed and the answer was
    // lost, what does the next process see?" — because that answer differs
    // between the first call and the second.
    let key = script_key(&args);
    if key.starts_with("GET") {
        // The *raw* path, query and all. `script_key` mangles the separators so
        // that a key is a filename, which is fine for choosing a script and
        // useless for answering a filtered read: a list endpoint's answer
        // depends on the parameters, and a stub that could not read them back
        // would answer the same thing whatever it was asked.
        let path = request_path(&args);
        // The comment collections come first, and they answer with headers,
        // which is what the rest of the world route has never needed.
        if let Some((status, headers, body)) = comment_answer(&dir, &path) {
            print!("HTTP/2.0 {status} \r\n{headers}\r\n{body}");
            std::process::exit(if status < 400 { 0 } else { 1 });
        }
        let (status, body) = world_answer(&dir, &key, &path);
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
    if let Some(ending) = mode.strip_prefix("commit_then_") {
        apply_effect(&dir, &key, &body_in, mode);
        end_without_answering(&dir, ending);
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

    // Another actor got there first. GitHub answers a duplicate pull request
    // with a 422, and the reason it can is that a pull request for that head and
    // base already exists — so the refusal and the object are one event, and the
    // stub writes both. It is written *before* the response for the same reason
    // `commit_then_*` mutates before it dies: a world that changed only after
    // the client heard about it would be a world the client could not have
    // raced.
    if mode == "conflict" {
        open_pull_request_for(&dir, &body_in);
    }

    if status < 400 {
        apply_effect(&dir, &key, &body_in, mode);
    }
    print!(
        "HTTP/2.0 {status} \r\n{}\r\n{}",
        response_headers(mode),
        response_body(status, mode)
    );
    std::process::exit(exit);
}

/// End the process the way a `gh` whose answer was lost ends: without one.
///
/// Called from both write routes — the REST script's `commit_then_*` and the
/// GraphQL per-call file's — and that sharing is the point. The two routes
/// disagree about how an ending is *scripted*, because one is keyed by method and
/// path and the other has neither, but "the mutation landed and then the answer
/// was lost" is one fact with one set of provenances, and a second copy of these
/// three arms would be a second fixture that could drift from this one. The
/// caller applies the effect first; this function only loses the answer.
///
/// Diverging returns rather than a value, so the caller cannot accidentally carry
/// on and print a response after the answer was supposed to have been lost.
fn end_without_answering(dir: &Path, ending: &str) -> ! {
    match ending {
        // 128 + SIGKILL, the shell's spelling of a killed child, which some
        // wrappers pass on as their own exit code.
        "die" => std::process::exit(137),
        // A real signal death, so `ExitStatus::code()` is `None`. The adapter
        // must classify both as `Unknown`, and this is the pair that proves it
        // does not depend on which one it got.
        "abort" => std::process::abort(),
        // The **third** provenance of one ambiguous write, and the one the other
        // two cannot reach: the mutation lands and then the answer is lost to a
        // *cancellation* rather than to a death.
        //
        // This mode does not end itself. It records that the write landed and
        // then waits to be killed, so what ends it is the runtime's own
        // cancellation token — the channel a `^C` reaches a bounded child
        // through, since the child has a process group of its own. A `gh` that
        // exited on its own could only ever produce the killed-child provenance,
        // which is the one the adapter already got right; the milestone's
        // holistic review found that the harness had therefore never injected the
        // one it got wrong.
        //
        // The marker is written *after* the caller's mutation and *before* the
        // wait, for the same reason `git_stub`'s `pushed_then_died` is: it is the
        // fixture's own record that the world really changed before the answer
        // was lost, and it is what a test waits on before it interrupts.
        "wait" => {
            std::fs::write(dir.join("landed_and_waiting"), "yes").unwrap();
            std::thread::sleep(FOREVER);
            // Only reachable if nothing ever cancelled, which is a test that
            // arranged an interrupt and failed to deliver it. Exiting non-zero
            // rather than answering keeps that from looking like a successful
            // write.
            std::process::exit(1);
        }
        other => panic!("unknown ending mode {other}"),
    }
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
    // A response that offers a run id, which the real dispatch endpoint never
    // does — it answers 204 with nothing at all. The mode exists so a suite can
    // prove the client is not reading one: an implementation that took its
    // external reference from here would report 999999, which is not the id of
    // anything in the world the listing describes.
    if mode == "answers_a_run_id" {
        return serde_json::json!({ "id": 999_999 }).to_string();
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

/// What `gh api` was pointed at: the first argument after `api` that is neither
/// a flag nor a flag's value.
///
/// Written as a scan rather than as "the argument that is not `-i`", because a
/// GraphQL call carries `-f query=…` whose value would otherwise look like an
/// endpoint. For a REST call this is the path, which [`script_key`] reads for
/// itself; the one caller here only asks whether it is `graphql`.
fn endpoint(args: &[String]) -> Option<&str> {
    let mut rest = args.iter().skip_while(|a| a.as_str() != "api");
    rest.next()?;
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            // Flags that take a value, so the value is not an endpoint.
            "--method" | "-X" | "-f" | "-F" | "--input" => {
                rest.next();
            }
            flag if flag.starts_with('-') => {}
            other => return Some(other),
        }
    }
    None
}

/// The world key a GraphQL mutation lands under.
///
/// `POST_graphql`, because that is the request: every GraphQL call is a `POST` to
/// `/graphql`. Stated here rather than derived by [`script_key`], which cannot
/// produce it — a GraphQL call carries no path argument at all, so the key that
/// function derives is `GET_`, and [`apply_effect`] reads a `GET` as a read and
/// drops it. The mangling is right for REST and this route is the one place a key
/// has to be named instead of parsed.
const GRAPHQL_KEY: &str = "POST_graphql";

/// The next scripted GraphQL answer, in call order, and how the call should end.
///
/// Each `graphql/<n>.json` holds `{"status": <code>, "body": <value>}` and may
/// hold a `"mode"` beside them. The status and the body are scripted separately
/// on purpose: for GraphQL they are independent facts, since a refusal arrives as
/// 200 with `errors[]`, and a fixture that derived one from the other could not
/// express the case this route exists for.
///
/// The mode is a **third** independent fact and is why it is a key here rather
/// than a fourth word in the REST route's `<status> <exit> <mode>` script. That
/// script is looked up by [`script_key`], which is one key for every GraphQL call
/// ever made, so a sequence of calls could not be given different endings; and
/// this route returns before that script is read, deliberately, because a single
/// status field cannot carry a verdict that lives in the body. `page-{k}.link`
/// beside a comment page is the same shape — a per-response-unit sidecar for the
/// one fact the shared script cannot express.
///
/// The counter is a file rather than a count of recorded requests because a test
/// may script a sequence — a refusal, then a success — and each call is its own
/// process, so there is nowhere else for the position to live.
fn graphql_answer(dir: &Path) -> (u16, serde_json::Value, String) {
    let counter = dir.join("graphql_calls");
    let n: usize = std::fs::read_to_string(&counter)
        .ok()
        .and_then(|seen| seen.trim().parse().ok())
        .unwrap_or(0);
    std::fs::write(&counter, (n + 1).to_string()).unwrap();

    // Unscripted answers a plain success, the same courtesy the REST route's
    // `201 0 normal` default extends: a test whose subject is the request rather
    // than the answer should not have to script one.
    let Ok(scripted) = std::fs::read_to_string(dir.join("graphql").join(format!("{n}.json")))
    else {
        return (200, serde_json::json!({ "data": {} }), "normal".to_string());
    };
    let scripted: serde_json::Value =
        serde_json::from_str(&scripted).expect("a scripted GraphQL answer must be JSON");
    (
        scripted["status"].as_u64().unwrap_or(200) as u16,
        scripted["body"].clone(),
        // The REST script's own default, spelled the same way: a call whose
        // subject is the answer should not have to say how it ended.
        scripted["mode"].as_str().unwrap_or("normal").to_string(),
    )
}

/// The `-f name=value` pairs a GraphQL call carries, as one object.
///
/// This is the request in everything but transport, and it is what a world entry
/// records: `gh` turns these into the query and the variables bound to it, so
/// `{"query": "mutation($id: ID!) { markPullRequestReadyForReview…", "id":
/// "PR_kwD…"}` is the whole of what was asked. The REST route reads its body off
/// stdin; a GraphQL call has none, because `GhCli::graphql` passes no `--input`.
///
/// Recorded flat, exactly as the child received it, rather than reshaped into
/// `{"query": …, "variables": {…}}`. `gh` spells a GraphQL variable and a form
/// field identically — that is ADR 018's measurement — so which of these is a
/// variable is not a fact this process has, and a fixture that guessed would be
/// modelling `gh` where it can simply record it.
fn graphql_request(args: &[String]) -> serde_json::Value {
    let mut fields = serde_json::Map::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if arg != "-f" && arg != "-F" {
            continue;
        }
        if let Some((name, value)) = it.next().and_then(|field| field.split_once('=')) {
            fields.insert(name.to_string(), value.into());
        }
    }
    serde_json::Value::Object(fields)
}

/// The API path exactly as it was asked for, query string included.
fn request_path(args: &[String]) -> String {
    args.iter()
        .find(|a| a.starts_with('/'))
        .cloned()
        .unwrap_or_default()
}

/// One query parameter, percent-decoded.
///
/// Decoded rather than compared raw, because the encoding is the client's
/// choice and the *value* is the contract: a stub that matched on the literal
/// `%3A` would fail a client that sent a bare colon, which GitHub accepts.
fn query_param(path: &str, name: &str) -> Option<String> {
    let query = path.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then(|| percent_decode(value))
    })
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => match u8::from_str_radix(&value[i + 1..i + 3], 16) {
                Ok(byte) => {
                    out.push(byte);
                    i += 3;
                }
                Err(_) => {
                    out.push(bytes[i]);
                    i += 1;
                }
            },
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The pull requests this world holds, in the order they came to exist.
///
/// Two sources, one list. `pulls_seed` is what existed before this process ran —
/// written by the test to arrange a world, or by the `conflict` mode to record
/// the racing actor's create — and the world log supplies the ones this run's
/// own `POST`s created. Numbers are positional and start at 7 rather than 1, so
/// a test asserting on an external reference cannot pass by accident against an
/// index or a count.
fn pull_requests(dir: &Path) -> Vec<serde_json::Value> {
    let seeded = read_pull_request_seed(dir);
    let created = std::fs::read_to_string(dir.join("world"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|w| {
            let key = w["key"].as_str().unwrap_or_default();
            key.starts_with("POST") && key.contains("pulls")
        })
        .map(|w| {
            serde_json::from_str::<serde_json::Value>(w["body"].as_str().unwrap_or("{}"))
                .unwrap_or(serde_json::Value::Null)
        })
        .collect::<Vec<_>>();

    seeded
        .into_iter()
        .chain(created)
        .enumerate()
        .map(|(i, pr)| {
            let head = pr["head"].as_str().unwrap_or_default().to_string();
            serde_json::json!({
                "number": 7 + i,
                "state": "open",
                "title": pr["title"].as_str().unwrap_or_default(),
                // GitHub's own shape: the head is a `label` of `owner:branch`
                // beside the bare `ref`, and the base is a `ref` alone.
                "head": {
                    "label": head,
                    "ref": head.split_once(':').map(|(_, r)| r).unwrap_or(&head),
                },
                "base": { "ref": pr["base"].as_str().unwrap_or_default() },
            })
        })
        .collect()
}

fn read_pull_request_seed(dir: &Path) -> Vec<serde_json::Value> {
    read_seed(dir, "pulls_seed")
}

/// Objects a test put in the world before the run under test started.
///
/// Arranged through the stub's own files rather than by driving the code under
/// test, so a world a suite claims to have built is never built by the thing its
/// assertions are about.
fn read_seed(dir: &Path, name: &str) -> Vec<serde_json::Value> {
    serde_json::from_str::<Vec<serde_json::Value>>(
        &std::fs::read_to_string(dir.join(name)).unwrap_or_default(),
    )
    .unwrap_or_default()
}

/// Record a pull request somebody else opened for the head and base this
/// request asked for.
///
/// The title is deliberately *not* the one the request carried: the whole point
/// of the case is that the object which makes the create a duplicate was written
/// by another actor, and an identity that read titles would fail to recognise
/// it.
fn open_pull_request_for(dir: &Path, body: &str) {
    let request: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    let mut seed = read_pull_request_seed(dir);
    seed.push(serde_json::json!({
        "head": request["head"].as_str().unwrap_or_default(),
        "base": request["base"].as_str().unwrap_or_default(),
        "title": "opened by another run",
    }));
    std::fs::write(
        dir.join("pulls_seed"),
        serde_json::Value::Array(seed).to_string(),
    )
    .unwrap();
}

/// The world is an append-only log of the mutations that actually landed, so a
/// test asserts against what happened rather than against what was answered.
///
/// The `mode` is recorded beside the mutation, and it earns its place: it is how
/// a test can assert, *of the world it is making its claims about*, that a write
/// landed under a `gh` that then died before it could say so. Without it, a
/// suite could only demonstrate the ambiguity on some other directory and hope
/// the run under test took the same route — and a test that would pass on a
/// request that simply succeeded is not yet a test of the ambiguous one.
fn apply_effect(dir: &Path, key: &str, body: &str, mode: &str) {
    if !key.starts_with("POST") && !key.starts_with("DELETE") {
        return; // a read changes nothing
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("world"))
        .unwrap();
    writeln!(
        f,
        "{}",
        serde_json::json!({ "key": key, "body": body, "mode": mode })
    )
    .unwrap();
}

/// Answer a read from the world the writes built. This is what makes the
/// exactly-once harness meaningful: after a `commit_then_*` mode, the object is
/// really there, so the fresh process's postcondition read really finds it.
fn world_answer(dir: &Path, key: &str, path: &str) -> (u16, String) {
    let world = std::fs::read_to_string(dir.join("world")).unwrap_or_default();
    let landed: Vec<serde_json::Value> = world
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let landed_key = |w: &serde_json::Value, needle: &str| {
        w["key"].as_str().unwrap_or_default().contains(needle)
    };

    if key.starts_with("GET_repos") && key.contains("git_ref_heads") {
        // `/repos/o/r/git/ref/heads/fiddle/<id>` arrives as one underscored key,
        // so the branch is everything after the endpoint and the separators go
        // back to slashes. Safe for the names this suite reads, which are
        // `fiddle/` plus a hex digest and carry no underscore of their own.
        let branch = key
            .split("git_ref_heads_")
            .nth(1)
            .unwrap_or_default()
            .replace('_', "/");
        // A ref is answered out of the bare repository beside the script,
        // because that is the only place a ref can actually come from: it must
        // point at an object the remote already holds, so it is created by a
        // real `git push` and never by a request this stub could have recorded.
        // The stub therefore mirrors the remote rather than modelling it, and
        // "what does the next process see?" is answered by the world a push
        // built and not by this fixture's idea of what a push does.
        return match bare_repository_ref(&dir.join("remote.git"), &branch) {
            Some(sha) => (200, format!(r#"{{"object":{{"sha":"{sha}"}}}}"#)),
            // An absent ref is a 404, which the adapter reads as knowledge.
            None => (404, r#"{"message":"Not Found"}"#.to_string()),
        };
    }
    // Ahead of the listing, because the listing's condition matches any key
    // containing `pulls` and would answer a by-number read with an *array* — a
    // 200 carrying the wrong shape, which a client reads as a malformed answer
    // rather than as the missing route it is.
    if let Some(answer) = pull_request_by_number(dir, path) {
        return answer;
    }
    if key.starts_with("GET_repos") && key.contains("pulls") {
        // The list endpoint filters, and the filtering is the fixture's real
        // work. A stub that answered every pull request it held whatever it was
        // asked would let a client with an unqualified — or simply wrong — query
        // pass, which is the exact defect the head-and-base identity exists to
        // prevent. `pulls_unfiltered` is the marker that switches it off, so a
        // test can put a pull request the client never asked for in front of it.
        let unfiltered = dir.join("pulls_unfiltered").exists();
        let matches: Vec<_> = pull_requests(dir)
            .into_iter()
            .filter(|pr| {
                unfiltered
                    || [
                        ("head", pr["head"]["label"].as_str()),
                        ("base", pr["base"]["ref"].as_str()),
                        ("state", pr["state"].as_str()),
                    ]
                    .into_iter()
                    .all(|(name, held)| match query_param(path, name) {
                        Some(asked) => held == Some(asked.as_str()),
                        // GitHub's own default: a parameter nobody sent
                        // constrains nothing.
                        None => true,
                    })
            })
            .collect();
        return (200, serde_json::Value::Array(matches).to_string());
    }
    if key.starts_with("GET_repos") && key.contains("commits") && key.contains("check-runs") {
        if let Some(status) = unreadable(dir, "checks_unreadable") {
            return (status, format!(r#"{{"message":"scripted {status}"}}"#));
        }
        // `/repos/{owner}/{repo}/commits/{sha}/check-runs` — the sha is the
        // fifth segment, and it is the whole of what this endpoint is addressed
        // by. The filtering is the fixture's real work: a stub that answered
        // every check run it held whatever head it was asked about would let a
        // client that observed the *branch* pass, which is the exact defect
        // observing by exact head exists to prevent. `checks_unfiltered` is the
        // marker that switches it off, so a test can put a result for a
        // superseded head in front of a client that never asked for it.
        let asked = path
            .split('?')
            .next()
            .unwrap_or_default()
            .split('/')
            .nth(5)
            .unwrap_or_default();
        let unfiltered = dir.join("checks_unfiltered").exists();
        let runs: Vec<_> = read_seed(dir, "checks_seed")
            .into_iter()
            .filter(|check| unfiltered || check["head_sha"].as_str() == Some(asked))
            .collect();
        return (
            200,
            serde_json::json!({ "total_count": runs.len(), "check_runs": runs }).to_string(),
        );
    }
    if key.starts_with("GET_repos") && key.contains("actions_workflows") {
        if let Some(status) = unreadable(dir, "runs_unreadable") {
            return (status, format!(r#"{{"message":"scripted {status}"}}"#));
        }
        // The same refusal, but only once a dispatch has actually landed — and
        // that ordering is the whole reason the marker exists separately.
        //
        // A lost answer's *classification* is observable only when the read that
        // would settle it cannot be made. If the read succeeds, the executor's
        // step 8 finds the object and reports `Committed` whatever it thought the
        // dispatch meant, so a suite whose reads always work cannot tell a
        // correctly-classified lost answer from a misclassified one. The
        // unconditional marker above cannot arrange this: it would refuse step
        // 3's read too, and the effect would fail before dispatching anything.
        if landed.iter().any(|w| landed_key(w, "dispatches")) {
            if let Some(status) = unreadable(dir, "runs_unreadable_after_a_dispatch") {
                return (status, format!(r#"{{"message":"scripted {status}"}}"#));
            }
        }
        // Runs are located by run-name, because a workflow dispatch answers 204
        // with no run id and the runs listing does not expose dispatch inputs.
        // The seeded runs come first and the dispatched ones after, so a test
        // can place its own run between two of somebody else's — which is what
        // makes "filtered by our id" distinguishable from "took the first" and
        // from "took the most recent".
        let dispatched = landed
            .iter()
            .filter(|w| landed_key(w, "dispatches"))
            .map(|w| {
                let body = parse(w["body"].as_str().unwrap_or("{}"));
                serde_json::json!({
                    "name": format!("fiddle-{}", effect_id_in(&w["body"])),
                    "status": "queued",
                    "head_branch": body["ref"],
                })
            });
        let runs: Vec<_> = read_seed(dir, "runs_seed")
            .into_iter()
            .chain(dispatched)
            // Ids start at 4200 rather than at 1 or at 0, so a test asserting on
            // an external reference cannot pass by accident against an index or
            // a count.
            .enumerate()
            .map(|(i, run)| {
                serde_json::json!({
                    "id": 4200 + i,
                    "name": run["name"],
                    "status": run["status"].as_str().unwrap_or("queued"),
                    "head_branch": run["head_branch"],
                    "event": "workflow_dispatch",
                })
            })
            .filter(|run| {
                [
                    ("branch", run["head_branch"].as_str()),
                    ("event", run["event"].as_str()),
                ]
                .into_iter()
                .all(|(name, held)| match query_param(path, name) {
                    Some(asked) => held == Some(asked.as_str()),
                    // GitHub's own default: a parameter nobody sent constrains
                    // nothing.
                    None => true,
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

/// One pull request, by its own number, from a collection of its own.
///
/// A separate collection here because it is a separate answer at GitHub. The
/// listing's entries carry the head, the base and the title; the by-number read
/// carries `draft` and `node_id` as well, and those two are the whole reason
/// `EnsurePullRequestReady` addresses this endpoint rather than the listing —
/// one read that answers both of its questions.
///
/// Matched on the raw path rather than on [`script_key`], for the reason
/// [`comment_answer`] gives: a key whose separators have all become underscores
/// cannot tell `/pulls/{n}` from `/pulls?head={n}`. The last segment has to
/// parse as a number, so nothing named rather than numbered is taken from the
/// routes it belongs to.
///
/// Unscripted is a panic naming the file. The tempting default is a 404 — a pull
/// request nobody opened, which is a real thing the world does — but that is a
/// *scenario*, and a fixture that produced it for a file a test forgot to write
/// would answer a question nobody asked. A test that wants the 404 can script
/// one; what it cannot be is the answer to an oversight.
///
/// `None` means this is not a by-number path, and the world answers it.
///
/// The file is the pull request as it was *seeded*, and the answer is that seed
/// with the mutations that have since landed applied over it — the same
/// two-sources-one-answer shape [`pull_requests`] uses for the listing, and for
/// the same reason. Both of a walk's reads address this one path, so a route that
/// answered the file verbatim answered the pre-mutation world twice, and no read
/// could ever show that the transition had happened. That is what left the
/// GraphQL route with no committed path to observe at all.
///
/// Note where the change comes from: the **world log**, which holds only mutations
/// that really landed, and not a per-call index. A counter would make the second
/// read differ from the first whether or not anything had happened, which is a
/// fixture that agrees with the property under test rather than one that measures
/// it — a refused mutation would then read as committed.
fn pull_request_by_number(dir: &Path, path: &str) -> Option<(u16, String)> {
    let bare = path.split('?').next().unwrap_or(path);
    let segments: Vec<&str> = bare.split('/').filter(|s| !s.is_empty()).collect();
    let ["repos", _, _, "pulls", number] = segments.as_slice() else {
        return None;
    };
    number.parse::<u64>().ok()?;

    let file = dir.join("pulls_by_number").join(format!("{number}.json"));
    let body = std::fs::read_to_string(&file).unwrap_or_else(|_| {
        panic!(
            "nothing scripted at {}; a pull request is answered from its file or not at all",
            file.display()
        )
    });
    Some((200, landed_transitions_applied(dir, body)))
}

/// One seeded pull request, brought up to date with the ready transitions that
/// have landed against it.
///
/// The seed is returned **byte for byte** when nothing applies, which is not
/// tidiness: a test may script a body deliberately missing `draft` or `node_id` to
/// ask what a client does with an answer it cannot read, and a route that
/// round-tripped every read through `serde_json` would quietly repair or reorder
/// what that test wrote.
///
/// Matched on the node id rather than on the number, because the node id is what
/// `markPullRequestReadyForReview` is addressed by. A mutation for some other
/// pull request therefore does not take this one out of draft, which is the
/// distinction a fixture keyed on "a mutation happened" would lose.
fn landed_transitions_applied(dir: &Path, body: String) -> String {
    let mut pull_request = parse(&body);
    let node_id = pull_request["node_id"].as_str().unwrap_or_default();
    if node_id.is_empty() || !readied_node_ids(dir).iter().any(|id| id == node_id) {
        return body;
    }
    // `draft` and nothing else. The transition takes a pull request out of draft;
    // a fixture that also rewrote the state or the head would be answering
    // questions this mutation does not ask.
    pull_request["draft"] = serde_json::Value::Bool(false);
    pull_request.to_string()
}

/// The node ids a landed `markPullRequestReadyForReview` took out of draft.
///
/// Read out of the world log, so these are the mutations that really happened —
/// including, and this is the whole point, the ones whose answer was lost on the
/// way back. Keyed off the query text because that is what says which mutation
/// was sent: `gh` addresses every GraphQL call to one endpoint, so the request is
/// the only thing that distinguishes a ready transition from anything else this
/// route may one day carry.
fn readied_node_ids(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("world"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|landed| landed["key"].as_str() == Some(GRAPHQL_KEY))
        .filter_map(|landed| {
            let request = parse(landed["body"].as_str().unwrap_or_default());
            let id = request["id"].as_str()?.to_string();
            request["query"]
                .as_str()
                .unwrap_or_default()
                .contains("markPullRequestReadyForReview")
                .then_some(id)
        })
        .collect()
}

/// The two comment collections, each answered from a directory of its own.
///
/// They are separate collections at GitHub and they are separate here, because
/// the property under test is that a work-level decision is read from the
/// conversation and from nowhere else. A fixture that served both out of one
/// directory could only demonstrate that a review comment was *filtered out*,
/// and the claim is stronger than that: the endpoint is never asked.
///
/// Routed off the raw path rather than off [`script_key`], for two reasons. The
/// collections differ by *where* the number sits — `/issues/{n}/comments`
/// against `/issues/comments/{id}` — and a key whose separators have all become
/// underscores has thrown that distinction away. And this runs ahead of
/// [`world_answer`] rather than inside it because that function's pull-request
/// route matches any key containing `pulls`, so `/pulls/{n}/comments` would be
/// answered with a listing of pull requests rather than with the decoy a test
/// put there.
///
/// `None` means this is not a comment path, and the world answers it.
fn comment_answer(dir: &Path, path: &str) -> Option<(u16, String, String)> {
    let bare = path.split('?').next().unwrap_or(path);
    let segments: Vec<&str> = bare.split('/').filter(|s| !s.is_empty()).collect();
    match segments.as_slice() {
        ["repos", _, _, "issues", "comments", id] => Some(comment_by_id(dir, id)),
        ["repos", _, _, "issues", _, "comments"] => Some(comment_page(dir, "issue-comments", path)),
        ["repos", _, _, "pulls", _, "comments"] => Some(comment_page(dir, "review-comments", path)),
        _ => None,
    }
}

/// One page of a collection, and the header that says whether there is another.
///
/// The `Link` header is the fixture's real work. A page holding one comment and
/// a page holding a hundred look the same to a client that counts what it got,
/// so a scripted conversation of three single-comment pages is only readable by
/// a client that follows the header — which is exactly the shape the suite
/// scripts.
///
/// The last page carries a `Link` too, holding `rel="first"` and `rel="prev"`,
/// because that is what GitHub sends and because a client that stopped on the
/// header being *absent* rather than on `rel="next"` being absent would run past
/// the end here rather than passing.
///
/// The header can also be scripted verbatim, by writing `page-{k}.link` beside
/// the page. That *replaces* the synthesized one rather than adding to it,
/// because the cases needing it are the ones asserting which RFC 8288 spellings
/// of a relation are read as a further page — `rel="next last"`, `rel = "next"`,
/// a relation spelled inside another parameter's value — and a synthesized
/// `rel="next"` sitting alongside would answer those either way.
///
/// A page nobody scripted is a panic naming the file, and deliberately not an
/// empty list. This route has no unscripted default at all, because the reads
/// it serves are the ones whose whole subject is completeness: an empty answer
/// is a legitimate conversation — nobody has replied yet — so a fixture that
/// produced one for a file a test forgot to write would let that test assert
/// "no approval was found" against a world it never built. A conversation that
/// really is empty is scripted by writing `page-1.json` holding `[]`, which
/// says so on purpose. The `graphql` route above still defaults to a silent
/// success; that is `fiddle-e902`'s to fix and not a pattern to copy.
fn comment_page(dir: &Path, collection: &str, path: &str) -> (u16, String, String) {
    if let Some(status) = unreadable(dir, &format!("{collection}-unreadable")) {
        return (
            status,
            String::new(),
            format!(r#"{{"message":"scripted {status}"}}"#),
        );
    }
    // GitHub's own default: a listing nobody paginated is page one.
    let page: u64 = query_param(path, "page")
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let bare_path = path.split('?').next().unwrap_or(path);
    let file = |k: u64| dir.join(collection).join(format!("page-{k}.json"));

    let body = std::fs::read_to_string(file(page)).unwrap_or_else(|_| {
        panic!(
            "nothing scripted at {}; a comment page is answered from its file or not at all",
            file(page).display()
        )
    });
    let link = |k: u64, rel: &str| {
        format!("<https://api.github.com{bare_path}?per_page=100&page={k}>; rel=\"{rel}\"")
    };
    let mut rels = Vec::new();
    if file(page + 1).exists() {
        rels.push(link(page + 1, "next"));
    }
    if page > 1 {
        rels.push(link(page - 1, "prev"));
        rels.push(link(1, "first"));
    }
    let scripted = std::fs::read_to_string(dir.join(collection).join(format!("page-{page}.link")));
    let headers = match scripted {
        Ok(header) => format!("Link: {}\r\n", header.trim_end_matches(['\r', '\n'])),
        Err(_) => match rels.is_empty() {
            true => String::new(),
            false => format!("Link: {}\r\n", rels.join(", ")),
        },
    };
    let last = !file(page + 1).exists();
    (
        200,
        headers,
        with_posted_comments(dir, collection, last, bare_path, body),
    )
}

/// The conversation collection: the one comments are *posted* to, and therefore
/// the only one whose listing has anything to merge.
///
/// Nothing in this build writes an inline review comment, so `review-comments`
/// stays a purely scripted decoy — which is what
/// `inline_review_comments_are_never_read` is about.
const CONVERSATION: &str = "issue-comments";

/// The id the first comment a `POST` created in this world is listed under.
///
/// Well clear of the small ids the suites seed by hand, and not a number that
/// could be an index, a count or a page — [`pull_requests`]' reason for starting
/// its numbering at 7 and the runs listing's for starting at 4200.
const FIRST_POSTED_COMMENT: u64 = 3001;

/// A page of the conversation with the comments this world's own `POST`s created
/// appended to it.
///
/// This is the comment collection's half of what [`pull_requests`] does for the
/// pull request listing, and it exists for the same reason: a read has to be
/// answered from the world the writes built, or the only question this fixture
/// exists to answer — *after the write landed and the answer was lost, what does
/// the next process see?* — cannot be asked. Without it a posted comment was
/// recorded in the world log and never listed, so a postcondition read taken
/// after a `commit_then_die` found nothing and a walk that had really published
/// its question looked like one that had not.
///
/// Appended to the **last** page and to no other, because the client follows
/// `rel="next"` to the end and GitHub returns a conversation oldest first: a
/// comment created now belongs after everything already there. A merge onto page
/// one would put it before comments that predate it and, worse, would repeat it
/// on every page of a paginated read.
///
/// The page is returned **byte for byte** when nothing applies, for
/// [`landed_transitions_applied`]'s reason: a test may script a body deliberately
/// missing a field to ask what the client does with an answer it cannot read, and
/// a route that round-tripped every read through `serde_json` would quietly
/// repair what that test wrote.
fn with_posted_comments(
    dir: &Path,
    collection: &str,
    last_page: bool,
    bare_path: &str,
    body: String,
) -> String {
    if collection != CONVERSATION || !last_page {
        return body;
    }
    let posted = posted_comments(dir, bare_path);
    if posted.is_empty() {
        return body;
    }
    let serde_json::Value::Array(mut listed) = parse(&body) else {
        return body;
    };
    listed.extend(posted);
    serde_json::Value::Array(listed).to_string()
}

/// The comments that landed on one conversation, in the order they were posted,
/// each in the shape the listing returns.
///
/// Read out of the world log, so these are the writes that really happened —
/// including the ones whose answer was lost on the way back, which is the whole
/// point. Keyed on the exact path rather than on "a comment was posted": a
/// question published on one pull request must not appear in another's
/// conversation, and a fixture that could not tell them apart would answer a
/// duplicate-detection test either way.
///
/// The author is a `User` with no app attribution, which is what a comment
/// written through a personal access token looks like — the credential every
/// `GhCli` in the deterministic suite carries. The Actions shape is a `Bot` and is
/// a different world; a test that needs one seeds a page instead.
fn posted_comments(dir: &Path, bare_path: &str) -> Vec<serde_json::Value> {
    // [`script_key`]'s mangling, for a path that carries no query — which a
    // comment `POST` never does. Derived here rather than matched loosely so the
    // key is the one `apply_effect` recorded and not a substring of it.
    let key = format!(
        "POST_{}",
        bare_path.trim_start_matches('/').replace('/', "_")
    );
    std::fs::read_to_string(dir.join("world"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|landed| landed["key"].as_str() == Some(key.as_str()))
        .enumerate()
        .map(|(i, landed)| {
            let request = parse(landed["body"].as_str().unwrap_or_default());
            serde_json::json!({
                "id": FIRST_POSTED_COMMENT + i as u64,
                // The body as it was *sent*. A fixture that re-rendered it would
                // make the bytes posted and the bytes read back agree by
                // construction, and whether they agree is a property under test.
                "body": request["body"].as_str().unwrap_or_default(),
                // Equal, which is what says a comment nobody has edited since —
                // true of one this very run created.
                "created_at": "2026-08-11T00:00:00Z",
                "updated_at": "2026-08-11T00:00:00Z",
                "author_association": "OWNER",
                "user": { "login": "fiddle", "id": 1_000_001, "type": "User" },
                "performed_via_github_app": serde_json::Value::Null,
            })
        })
        .collect()
}

/// One comment by its own id, from the conversation collection.
///
/// Only that collection has a by-id route here, because only that collection is
/// ever read: the re-read exists to find out whether a comment changed since it
/// was listed, and nothing lists a review comment.
///
/// Unscripted is a panic here as well, and for a sharper reason than on the
/// listing. The tempting default is a 404 — a comment deleted between the two
/// reads, which is a real thing the world does — but that is a *scenario*, and
/// a fixture that produced it for a file a test forgot to write would answer a
/// question nobody asked. When a bean needs a deletion it can be scripted; what
/// it cannot be is the answer to an oversight.
fn comment_by_id(dir: &Path, id: &str) -> (u16, String, String) {
    let file = dir
        .join("issue-comments")
        .join("by-id")
        .join(format!("{id}.json"));
    let body = std::fs::read_to_string(&file).unwrap_or_else(|_| {
        panic!(
            "nothing scripted at {}; a comment is answered from its file or not at all",
            file.display()
        )
    });
    (200, String::new(), body)
}

/// What a bare repository's `refs/heads/<branch>` points at, or `None` when it
/// holds no such ref.
///
/// Read out of the repository's own files rather than by spawning a `git`: this
/// fixture stands in for GitHub's ref endpoint, and a fixture that shelled out
/// would be one more child inside a suite whose whole subject is which children
/// get spawned with what. Both storage forms are handled, because a repository
/// is free to pack a ref at any time and a fixture that only understood loose
/// refs would fail on the day one did.
fn bare_repository_ref(remote: &Path, branch: &str) -> Option<String> {
    let loose = remote.join("refs").join("heads").join(branch);
    if let Ok(sha) = std::fs::read_to_string(loose) {
        return Some(sha.trim().to_string());
    }
    let packed = std::fs::read_to_string(remote.join("packed-refs")).ok()?;
    let wanted = format!("refs/heads/{branch}");
    packed.lines().find_map(|line| {
        let (sha, name) = line.split_once(' ')?;
        (name.trim() == wanted).then(|| sha.trim().to_string())
    })
}

fn effect_id_in(body: &serde_json::Value) -> String {
    parse(body.as_str().unwrap_or("{}"))["inputs"]["fiddle_effect_id"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

/// The status a read has been made to fail with, when a test asked for one.
///
/// Scoped to an endpoint by marker file rather than keyed on the mangled request
/// path, so a suite asking "what does this client do when the source is
/// unreadable?" does not have to spell the client's own query string back at it
/// — and so the answer to that question cannot quietly become "the read
/// succeeded" the day the query changes.
fn unreadable(dir: &Path, marker: &str) -> Option<u16> {
    std::fs::read_to_string(dir.join(marker))
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// One recorded request body, read back as JSON. `Null` when it was not one.
fn parse(body: &str) -> serde_json::Value {
    serde_json::from_str(body).unwrap_or(serde_json::Value::Null)
}
