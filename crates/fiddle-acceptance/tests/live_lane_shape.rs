use std::path::{Path, PathBuf};

const LANE: &str = ".github/workflows/cve-live.yml";
const EXEMPLAR: &str = ".github/workflows/github-effects.yml";
const GATE: &str = "scripts/gate.sh";
const GUARD: &str = "Require the live credentials";
const PREFLIGHT: &str = "Require a Cargo workspace on the dispatched ref";
const STEP: &str = "      - ";

#[test]
fn the_repository_carries_an_opt_in_live_lane() {
    let path = repo_root().join(LANE);
    assert!(
        path.is_file(),
        "{} does not exist, so nothing exercises the real forge, the real \
         scanner and real CI feedback",
        path.display()
    );
}

#[test]
fn a_human_dispatches_the_lane_and_no_schedule_starts_it() {
    let block = trigger_block(&read(LANE));
    assert!(
        block.contains("workflow_dispatch"),
        "{LANE} does not trigger on `workflow_dispatch`, so an operator cannot \
         start it. block = {block:?}"
    );
    for other in ["schedule", "push", "pull_request", "issue_comment"] {
        assert!(
            !block.contains(other),
            "{LANE} also triggers on `{other}`. This lane needs a credential, \
             and M0's rule says no acceptance lane is gated on a secret. A \
             trigger that fires without an operator makes the lane part of the \
             offline gate. block = {block:?}"
        );
    }
}

#[test]
fn no_step_of_the_lane_can_skip_itself() {
    let src = strip_comments(&read(LANE));
    for key in ["if", "continue-on-error"] {
        let hits: Vec<&str> = src
            .lines()
            .filter(|line| names_key(line, key))
            .map(str::trim)
            .collect();
        assert!(
            hits.is_empty(),
            "`{key}:` appears in {LANE}, so this lane can report success \
             without running. A lane that skips in silence looks exactly like \
             a lane that passes. hits = {hits:?}"
        );
    }
}

#[test]
fn the_credential_guard_is_the_first_step() {
    let src = strip_comments(&read(LANE));
    let first = steps(&src)
        .first()
        .cloned()
        .unwrap_or_else(|| panic!("{LANE} declares no step"));
    assert!(
        first.contains(GUARD),
        "the first step of {LANE} is not the credential guard `{GUARD}`. An \
         absent credential must cost seconds, not a build and a scan. first \
         step = {first:?}"
    );
}

#[test]
fn the_guard_tests_every_credential_the_lane_reads() {
    let src = strip_comments(&read(LANE));
    let guard = step_named(&src, GUARD);
    let block = run_block(&guard);
    let declared = secrets(&src);
    assert!(
        !declared.is_empty(),
        "{LANE} reads no secret at all, so it exercises nothing live"
    );
    for name in &declared {
        assert!(
            block.contains(name),
            "{LANE} reads the secret `{name}`, and the guard never tests it. \
             The lane then starts, and it fails deep inside a run, or it \
             passes over an empty value. guard = {block:?}"
        );
    }
    assert!(
        block.contains("exit 1"),
        "the guard of {LANE} has no `exit 1`, so it reports success with no \
         credential. guard = {block:?}"
    );
}

#[test]
fn the_lane_builds_the_binary_from_the_dispatched_ref() {
    let src = strip_comments(&read(LANE));
    let preflight = position_of(&src, PREFLIGHT);
    let toolchain = position_of(&src, "uses: dtolnay/rust-toolchain");
    let build = position_of(&src, "cargo build --release");
    assert!(
        preflight < toolchain && preflight < build,
        "{LANE} must check the dispatched ref for a Cargo workspace before it \
         installs a toolchain and before it builds. A ref with no Cargo.toml \
         then fails on the name of the problem, and not forty lines into a \
         build log. preflight = {preflight}, toolchain = {toolchain}, build = \
         {build}"
    );
}

#[test]
fn the_lane_runs_the_binary_against_the_real_forge_and_the_real_scanner() {
    let src = read(LANE);
    for evidence in ["run cve", "wizcli", "docker build"] {
        assert!(
            src.contains(evidence),
            "{LANE} does not name `{evidence}`. The lane proves the deployment, \
             so it runs the released command over a real image with the real \
             scanner"
        );
    }
}

#[test]
fn the_lane_reads_real_ci_feedback_on_a_second_run() {
    let runs = read(LANE).matches("run cve").count();
    assert!(
        runs >= 2,
        "{LANE} runs `fiddle run cve` {runs} time(s). A later run reads the \
         open pull request's check runs, so one run cannot prove the CI \
         feedback path"
    );
}

