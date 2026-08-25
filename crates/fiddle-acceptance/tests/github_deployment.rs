mod support;

use std::path::{Path, PathBuf};
use support::Scenario;

const WORK_ID: &str = "fiddle-m0-demo";
const INVOCATION_REF: &str = "beans:fiddle-m0-demo";

const CREDENTIAL: &str = "FIDDLE_GITHUB_TOKEN";

const SENTINEL: &str = "ghp_sentinel_github_deployment_must_never_print_7c31";

fn publishable() -> (Scenario, PathBuf) {
    let scenario = Scenario::new();
    scenario.write_work_item(WORK_ID, "open");
    let work = scenario.write_fixture_repo();
    (scenario, work)
}

fn head_sha(work: &Path) -> String {
    let out = std::process::Command::new("git")
        .current_dir(work)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(out.status.success(), "could not read HEAD of {work:?}");
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn gh_answering_nothing_exists(dir: &Path) -> PathBuf {
    write_gh(
        dir,
        "empty-gh",
        "printf 'HTTP/1.1 404 Not Found\\r\\n\\r\\n{\"message\":\"Not Found\"}\\n'\nexit 1\n",
    )
}

fn gh_answering_the_branch_is_published(dir: &Path, sha: &str) -> PathBuf {
    write_gh(
        dir,
        "published-gh",
        &format!(
            "for a in \"$@\"; do path=\"$a\"; done\n\
             case \"$path\" in\n\
             \x20 */git/ref/heads/*)\n\
             \x20   printf 'HTTP/1.1 200 OK\\r\\n\\r\\n{{\"object\":{{\"sha\":\"{sha}\"}}}}\\n' ;;\n\
             \x20 */pulls*)\n\
             \x20   printf 'HTTP/1.1 200 OK\\r\\n\\r\\n[]\\n' ;;\n\
             \x20 *)\n\
             \x20   printf 'HTTP/1.1 404 Not Found\\r\\n\\r\\n{{\"message\":\"Not Found\"}}\\n'\n\
             \x20   exit 1 ;;\n\
             esac\n"
        ),
    )
}

#[cfg(unix)]
fn write_gh(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn forge_table(scenario: &Scenario, gh: &Path, work: &Path, extra: &str) -> String {
    format!(
        "[github]\n\
         repo = \"peel/fiddle-effects-acceptance\"\n\
         base = \"main\"\n\
         token = {{ env = \"{CREDENTIAL}\" }}\n\
         cli = {{ program = {gh}, args = [] }}\n\
         git = \"git\"\n\
         work = {work}\n\
         workflow = \"verify.yml\"\n\
         config_dir = {config_dir}\n\
         timeout = \"120s\"\n\
         {extra}",
        gh = toml_path(gh),
        work = toml_path(work),
        config_dir = toml_path(&scenario.dir().join("gh-config")),
    )
}

fn toml_path(path: &Path) -> String {
    format!("{:?}", path.display().to_string())
}

fn publish(scenario: &Scenario) -> std::process::Output {
    scenario
        .run_command(INVOCATION_REF)
        .args(["--capability", "publish_change", "--json"])
        .env(CREDENTIAL, SENTINEL)
        .output()
        .unwrap()
}

fn summary_of(payload: &serde_json::Value) -> String {
    payload["progress"][0]["summary"]
        .as_str()
        .unwrap_or_else(|| panic!("a run that executed publishes one progress entry: {payload}"))
        .to_string()
}

fn payload_of(out: &std::process::Output) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}): {stdout}\nstderr = {}",
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

#[test]
fn run_constructs_and_executes_the_publishing_capability() {
    let (s, work) = publishable();
    let gh = gh_answering_nothing_exists(s.dir());
    s.append_config(&forge_table(&s, &gh, &work, ""));

    let out = publish(&s);
    let payload = payload_of(&out);

    assert_eq!(
        payload["capability_executions"][0]["capability_id"], "publish_change",
        "the run must execute the capability it was asked for, got {payload}"
    );
    assert_eq!(
        payload["progress"][0]["stage"], "publish",
        "the executed capability names its own stage, got {payload}"
    );
    assert_ne!(
        payload["outcome"],
        serde_json::Value::Null,
        "the run must have reached a conclusion, got {payload}"
    );
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        !stderr.contains("[github]"),
        "the capability is configured and must no longer be refused: {stderr}"
    );
    assert_eq!(
        out.status.code(),
        Some(11),
        "with nothing on the far end of `git push`, the run fails — but it \
         fails having executed: {payload}\nstderr = {stderr}"
    );
}

#[test]
fn a_deployment_rule_in_the_document_refuses_the_effect_it_names() {
    let (s, work) = publishable();
    let gh = gh_answering_nothing_exists(s.dir());
    s.append_config(&forge_table(
        &s,
        &gh,
        &work,
        "\n[github.policy]\nensure_branch_published = \"deny\"\n",
    ));

    let out = publish(&s);
    let payload = payload_of(&out);
    let summary = summary_of(&payload);

    assert_eq!(
        out.status.code(),
        Some(20),
        "a refused effect will not succeed by being repeated, so it is not \
         retryable: {payload}"
    );
    assert_eq!(payload["capability_executions"][0]["status"], "failed");
    assert!(
        summary.contains("policy denied"),
        "the document's rule must produce a policy refusal, got {payload}"
    );
    assert!(
        summary.contains("ensure_branch_published"),
        "the refusal must name the effect the rule was written for, in the \
         spelling the document uses, got {payload}"
    );
}

