mod support;

use support::{accepted, calls, reports, Reply, Scenario, StubGateway};

const WORK_ID: &str = "fiddle-m1-demo";
const INVOCATION_REF: &str = "beans:fiddle-m1-demo";

const CREDENTIAL: &str = "LITELLM_API_KEY";

const SENTINEL: &str = "sk-sentinel-must-never-be-printed-9f3a1c";

const REDACTED: &str = "[redacted]";

const SWITCH: &str = "FIDDLE_TRANSCRIPT";

const ON: &str = "1";

const TRANSCRIPT_DIR: &str = "transcript";

const ELAPSED: &str = "elapsed_ms";

const TOOK: &str = "duration_ms";

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

fn a_repair_that_quotes(key: &str) -> Vec<Reply> {
    vec![
        accepted(calls(
            "write_file",
            serde_json::json!({
                "path": "src/lib.rs",
                "contents": support::REPAIRED_FIXTURE,
            }),
        )),
        accepted(reports(serde_json::json!({
            "changed_files": ["src/lib.rs"],
            "summary": format!("the gateway said: Incorrect API key provided: {key}"),
            "claimed_complete": true,
        }))),
    ]
}

struct Ran {
    status: Option<i32>,
    stdout: String,
    stderr: String,
}

fn repair(s: &Scenario, switch: Option<&str>) -> Ran {
    let mut command = s.run_command(INVOCATION_REF);
    command
        .args(["--capability", "fixture_repair", "--json"])
        .env(CREDENTIAL, SENTINEL);
    match switch {
        Some(value) => command.env(SWITCH, value),
        None => command.env_remove(SWITCH),
    };
    let out = command.output().unwrap();
    Ran {
        status: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn transcripts(s: &Scenario) -> Vec<std::path::PathBuf> {
    let dir = s.report_dir().join(TRANSCRIPT_DIR);
    if !dir.exists() {
        return Vec::new();
    }
    let mut found: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .collect();
    found.sort();
    found
}

fn only_transcript(s: &Scenario) -> std::path::PathBuf {
    let found = transcripts(s);
    assert_eq!(
        found.len(),
        1,
        "one run writes one transcript, and this one wrote {found:?}"
    );
    found.into_iter().next().unwrap()
}

fn records(path: &std::path::Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .unwrap_or_else(|why| panic!("the transcript at {} is unreadable: {why}", path.display()))
        .lines()
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|why| panic!("a transcript line is not JSON ({why}): {line}"))
        })
        .collect()
}

#[test]
fn the_switch_off_writes_no_transcript_and_says_nothing() {
    let gateway = StubGateway::serving(a_repair_that_quotes(SENTINEL));
    let s = scenario(&gateway);

    let ran = repair(&s, None);

    assert_eq!(
        ran.status,
        Some(0),
        "the repair must complete: stdout = {} stderr = {}",
        ran.stdout,
        ran.stderr
    );
    assert!(
        transcripts(&s).is_empty(),
        "a run that was not asked for a transcript wrote {:?}",
        transcripts(&s)
    );
    assert!(
        !ran.stderr.contains("transcript") && !ran.stdout.contains("transcript"),
        "a run that wrote no transcript must not mention one: stdout = {} stderr = {}",
        ran.stdout,
        ran.stderr
    );
}

#[test]
fn the_switch_on_writes_the_transcript_and_says_it_did() {
    let gateway = StubGateway::serving(a_repair_that_quotes(SENTINEL));
    let s = scenario(&gateway);

    let ran = repair(&s, Some(ON));

    assert_eq!(
        ran.status,
        Some(0),
        "one input separates this run from the last, and the outcome must not \
         change: stdout = {} stderr = {}",
        ran.stdout,
        ran.stderr
    );
    let path = only_transcript(&s);
    assert!(
        ran.stderr.contains(&path.display().to_string()),
        "a run that wrote a transcript must name the file it wrote: {}",
        ran.stderr
    );
    assert!(
        ran.stderr.contains("the model's replies"),
        "the note must say what the file carries: {}",
        ran.stderr
    );
}