#[test]
fn the_offline_gate_never_reaches_the_lane() {
    let gate = read(GATE);
    assert!(
        !gate.contains("cve-live"),
        "{GATE} names the live lane, so the offline gate now needs a \
         credential. M0's rule says no acceptance lane is gated on a secret"
    );
    for neighbour in workflows() {
        let name = neighbour.file_name().unwrap_or_default().to_string_lossy();
        let is_lane = name == "cve-live.yml";
        let calls = read_path(&neighbour).contains("cve-live");
        assert!(
            is_lane || !calls,
            "{name} names the live lane. A workflow that another trigger \
             starts would run the credentialled lane outside an operator's \
             dispatch"
        );
    }
}

#[test]
fn the_lane_holds_the_two_properties_the_exemplar_holds() {
    let exemplar = strip_comments(&read(EXEMPLAR));
    let first = steps(&exemplar)
        .first()
        .cloned()
        .unwrap_or_else(|| panic!("{EXEMPLAR} declares no step"));
    assert!(
        first.contains("Require FIDDLE_EFFECTS_TOKEN"),
        "the credential guard of {EXEMPLAR} is no longer its first step. This \
         lane copies that property, so the exemplar must still hold it. first \
         step = {first:?}"
    );
    assert!(
        !exemplar.lines().any(|line| names_key(line, "if")),
        "{EXEMPLAR} carries an `if:`, so the property this lane copies is gone \
         from the lane that established it"
    );
}

#[test]
fn the_key_scan_reads_a_key_and_not_a_word_that_holds_one() {
    assert!(names_key("        if: false", "if"));
    assert!(names_key("        - if: false", "if"));
    assert!(names_key(
        "        continue-on-error: true",
        "continue-on-error"
    ));
    assert!(
        !names_key("        run: echo \"a gift, if you like\"", "if"),
        "prose that holds the word `if` is not the key `if:`, and a scan that \
         confuses the two refuses a lane for its comments"
    );
    assert!(
        !names_key("        name: verify", "if"),
        "a step name that ends in `if` is not the key `if:`"
    );
}

#[test]
fn the_comment_strip_removes_a_comment_and_keeps_the_shell_it_documents() {
    let src = "      - name: A step\n        # if: false\n        run: echo 1\n";
    let stripped = strip_comments(src);
    assert!(!stripped.contains("if: false"));
    assert!(
        stripped.contains("run: echo 1"),
        "the strip must keep the lane, or every scan over it measures nothing"
    );
}

fn names_key(line: &str, key: &str) -> bool {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("- ").unwrap_or(trimmed);
    rest.strip_prefix(key)
        .map(|tail| tail.trim_start().starts_with(':'))
        .unwrap_or(false)
}

fn strip_comments(src: &str) -> String {
    src.lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

fn steps(src: &str) -> Vec<String> {
    let mut steps: Vec<String> = Vec::new();
    for line in src.lines() {
        match line.starts_with(STEP) {
            true => steps.push(line.to_string()),
            false => {
                if let Some(current) = steps.last_mut() {
                    match line.starts_with("        ") || line.trim().is_empty() {
                        true => {
                            current.push('\n');
                            current.push_str(line);
                        }
                        false => break,
                    }
                }
            }
        }
    }
    steps
}

fn step_named(src: &str, name: &str) -> String {
    steps(src)
        .into_iter()
        .find(|step| step.contains(name))
        .unwrap_or_else(|| panic!("{LANE} declares no step named `{name}`"))
}

fn run_block(step: &str) -> String {
    step.lines()
        .skip_while(|line| line.trim() != "run: |")
        .skip(1)
        .collect::<Vec<_>>()
        .join("\n")
}

fn secrets(src: &str) -> Vec<String> {
    let mut names: Vec<String> = src
        .split("secrets.")
        .skip(1)
        .map(|tail| {
            tail.chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect::<String>()
        })
        .filter(|name| !name.is_empty())
        .collect();
    names.sort();
    names.dedup();
    names
}

fn position_of(src: &str, needle: &str) -> usize {
    src.lines()
        .position(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("no line of the lane holds `{needle}`"))
}

fn workflows() -> Vec<PathBuf> {
    let dir = repo_root().join(".github/workflows");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("could not read {} ({e})", dir.display()))
        .map(|entry| entry.expect("a readable directory entry").path())
        .filter(|path| path.extension().is_some_and(|kind| kind == "yml"))
        .collect();
    files.sort();
    files
}

fn trigger_block(src: &str) -> String {
    src.lines()
        .skip_while(|line| line.trim_end() != "on:")
        .skip(1)
        .take_while(|line| line.starts_with(' ') || line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn read(relative: &str) -> String {
    read_path(&repo_root().join(relative))
}

fn read_path(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("could not read {} ({e})", path.display()))
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the manifest directory is two levels below the repository root")
}