#[test]
fn the_same_world_without_the_rule_is_not_refused_by_policy() {
    let (s, work) = publishable();
    let gh = gh_answering_nothing_exists(s.dir());
    s.append_config(&forge_table(&s, &gh, &work, ""));

    let payload = payload_of(&publish(&s));
    assert!(
        !summary_of(&payload).contains("policy denied"),
        "nothing in this document denies anything, and something did: {payload}"
    );
}

#[test]
fn a_rule_written_for_one_kind_refuses_that_kind_and_not_the_one_before_it() {
    let (s, work) = publishable();
    let gh = gh_answering_the_branch_is_published(s.dir(), &head_sha(&work));
    s.append_config(&forge_table(
        &s,
        &gh,
        &work,
        "\n[github.policy]\nensure_pull_request = \"deny\"\n",
    ));

    let payload = payload_of(&publish(&s));
    let summary = summary_of(&payload);

    assert!(
        summary.contains("policy denied") && summary.contains("ensure_pull_request"),
        "the pull request is the effect the document names, got {payload}"
    );
    assert!(
        !summary.contains("ensure_branch_published"),
        "the branch carries no rule and must not have been refused, got {payload}"
    );
    let evidence = payload["progress"][0]["evidence"].to_string();
    assert!(
        evidence.contains("effect:ensure_branch_published:"),
        "the branch effect must have produced a receipt, got {payload}"
    );
}

#[test]
fn the_forge_credential_reaches_no_surface() {
    let (s, work) = publishable();
    let gh = gh_answering_nothing_exists(s.dir());
    s.append_config(&forge_table(&s, &gh, &work, ""));

    let out = publish(&s);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert_eq!(
        payload_of(&out)["capability_executions"][0]["capability_id"],
        "publish_change",
        "the credential was resolved and the capability ran: {stdout}"
    );
    assert!(
        !stdout.contains(SENTINEL) && !stderr.contains(SENTINEL),
        "the token reached a surface: {stdout}{stderr}"
    );
    for (path, bytes) in s.project_tree() {
        assert!(
            !String::from_utf8_lossy(&bytes).contains(SENTINEL),
            "the token was written to {path}"
        );
    }
}

#[test]
fn a_publication_without_its_credential_names_the_variable() {
    let (s, work) = publishable();
    let gh = gh_answering_nothing_exists(s.dir());
    s.append_config(&forge_table(&s, &gh, &work, ""));

    let out = s
        .run_command(INVOCATION_REF)
        .args(["--capability", "publish_change", "--json"])
        .env_remove(CREDENTIAL)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    assert_eq!(out.status.code(), Some(2), "stderr = {stderr}");
    assert!(
        stderr.contains(CREDENTIAL),
        "name the variable, not the value: {stderr}"
    );
    assert!(!s.report_dir().exists(), "a refused run published nothing");
}

#[test]
fn a_publication_names_the_table_or_key_the_document_is_missing() {
    let cases: [(&str, &str); 3] = [
        ("", "[github]"),
        ("no-work", "github.work"),
        ("no-workflow", "github.workflow"),
    ];
    for (shape, expected) in cases {
        let (s, work) = publishable();
        let gh = gh_answering_nothing_exists(s.dir());
        match shape {
            "" => {}
            "no-work" => s.append_config(
                &forge_table(&s, &gh, &work, "")
                    .replace(&format!("work = {}\n", toml_path(&work)), ""),
            ),
            _ => s.append_config(
                &forge_table(&s, &gh, &work, "").replace("workflow = \"verify.yml\"\n", ""),
            ),
        }

        let out = publish(&s);
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        assert_eq!(out.status.code(), Some(2), "{shape}: stderr = {stderr}");
        assert!(
            stderr.contains(expected),
            "{shape}: the refusal must name {expected}, got {stderr}"
        );
    }
}

#[test]
fn inspect_names_the_publishing_capability_without_building_it() {
    let s = Scenario::new();
    s.write_work_item(WORK_ID, "open");

    let out = s
        .command()
        .args([
            "inspect",
            INVOCATION_REF,
            "--config",
            s.config_path().to_str().unwrap(),
            "--capability",
            "publish_change",
            "--json",
        ])
        .env_remove(CREDENTIAL)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    assert_eq!(out.status.code(), Some(0), "stderr = {stderr}");
    let payload: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        payload["next_action"]["execute"]["capability_id"], "publish_change",
        "inspect must name the capability it was asked about, got {payload}"
    );
    assert!(
        !s.report_dir().exists(),
        "inspect is read-only and published nothing"
    );
}
