mod support;

use support::{a_real_repair, accepted, completion, Reply, Scenario, StubGateway};

const WORK_ID: &str = "fiddle-m1-demo";
const INVOCATION_REF: &str = "beans:fiddle-m1-demo";

const CREDENTIAL: &str = "LITELLM_API_KEY";

const SENTINEL: &str = "sk-sentinel-must-never-be-printed-9f3a1c";

const SWITCH: &str = "FIDDLE_TRANSCRIPT";

const ON: &str = "1";

const TRANSCRIPT_DIR: &str = "transcript";

const RETRIES: usize = 2;

const EMPTY: &str = "Response contained no message or tool call (empty)";

const PASSES_ONCE_REPAIRED: &str =
    "{ program = \"grep\", args = [\"-q\", \"len - 1\", \"src/lib.rs\"] }";

fn scenario(gateway: &StubGateway) -> Scenario {
    let s = Scenario::new();
    s.write_work_item(WORK_ID, "open");
    let fixture = s.write_fixture_repo();
    s.append_config(&format!(
        "[agent]\n\
         model = \"a-model\"\n\
         base_url = \"{}\"\n\
         api_key = {{ env = \"{CREDENTIAL}\" }}\n\
         max_turns = 4\n\
         max_tokens = 512\n\
         max_changed_files = 4\n\
         deadline = \"300s\"\n\
         tool_timeout = \"300s\"\n\
         \n\
         [workspace]\n\
         root = {}\n\
         fixture = {}\n\
         check = {PASSES_ONCE_REPAIRED}\n\
         command_timeout = \"300s\"\n",
        gateway.base_url(),
        support::toml_string(&s.dir().join("workspaces")),
        support::toml_string(&fixture),
    ));
    s
}

fn emptily() -> Reply {
    accepted(completion(
        serde_json::json!({ "role": "assistant", "content": "" }),
        "stop",
    ))
}

fn empty_once_then_a_repair() -> Vec<Reply> {
    let mut script = vec![emptily()];
    script.extend(a_real_repair());
    script
}

fn only_empty() -> Vec<Reply> {
    (0..RETRIES + 1).map(|_| emptily()).collect()
}

struct Ran {
    status: Option<i32>,
    stdout: String,
    stderr: String,
}

fn repair(s: &Scenario) -> Ran {
    let out = s
        .run_command(INVOCATION_REF)
        .args(["--capability", "fixture_repair", "--json"])
        .env(CREDENTIAL, SENTINEL)
        .env(SWITCH, ON)
        .output()
        .unwrap();
    Ran {
        status: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn records(s: &Scenario) -> Vec<serde_json::Value> {
    let dir = s.report_dir().join(TRANSCRIPT_DIR);
    let mut found: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|why| panic!("no transcript directory at {}: {why}", dir.display()))
        .flatten()
        .map(|entry| entry.path())
        .collect();
    found.sort();
    assert_eq!(
        found.len(),
        1,
        "one run writes one transcript, and this one wrote {found:?}"
    );
    std::fs::read_to_string(&found[0])
        .unwrap()
        .lines()
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|why| panic!("a transcript line is not JSON ({why}): {line}"))
        })
        .collect()
}

fn of_kind(records: &[serde_json::Value], kind: &str) -> Vec<serde_json::Value> {
    records
        .iter()
        .filter(|record| record["record"] == kind)
        .cloned()
        .collect()
}

#[test]
fn a_gateway_that_answers_emptily_once_is_asked_again_and_the_repair_completes() {
    let gateway = StubGateway::serving(empty_once_then_a_repair());
    let s = scenario(&gateway);

    let ran = repair(&s);

    assert_eq!(
        ran.status,
        Some(0),
        "one empty response must not discard the turns the deployment paid for: \
         stdout = {} stderr = {}",
        ran.stdout,
        ran.stderr
    );
    assert_eq!(
        gateway.served(),
        3,
        "the empty response, the retry that carried the same request, and the \
         turn after it"
    );

    let records = records(&s);
    let retried = of_kind(&records, "retry");
    assert_eq!(
        retried.len(),
        1,
        "a retry that leaves no record turns an intermittent fault into an \
         invisible one: {records:?}"
    );
    assert_eq!(retried[0]["turn"], 1, "{}", retried[0]);
    assert_eq!(retried[0]["retries"], 1, "{}", retried[0]);
    assert_eq!(retried[0]["bound"], RETRIES as u64, "{}", retried[0]);
    assert_eq!(retried[0]["reason"], EMPTY, "{}", retried[0]);
    assert!(
        of_kind(&records, "unanswered").is_empty(),
        "the turn was answered on the retry: {records:?}"
    );
    assert_eq!(
        records[0]["max_retries"], RETRIES as u64,
        "the first record states every bound the attempt holds: {}",
        records[0]
    );
}

#[test]
fn a_gateway_that_only_answers_emptily_ends_the_attempt_after_the_stated_bound() {
    let gateway = StubGateway::serving(only_empty());
    let s = scenario(&gateway);

    let ran = repair(&s);

    assert_eq!(
        ran.status,
        Some(11),
        "a provider that never answered did not do the work, and repeating the \
         run may well succeed: stdout = {} stderr = {}",
        ran.stdout,
        ran.stderr
    );
    let payload: serde_json::Value = serde_json::from_str(&ran.stdout)
        .unwrap_or_else(|why| panic!("stdout is not JSON ({why}): {}", ran.stdout));
    let reason = payload["outcome"]["retryable"]["reason"]
        .as_str()
        .unwrap_or_else(|| panic!("an unanswered attempt concludes retryably: {payload}"));
    assert!(
        reason.contains(EMPTY),
        "the run must name the fault it gave up on: {reason}"
    );
    assert_eq!(
        gateway.served(),
        RETRIES + 1,
        "the first call and {RETRIES} retries, and the attempt asks no more"
    );

    let records = records(&s);
    let retried = of_kind(&records, "retry");
    assert_eq!(
        retried.len(),
        RETRIES,
        "each retry is recorded: {records:?}"
    );
    let turns: Vec<u64> = retried
        .iter()
        .map(|record| record["turn"].as_u64().unwrap())
        .collect();
    assert_eq!(
        turns,
        vec![1; RETRIES],
        "every retry answered the first turn: {records:?}"
    );
    let unanswered = of_kind(&records, "unanswered");
    assert_eq!(
        unanswered.len(),
        1,
        "the file must say why it stops, or a reader guesses: {records:?}"
    );
    assert_eq!(
        unanswered[0]["retries"], RETRIES as u64,
        "{}",
        unanswered[0]
    );
    assert_eq!(unanswered[0]["reason"], EMPTY, "{}", unanswered[0]);
    assert!(
        of_kind(&records, "finish").is_empty(),
        "no response ended, so no response names a reason: {records:?}"
    );
}