#[test]
fn the_transcript_carries_the_model_response_and_not_the_credential() {
    let gateway = StubGateway::serving(a_repair_that_quotes(SENTINEL));
    let s = scenario(&gateway);

    let ran = repair(&s, Some(ON));
    assert_eq!(ran.status, Some(0), "stderr = {}", ran.stderr);

    let path = only_transcript(&s);
    let whole = std::fs::read_to_string(&path).unwrap();
    assert!(
        !whole.contains(SENTINEL),
        "the credential reached the transcript at {}: {whole}",
        path.display()
    );
    assert!(
        whole.contains("Incorrect API key provided: [redacted]"),
        "the model's reply must survive with the credential replaced: {whole}"
    );
    assert_eq!(
        whole.matches(REDACTED).count(),
        1,
        "the reply quoted the credential once: {whole}"
    );

    let records = records(&path);
    let kinds: Vec<&str> = records
        .iter()
        .map(|record| record["record"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds.first(),
        Some(&"brief"),
        "the transcript opens with what fiddle sent the model: {kinds:?}"
    );
    for expected in ["sent", "spent", "received", "tool"] {
        assert!(
            kinds.contains(&expected),
            "the transcript is missing its `{expected}` records: {kinds:?}"
        );
    }

    let brief = &records[0];
    assert_eq!(brief["tool_choice"], "required", "{brief}");
    assert_eq!(brief["max_turns"], 4, "{brief}");
    assert!(
        brief["tools"].as_str().unwrap().contains("read_file"),
        "the brief record must name the set the run offered: {brief}"
    );
    assert!(
        brief["preamble"].as_str().unwrap().contains("write_file"),
        "the brief record must carry the preamble the model was sent: {brief}"
    );

    let answered = records
        .iter()
        .find(|record| record["record"] == "tool")
        .expect("the run made one tool call");
    assert_eq!(answered["tool"], "write_file", "{answered}");
    assert!(
        answered["args"].as_str().unwrap().contains("src/lib.rs"),
        "a tool record must carry the arguments the model chose: {answered}"
    );
}

#[test]
fn every_record_carries_an_elapsed_value_that_never_decreases() {
    let gateway = StubGateway::serving(a_repair_that_quotes(SENTINEL));
    let s = scenario(&gateway);

    let ran = repair(&s, Some(ON));
    assert_eq!(ran.status, Some(0), "stderr = {}", ran.stderr);

    let records = records(&only_transcript(&s));
    assert!(
        records.len() > 4,
        "one repair writes a brief, both sides of two turns, and one tool call: \
         {records:?}"
    );
    let elapsed: Vec<u64> = records
        .iter()
        .map(|record| {
            record[ELAPSED]
                .as_u64()
                .unwrap_or_else(|| panic!("every record carries its elapsed time: {record}"))
        })
        .collect();
    assert_eq!(
        elapsed[0], 0,
        "the brief is the origin every later record is measured from: {elapsed:?}"
    );
    assert!(
        elapsed.windows(2).all(|pair| pair[0] <= pair[1]),
        "a time that falls down the file is worse than none, because a reader \
         will trust it: {elapsed:?}"
    );
    assert!(
        elapsed[elapsed.len() - 1] > 0,
        "two round trips to the gateway and one write take longer than a \
         millisecond, so a file of zeroes is a clock that never started: \
         {elapsed:?}"
    );

    let answered = records
        .iter()
        .find(|record| record["record"] == "tool")
        .expect("the run made one tool call");
    assert!(
        answered[TOOK].is_u64(),
        "a four-minute check and a four-millisecond read must not read alike: \
         {answered}"
    );
}

#[test]
fn the_finish_reason_of_every_response_reaches_the_transcript() {
    let gateway = StubGateway::serving(a_repair_that_quotes(SENTINEL));
    let s = scenario(&gateway);

    let ran = repair(&s, Some(ON));
    assert_eq!(ran.status, Some(0), "stderr = {}", ran.stderr);

    let records = records(&only_transcript(&s));
    let finished: Vec<&serde_json::Value> = records
        .iter()
        .filter(|record| record["record"] == "finish")
        .collect();
    let reasons: Vec<&str> = finished
        .iter()
        .map(|record| {
            record["reason"]
                .as_str()
                .unwrap_or_else(|| panic!("a finish record names the reason: {record}"))
        })
        .collect();
    assert_eq!(
        reasons,
        vec!["tool_calls", "stop"],
        "this run asked the gateway twice, and the transcript must carry the \
         reason each response ended with: {records:?}"
    );

    let spent: Vec<u64> = records
        .iter()
        .filter(|record| record["record"] == "spent")
        .map(|record| record["turn"].as_u64().unwrap())
        .collect();
    let turns: Vec<u64> = finished
        .iter()
        .map(|record| record["turn"].as_u64().unwrap())
        .collect();
    assert_eq!(
        turns, spent,
        "a finish reason must name the turn whose response it ended: \
         {records:?}"
    );
}

#[test]
fn no_host_fact_reaches_the_transcript() {
    let gateway = StubGateway::serving(a_repair_that_quotes(SENTINEL));
    let s = scenario(&gateway);

    let ran = repair(&s, Some(ON));
    assert_eq!(ran.status, Some(0), "stderr = {}", ran.stderr);

    let path = only_transcript(&s);
    let whole = std::fs::read_to_string(&path).unwrap();
    let root = s.dir().display().to_string();
    assert!(
        !root.is_empty(),
        "the scan would pass vacuously against an empty root"
    );
    assert!(
        !whole.contains(&root),
        "the transcript carries the host root {root}, which the model was never \
         given: {whole}"
    );
}

#[test]
fn a_switch_value_the_run_cannot_read_is_refused_before_any_work() {
    let gateway = StubGateway::serving(a_repair_that_quotes(SENTINEL));
    let s = scenario(&gateway);

    let ran = repair(&s, Some("true"));

    assert_eq!(
        ran.status,
        Some(2),
        "an unreadable switch is invalid input: stdout = {} stderr = {}",
        ran.stdout,
        ran.stderr
    );
    assert!(
        ran.stderr.contains(SWITCH) && ran.stderr.contains("true"),
        "the refusal must name the variable and the value it was given: {}",
        ran.stderr
    );
    assert_eq!(
        gateway.served(),
        0,
        "the run must refuse before it reaches the model"
    );
    assert!(
        transcripts(&s).is_empty(),
        "a refused run must write nothing: {:?}",
        transcripts(&s)
    );
}
