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
        response_body(&dir, status, mode, &key)
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
///
/// A comment create answers with the comment it created, because the real endpoint
/// does and because [`HumanInteractionPort::request`] reads the id out of it. The
/// id is **the one [`posted_comments`] will list it under** rather than a number of
/// this function's own: GitHub's create and GitHub's listing agree about a
/// comment's id, and a fixture whose two halves disagreed would let a client that
/// invented an id from the response pass a test asserting the id it read back.
fn response_body(dir: &Path, status: u16, mode: &str, key: &str) -> String {
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
    // Read *after* `apply_effect` has appended this write, so the last comment landed
    // on this path is the one just created — and read out of the log rather than
    // recounted, which is what makes the create's answer and the listing's entry one
    // value instead of two derivations that agree until they do not.
    if status < 400 && is_conversation_comment_post(key) {
        return serde_json::json!({ "id": last_posted_comment_id(dir, key) }).to_string();
    }
    // A pull request create answers with the pull request it created, because
    // the real endpoint does and because a label is applied through
    // `/issues/{n}/labels` — a path there is no way to address without the
    // number. Read *after* [`apply_effect`] has appended this create, and read
    // through [`pull_requests`], so the number the create answers is the number
    // the listing will report it under: a fixture whose two halves disagreed
    // would let a client that invented one pass a test asserting the one it
    // reads back.
    if status < 400 && is_pull_request_create(key) {
        let number = pull_requests(dir)
            .last()
            .and_then(|pr| pr["number"].as_u64())
            .unwrap_or_else(|| panic!("a create landed under {key} and the world holds none"));
        return serde_json::json!({ "number": number }).to_string();
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
///
/// **There is no unscripted default**, and the reason is inline below: a route
/// whose omission answers a success is a route where forgetting to script one
/// looks exactly like meaning it.
fn graphql_answer(dir: &Path) -> (u16, serde_json::Value, String) {
    let counter = dir.join("graphql_calls");
    let n: usize = std::fs::read_to_string(&counter)
        .ok()
        .and_then(|seen| seen.trim().parse().ok())
        .unwrap_or(0);
    std::fs::write(&counter, (n + 1).to_string()).unwrap();

    // A call nobody scripted is a panic naming the file, and deliberately not the
    // silent 200 this route used to extend as a courtesy. The courtesy made an
    // omission indistinguishable from a deliberate case: a test that forgot to
    // script an answer still passed, and passed *through the success path*, which
    // for this route means applying a mutation to the world. The same argument
    // [`comment_page`] makes about an empty conversation, and sharper here — a
    // GraphQL 200 is the one answer whose verdict lives in the body, so a
    // fabricated one is a fabricated verdict.
    //
    // The REST route's `201 0 normal` default is not this and stays: it answers a
    // status line, where a test whose subject is the request really does not have
    // to say what came back, and a REST verdict is not something the fixture has
    // to invent.
    //
    // Loud enough to be diagnosable: the panic leaves stdout empty, so the client
    // reads no status line and reports `GhError::Malformed` — which is the one
    // failure that quotes `stderr`, so the missing filename reaches whoever is
    // looking at the test output.
    let file = dir.join("graphql").join(format!("{n}.json"));
    let scripted = std::fs::read_to_string(&file).unwrap_or_else(|_| {
        // Printed before the panic rather than only inside it, and that is not
        // belt and braces: the client quotes `stderr` through a 120-character
        // bound, and a panic's own `thread … panicked at <file>:<line>` prefix
        // eats most of it — so the filename this diagnostic exists to name would
        // be truncated out of the one place a test author reads it.
        eprintln!(
            "nothing scripted at {}; a GraphQL call is answered from its file or not at all",
            file.display()
        );
        panic!("no scripted GraphQL answer");
    });
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
/// A seed entry may name its own number, and the numbers a world holds must be
/// distinct, so a positional default has to skip over the named ones.
///
/// A named number is what makes an anomaly arrangeable: the shared-pull-request
/// lanes seed 57, 41 and 63 in that order precisely so that *the lowest*, *the
/// first* and *the last* are three different answers, which positional numbering
/// makes impossible. Unnamed entries keep the old rule — `7 + i` and never `1`,
/// so an assertion on an external reference cannot pass against an index — and
/// take the next free number at or above it.
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

    let all: Vec<serde_json::Value> = seeded.into_iter().chain(created).collect();
    let named: Vec<u64> = all.iter().filter_map(|pr| pr["number"].as_u64()).collect();

    let mut next = 7u64;
    all.iter()
        .map(|pr| {
            let head = pr["head"].as_str().unwrap_or_default().to_string();
            let bare = head.split_once(':').map(|(_, r)| r).unwrap_or(&head);
            let number = pr["number"].as_u64().unwrap_or_else(|| {
                while named.contains(&next) {
                    next += 1;
                }
                let mine = next;
                next += 1;
                mine
            });
            serde_json::json!({
                "number": number,
                // Named by the seed when a test needs a closed one, and `open`
                // otherwise — which is every world that predates the label
                // search. A closed pull request is reachable only by asking for
                // one, so no existing lane's world changed shape.
                "state": pr["state"].as_str().unwrap_or("open"),
                "title": pr["title"].as_str().unwrap_or_default(),
                // GitHub's own shape: the head is a `label` of `owner:branch`
                // beside the bare `ref`, and the base is a `ref` alone.
                "head": {
                    "label": head,
                    "ref": bare,
                    // The tip, read out of the bare repository beside this script
                    // for [`bare_repository_ref`]'s reason. A head sha is a fact
                    // about the remote, and a fixture that invented one could
                    // report a tip the remote does not hold — which is precisely
                    // the disagreement a run that checks the reported sha out
                    // would then be unable to survive. `null` when the remote has
                    // no such branch, so a world that never put one there gets a
                    // malformed answer rather than a blank carried forward as a
                    // revision.
                    "sha": bare_repository_ref(&dir.join("remote.git"), bare),
                },
                "base": { "ref": pr["base"].as_str().unwrap_or_default() },
                // Carried because the listing really does: GitHub's pulls
                // listing answers full pull request objects, and
                // `EnsurePullRequest`'s postcondition now asks whether the one
                // it found is labelled.
                "labels": labels_of(dir, number, pr),
                // A create sends a body and the object keeps it, so a pull
                // request this world created can be read by number like one it
                // was seeded with.
                "body": pr["body"].clone(),
                "draft": pr["draft"].as_bool().unwrap_or(false),
            })
        })
        .collect()
}

/// Every label the world holds for pull request `number`.
///
/// Two sources, one list, for [`pull_requests`]'s reason. A seeded entry names
/// its labels directly — a world a test arranged — and a created one acquires
/// them through the `POST /repos/{o}/{r}/issues/{n}/labels` that
/// `EnsurePullRequest::apply` makes after the create, read out of the **world
/// log** so that only the label calls that really landed count.
///
/// That second half is the whole of what makes
/// `a_created_pull_request_carries_the_label_that_finds_it` a test rather than a
/// tautology: the label reaches this world only by the client actually sending
/// it, so a create that skipped the second request produces a pull request this
/// function reports as unlabelled — and the discovery read then cannot find it.
fn labels_of(dir: &Path, number: u64, seeded: &serde_json::Value) -> Vec<serde_json::Value> {
    let name =
        |it: &serde_json::Value| serde_json::json!({ "name": it.as_str().unwrap_or_default() });
    let mut labels: Vec<serde_json::Value> = seeded["labels"]
        .as_array()
        .map(|it| it.iter().map(name).collect())
        .unwrap_or_default();

    let applied = format!("_issues_{number}_labels");
    for landed in std::fs::read_to_string(dir.join("world"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
    {
        let key = landed["key"].as_str().unwrap_or_default();
        if !key.starts_with("POST") || !key.ends_with(&applied) {
            continue;
        }
        let body = parse(landed["body"].as_str().unwrap_or("{}"));
        if let Some(sent) = body["labels"].as_array() {
            labels.extend(sent.iter().map(name));
        }
    }
    labels
}

/// The plain issues — not pull requests — a test put in the world.
///
/// A separate seed because they are separate objects, and the discovery read's
/// whole difficulty is that GitHub's label search answers about both: a pull
/// request *is* an issue there, and only the `pull_request` key says which is
/// which. A world that could not hold a labelled ordinary issue could not put
/// that confusion in front of a client.
fn read_issue_seed(dir: &Path) -> Vec<serde_json::Value> {
    read_seed(dir, "issues_seed")
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
/// The comment's own id is minted **here**, at post time, and recorded beside the
/// write — for a conversation `POST` and for nothing else.
///
/// # Why the id is recorded rather than derived later
///
/// It used to be derived positionally, `FIRST_POSTED_COMMENT + i` within a path, by
/// whoever read the log. That put **two independent numbering schemes over one
/// conversation**: this one, which knew nothing about what a test had seeded, and
/// `World::post_comment`'s `max(id) + 1`, which knew nothing about this one. Measured:
/// a run's question was 9000, a person's reply 9001, a second reply 9002 — and a
/// redirect's second question was **9001 again**.
///
/// The consequence is not cosmetic. [`comment_by_id`] reports a duplicate rather than
/// choosing from it, and step 5 of the decision walk re-reads the request comment by
/// id, so **no third process could run in a world where a redirect had asked again**.
/// That is a ceiling on what any scenario can drive, and a redirect asking again after
/// a reply is the ordinary case rather than a defect.
///
/// Minting at post time removes the second scheme instead of reconciling the two:
/// there is now one moment at which a comment acquires an id, and it is the moment
/// GitHub would assign one.
///
/// # Above everything the world holds, and never below the floor
///
/// [`FIRST_POSTED_COMMENT`] stays as a **floor** rather than a base. Its reason is
/// still good — `decision_request_effect.rs` states it: an id far from zero and far
/// from one cannot pass by accident against an index, a count or a page number — and a
/// floor keeps that for the ordinary case of a `POST` onto an empty conversation,
/// which is what every assertion naming `9000` is about.
///
/// Above **every** comment the world holds and not only this conversation's, which is
/// also what GitHub does: an issue comment id is unique across the repository, not
/// within a thread. Numbering per path would leave two conversations' first comments
/// sharing an id, and a fixture cannot both do that and answer a by-id read.
fn apply_effect(dir: &Path, key: &str, body: &str, mode: &str) {
    // `PATCH` alongside the other two, because a body update is a mutation and a
    // world that dropped it could not answer the one question
    // `EnsurePullRequestBody`'s postcondition asks — *does the pull request say
    // this now?* Read out of this log by [`landed_body_rewrites`], so the answer a
    // second run gets is a rewrite that really landed, including the ones whose
    // answer was lost on the way home.
    if !key.starts_with("POST") && !key.starts_with("DELETE") && !key.starts_with("PATCH") {
        return; // a read changes nothing
    }
    let mut landed = serde_json::json!({ "key": key, "body": body, "mode": mode });
    if is_conversation_comment_post(key) {
        landed["comment_id"] = serde_json::json!(next_comment_id(dir));
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("world"))
        .unwrap();
    writeln!(f, "{landed}").unwrap();
}

/// Whether `key` is a `POST` of a comment onto a pull request's **conversation**.
///
/// One predicate rather than the same three conditions written at each of the four
/// places that need them — the mint, the listing, the by-id union and the create's own
/// answer. They have to agree about which writes carry an id, and three copies of a
/// condition is how two numbering schemes came to exist in the first place.
///
/// `_issues_` and not `_pulls_`: the inline review collection is a different
/// collection, nothing lists it, and [`with_posted_comments`] merges only the
/// conversation.
fn is_conversation_comment_post(key: &str) -> bool {
    key.starts_with("POST_repos_") && key.contains("_issues_") && key.ends_with("_comments")
}

/// Whether `key` is a `POST` onto the **pulls collection** — a create.
///
/// Ends at `_pulls`, so `POST_repos_o_r_pulls_7_reviews` and anything else
/// addressed at a pull request that already exists is not one. The distinction
/// matters because only a create has a number to answer with.
fn is_pull_request_create(key: &str) -> bool {
    key.starts_with("POST_repos_") && key.ends_with("_pulls")
}

/// The id a comment posted **now** gets: one above the highest the world already
/// holds, and never below [`FIRST_POSTED_COMMENT`].
///
/// Read through [`listed_conversation_comments`], which is the union of both sources a
/// listing draws on — the page files a test wrote and the comments earlier `POST`s
/// created. Reading one of them would reinstate exactly the blindness this replaces:
/// over the log alone it cannot see a seeded reply, and over the pages alone it cannot
/// see its own earlier question.
fn next_comment_id(dir: &Path) -> u64 {
    let highest = listed_conversation_comments(dir)
        .iter()
        .filter_map(|comment| comment["id"].as_u64())
        .max();
    match highest {
        Some(highest) => (highest + 1).max(FIRST_POSTED_COMMENT),
        None => FIRST_POSTED_COMMENT,
    }
}

/// The id of the last comment posted under `key` — the one a create is answering
/// about, because [`apply_effect`] has already appended it.
fn last_posted_comment_id(dir: &Path, key: &str) -> u64 {
    std::fs::read_to_string(dir.join("world"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|landed| landed["key"].as_str() == Some(key))
        .filter_map(|landed| landed["comment_id"].as_u64())
        .next_back()
        .unwrap_or_else(|| {
            panic!("no comment has landed under {key}, so no create can be answered about one")
        })
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
    // The label search, which is answered about **issues** because that is where
    // GitHub keeps labels — and a pull request is an issue there, which is the
    // whole reason the endpoint can answer this question and the whole reason its
    // answer needs telling apart. Placed before the pulls listing because the
    // key contains neither `pulls` nor a number, so nothing above would claim it.
    if key.starts_with("GET_repos") && key.contains("_issues") && !key.contains("comments") {
        // Filtered, for the pulls listing's reason: a stub that answered every
        // labelled object whatever it was asked would let a client with no label
        // in its query pass, which is the exact defect the label-as-discriminator
        // model exists to prevent. `issues_unfiltered` switches it off so a test
        // can put an object the client never asked for in front of it.
        let unfiltered = dir.join("issues_unfiltered").exists();
        let asked: Vec<String> = query_param(path, "labels")
            .unwrap_or_default()
            .split(',')
            .filter(|it| !it.is_empty())
            .map(str::to_string)
            .collect();

        // Pull requests carry `pull_request`; plain issues do not. Nothing else
        // in the answer distinguishes them, which is GitHub's own arrangement.
        let pulls = pull_requests(dir).into_iter().map(|pr| {
            let number = pr["number"].as_u64().unwrap_or_default();
            serde_json::json!({
                "number": number,
                "state": pr["state"],
                "title": pr["title"],
                "labels": pr["labels"],
                "pull_request": { "url": format!("/repos/o/r/pulls/{number}") },
            })
        });
        let issues = read_issue_seed(dir).into_iter().map(|issue| {
            serde_json::json!({
                "number": issue["number"],
                "state": "open",
                "title": issue["title"],
                "labels": issue["labels"]
                    .as_array()
                    .map(|it| it
                        .iter()
                        .map(|name| serde_json::json!({ "name": name }))
                        .collect::<Vec<_>>())
                    .unwrap_or_default(),
            })
        });

        let matches: Vec<_> = pulls
            .chain(issues)
            .filter(|it| {
                unfiltered
                    || (asked.iter().all(|wanted| {
                        it["labels"].as_array().is_some_and(|held| {
                            held.iter().any(|l| l["name"].as_str() == Some(wanted))
                        })
                    }) && match query_param(path, "state") {
                        Some(state) => it["state"].as_str() == Some(state.as_str()),
                        None => true,
                    })
            })
            .collect();
        return (200, serde_json::Value::Array(matches).to_string());
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
    let body = match std::fs::read_to_string(&file) {
        Ok(scripted) => scripted,
        // Not scripted, but **the world holds it** — because it was seeded into
        // the listing or created by a `POST` that really landed. Answering from
        // the world is not "the answer to an oversight" the panic below refuses;
        // it is the same object the listing is already describing, and GitHub
        // answers about it by number too. A fixture whose two routes disagreed
        // about a pull request it visibly holds would be the defect, not the
        // convenience.
        //
        // Kept behind the file, so every test that scripted one keeps exactly the
        // bytes it wrote — including the deliberately incomplete ones.
        Err(_) => match pull_requests(dir)
            .into_iter()
            .find(|pr| pr["number"].as_u64().map(|it| it.to_string()).as_deref() == Some(*number))
        {
            Some(held) => held.to_string(),
            // Still a panic, and for the reason it always was: the tempting
            // default is a 404 — a pull request nobody opened, which is a real
            // thing the world does — but that is a *scenario*, and a fixture
            // that produced it for a number nothing holds would answer a
            // question nobody asked.
            None => panic!(
                "nothing scripted at {} and no pull request numbered {number} in this \
                 world; a pull request is answered from its file, from the world, or \
                 not at all",
                file.display()
            ),
        },
    };
    Some((200, landed_transitions_applied(dir, number, body)))
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
///
/// The body rewrite is the second transition this route replays, and it is keyed
/// on the **number** rather than on a node id because `PATCH /pulls/{n}` is
/// addressed by one. Same rule either way: a rewrite of some other pull request
/// does not change this one's body.
fn landed_transitions_applied(dir: &Path, number: &str, body: String) -> String {
    let readied = {
        let node_id = parse(&body)["node_id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        !node_id.is_empty() && readied_node_ids(dir).iter().any(|id| id == &node_id)
    };
    let rewritten = landed_body_rewrites(dir, number);
    if !readied && rewritten.is_none() {
        return body;
    }

    let mut pull_request = parse(&body);
    if readied {
        // `draft` and nothing else. The transition takes a pull request out of
        // draft; a fixture that also rewrote the state or the head would be
        // answering questions this mutation does not ask.
        pull_request["draft"] = serde_json::Value::Bool(false);
    }
    if let Some(rewritten) = rewritten {
        pull_request["body"] = serde_json::Value::String(rewritten);
    }
    pull_request.to_string()
}

/// The body the **last** landed `PATCH` left on this pull request, if any.
///
/// The last and not the first, because that is what a sequence of rewrites means:
/// a run that corrects its own description twice leaves the second sentence
/// standing, and a fixture that answered the first would let a stale body pass as
/// a satisfied postcondition.
///
/// Read out of the world log, so these are the writes that really happened —
/// including, and this is the point of the log, the ones whose answer was lost on
/// the way back. A `PATCH` that carried no `body` key changes nothing here: the
/// endpoint's other fields are none of this route's business, and inventing an
/// empty description for one would be the fixture answering a question the
/// request did not ask.
fn landed_body_rewrites(dir: &Path, number: &str) -> Option<String> {
    let key = format!("_pulls_{number}");
    std::fs::read_to_string(dir.join("world"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|landed| {
            landed["key"]
                .as_str()
                .is_some_and(|k| k.starts_with("PATCH_repos_") && k.ends_with(&key))
        })
        .filter_map(|landed| {
            parse(landed["body"].as_str().unwrap_or_default())["body"]
                .as_str()
                .map(str::to_string)
        })
        .next_back()
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
/// says so on purpose. The `graphql` route above now answers the same way, for
/// the same reason — it used to default to a silent success, which `fiddle-e902`
/// withdrew.
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
const FIRST_POSTED_COMMENT: u64 = 9000;

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
/// The author is a `Bot`, because fiddle is one. A request comment is fiddle's
/// own question and never anybody's answer to it, and a fixture that returned it
/// as an ordinary user would let a validation walk with that id on its allowlist
/// count the question as a reply. `is_bot` is the field
/// `validate::select_candidates` refuses on, so this is the shape that keeps the
/// fixture from quietly supplying the thing under test.
///
/// The id is **read** and never derived. [`apply_effect`] minted it at post time,
/// above everything the world held; a reader that recomputed it positionally is the
/// second numbering scheme that gave one conversation two comments with one id, and
/// that function records what it cost. An entry with no id is a panic rather than a
/// fallback, because the fallback *is* the defect: it would number this comment from a
/// base again and disagree with whatever the create answered.
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
        .map(|landed| {
            let request = parse(landed["body"].as_str().unwrap_or_default());
            let id = landed["comment_id"].as_u64().unwrap_or_else(|| {
                panic!(
                    "a comment landed under {key} with no id recorded at post time: \
                     {landed}"
                )
            });
            serde_json::json!({
                "id": id,
                // The body as it was *sent*. A fixture that re-rendered it would
                // make the bytes posted and the bytes read back agree by
                // construction, and whether they agree is a property under test.
                "body": request["body"].as_str().unwrap_or_default(),
                // Equal, which is what says a comment nobody has edited since —
                // true of one this very run created.
                "created_at": "2026-08-11T00:00:00Z",
                "updated_at": "2026-08-11T00:00:00Z",
                "author_association": "OWNER",
                "user": { "login": "fiddle[bot]", "id": 1_000_001, "type": "Bot" },
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
/// # The scripted file wins, and it has to
///
/// A test that writes `by-id/<id>.json` is saying *this is what the re-read
/// returns, whatever the listing says* — which is the only way to express an
/// **edit between the two reads**, the one thing step 5 of the validation order
/// exists to catch. `decision_protocol`'s `with_edited_approval` builds exactly
/// that: one listing entry and one by-id entry differing only in `updated_at`.
/// So the file is consulted first and the fallback below never overrides one.
///
/// # And when nothing is scripted, the listing answers — which it could not before
///
/// This route used to panic for **every** unscripted id, on the argument that a
/// default 404 would be a *scenario* answering an oversight. That argument still
/// holds for a 404 and is why there is none. What it did not justify was refusing
/// to answer about a comment **this world visibly holds**: the route knew nothing
/// of the two sources the listing draws on, so a comment the world's own `POST`
/// created could not be re-read at all.
///
/// **The consequence was that no continuation walk had ever been driven against a
/// posted question** — only against comments a test wrote straight into a by-id
/// file. The step whose whole purpose is comparing a re-read against a listing had
/// been exercised solely against inputs a test constructed on both sides. That is
/// the same shape as this fixture's other constructor-mistaken-for-observer traps,
/// one layer beneath them, and `fiddle-565u` is where it surfaced.
///
/// So the fallback is [`listed_conversation_comments`], the *union of both sources
/// the listing merges*, and it is deliberately not one of them: a fallback over the
/// world log alone answers fiddle's own `POST` and not a reply a test seeded onto a
/// page, and a fallback over the page files alone does the reverse. Step 5 re-reads
/// **the request comment and every candidate**, so answering one kind and not the
/// other leaves the walk refusing exactly where it did before.
///
/// An unmatched id is still a panic, now saying both things it looked at, because
/// "no file and no such comment" and "no file" are different oversights.
///
/// Two comments sharing an id is reported and never chosen from — the rule this
/// fixture applies to duplicate pull requests and duplicate request comments, for
/// the same reason: there is no principled way to pick, and a world holding two is
/// one somebody assembled.
///
/// # That arm was reachable, and the comment that said otherwise is why nobody looked
///
/// **Corrected. This used to say: *"Relaxing it to take the first of several broke no
/// test, because nothing constructs a duplicate: a seeded comment is numbered above
/// everything the conversation shows, and a posted one from `FIRST_POSTED_COMMENT`. The
/// one collision `World::post_comment` documents — a run that wrongly posted a second
/// question after a reply was seeded — needs the very defect a continuation exists not
/// to have."*** The last sentence was false, and it was the load-bearing one.
///
/// A **legitimate** redirect posts a second question after a reply. No defect is
/// required: the two numbering schemes were what collided, not a run misbehaving. The
/// converged
/// `human_direction::a_redirect_produces_a_different_change_and_asks_again_about_it`
/// already left a duplicate-id world behind, harmlessly, because nothing read by id
/// afterwards — and step 5 of the decision walk re-reads by id, so the real cost was
/// that **no third process could be driven in such a world at all**. That was a
/// ceiling on what any later scenario could reach, and the comment is why it read as a
/// cosmetic clash.
///
/// The two schemes are now one: [`apply_effect`] mints a comment's id at post time,
/// above every comment the world holds, so a duplicate is not merely unconstructed but
/// unconstructible from this fixture's own writers.
///
/// The arm is kept, because the alternative is not "no branch", it is *taking the
/// first*, and this match has to be total. A fail-closed arm nothing can reach is not
/// the same thing as a check that cannot fail: it adds no confidence, and it removes
/// the option of silently answering about the wrong comment — which is a behaviour
/// forbidden two paragraphs above, for duplicate pull requests and duplicate request
/// comments, on the argument that there is no principled way to pick. Said here so a
/// later reader does not mistake its untestedness for deadness, and so that the reason
/// it is unreachable is a property of the minting rather than an accident somebody can
/// undo without noticing.
fn comment_by_id(dir: &Path, id: &str) -> (u16, String, String) {
    let file = dir
        .join("issue-comments")
        .join("by-id")
        .join(format!("{id}.json"));
    if let Ok(body) = std::fs::read_to_string(&file) {
        return (200, String::new(), body);
    }

    let listed: Vec<serde_json::Value> = listed_conversation_comments(dir)
        .into_iter()
        .filter(|comment| {
            comment["id"]
                .as_u64()
                .is_some_and(|held| held.to_string() == id)
        })
        .collect();
    match listed.as_slice() {
        [only] => (200, String::new(), only.to_string()),
        [] => panic!(
            "nothing scripted at {}, and this world's conversation lists no comment \
             {id}: a comment is answered from its file, or from the listing, or not \
             at all",
            file.display()
        ),
        many => panic!(
            "{} comments in this world share id {id}, and a re-read is reported \
             rather than chosen from",
            many.len()
        ),
    }
}

/// Every comment the conversation collection would list, from **both** of the
/// sources its listing draws on.
///
/// The page files a test wrote, in page order, and then the comments this world's
/// own `POST`s created — which is the order and the composition
/// [`with_posted_comments`] produces for a read of the last page, and it is written
/// as one function rather than two so the by-id route and the listing route cannot
/// come to disagree about what the conversation holds.
///
/// The posted half is gathered per conversation path rather than over every `POST`
/// at once, because a question published on one pull request must not appear in
/// another's conversation and [`posted_comments`] is keyed on the exact path.
/// Discovering the paths from the world log rather than taking one as an argument is
/// what lets a by-id read — whose path names no issue — be answered at all.
///
/// **Corrected: this used to say the grouping was needed because `posted_comments`
/// numbers from `FIRST_POSTED_COMMENT` *within* a path, "and pooling the writes would
/// give two conversations' first comments the same id".** That reason is gone —
/// [`apply_effect`] mints an id above every comment the world holds, in any
/// conversation, so pooling would no longer collide. The grouping is kept for the
/// reason that was always the real one and is stated first above: which conversation a
/// comment belongs to is a property, and a fixture that could not tell two
/// conversations apart would answer a duplicate-detection test either way.
fn listed_conversation_comments(dir: &Path) -> Vec<serde_json::Value> {
    let mut found = Vec::new();

    // The seeded half: whole page files, and never `by-id/*.json` beside them,
    // which is why the pages are walked by number rather than by globbing.
    let pages = dir.join(CONVERSATION);
    let mut page = 1;
    while let Ok(text) = std::fs::read_to_string(pages.join(format!("page-{page}.json"))) {
        if let Ok(serde_json::Value::Array(listed)) = serde_json::from_str(&text) {
            found.extend(listed);
        }
        page += 1;
    }

    // The posted half, per conversation. A `POST` to `/repos/o/r/issues/{n}/comments`
    // is recorded under the mangled key, so the paths are read back out of it.
    let mut conversations: Vec<String> = std::fs::read_to_string(dir.join("world"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter_map(|landed| landed["key"].as_str().map(str::to_string))
        .filter(|key| is_conversation_comment_post(key))
        .collect();
    conversations.sort();
    conversations.dedup();
    for key in conversations {
        // Un-mangling a key into a path cannot recover an owner or repository whose
        // own name held an underscore — `script_key` threw that apart. It does not
        // have to: [`posted_comments`] re-mangles whatever path it is given by the
        // same substitution, so key → path → key round-trips **exactly** whether or
        // not the middle step is a real path. The path is an intermediate, and the
        // key is what selects the writes.
        let path = format!("/{}", key.trim_start_matches("POST_").replace('_', "/"));
        found.extend(posted_comments(dir, &path));
    }
    found
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
