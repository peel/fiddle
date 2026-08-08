mod support;

use regex::Regex;

#[test]
fn version_reports_package_version_and_source_revision() {
    let output = support::fiddle_command().arg("--version").output().unwrap();
    assert!(output.status.success(), "exit={:?}", output.status.code());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let re = Regex::new(r"^fiddle \d+\.\d+\.\d+ \((?:[0-9a-f]{40}|unknown)\)\n$").unwrap();
    assert!(
        re.is_match(&stdout),
        "unexpected --version output: {stdout:?}"
    );
}
