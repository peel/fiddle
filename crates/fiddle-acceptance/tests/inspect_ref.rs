mod support;

use support::Scenario;

#[test]
fn inspect_echoes_a_parsed_invocation_ref() {
    let out = support::fiddle_command()
        .args([
            "inspect",
            "beans:fiddle-m0-demo",
            "--config",
            "../../tests/fixtures/fiddle.toml",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr = {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["invocation_ref"], "beans:fiddle-m0-demo");
    assert_eq!(v["scheme"], "beans");
}

#[test]
fn inspect_accepts_a_self_discovering_scheme_with_no_value() {
    let out = support::fiddle_command()
        .args([
            "inspect",
            "cve",
            "--config",
            "../../tests/fixtures/fiddle.toml",
            "--json",
        ])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert_ne!(
        out.status.code(),
        Some(2),
        "`cve` is a complete reference and must not be refused as usage; stderr = {stderr}"
    );
    assert!(
        !stderr.contains("<scheme>:<value>"),
        "the grammar must not demand a value it has no use for; stderr = {stderr}"
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!("a parsed reference is echoed on stdout: {e}; stderr = {stderr}")
    });
    assert_eq!(v["invocation_ref"], "cve", "echoed bare, never as `cve:`");
    assert_eq!(v["scheme"], "cve");
}

#[test]
fn inspect_rejects_a_malformed_invocation_ref() {
    let mut diagnostics = Vec::new();
    for (arg, needle) in [
        ("bogus", "is not an invocation reference"),
        ("mystery:x", "unknown invocation scheme"),
        ("beans:", "must not be empty"),
        ("cve:", "must not be empty"),
        ("beans:../../../pwned", "ASCII letters, digits"),
    ] {
        let out = support::fiddle_command()
            .args([
                "inspect",
                arg,
                "--config",
                "../../tests/fixtures/fiddle.toml",
            ])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "arg={arg}");
        let stderr = String::from_utf8(out.stderr).unwrap();
        assert!(stderr.contains(needle), "arg={arg} stderr={stderr}");
        assert!(
            out.stdout.is_empty(),
            "a rejected reference must write nothing to stdout; arg={arg}"
        );
        diagnostics.push((arg, stderr));
    }

    for (i, (arg, stderr)) in diagnostics.iter().enumerate() {
        for (other_arg, other) in &diagnostics[i + 1..] {
            assert_ne!(
                stderr, other,
                "`{arg}` and `{other_arg}` fail for different reasons and must not \
                 be reported with the same diagnostic"
            );
        }
    }
}

#[test]
fn a_malformed_reference_is_reported_without_reference_to_configuration() {
    let out = support::fiddle_command()
        .args(["inspect", "bogus", "--config", "no/such/fiddle.toml"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("is not an invocation reference"),
        "stderr={stderr}"
    );
    assert!(!stderr.contains("no/such/fiddle.toml"), "stderr={stderr}");
}

#[test]
fn a_traversing_reference_creates_nothing_anywhere() {
    let s = Scenario::new();
    s.write_work_item("fiddle-m0-demo", "open");
    let before = s.project_tree();

    let out = s.run_raw_with(&["--json"], "beans:../../../pwned");

    assert_eq!(
        out.status.code(),
        Some(2),
        "a reference that is not an identifier is invalid input, stderr = {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.stdout.is_empty(),
        "a rejected reference must write nothing to stdout, got {}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert_eq!(
        s.project_tree(),
        before,
        "a refused reference must leave the project tree byte for byte as it found it"
    );
    assert!(
        !s.report_dir().exists(),
        "a refused reference must not bring `<report.dir>` into existence"
    );
    assert!(
        !s.dir().join("pwned").exists(),
        "nothing may be created outside the roots the configuration names"
    );
}

#[test]
fn an_empty_value_is_told_every_repair_its_own_scheme_admits() {
    let refuse = |arg: &str| {
        let out = support::fiddle_command()
            .args([
                "inspect",
                arg,
                "--config",
                "../../tests/fixtures/fiddle.toml",
            ])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "arg={arg}");
        String::from_utf8(out.stderr).unwrap()
    };

    let sweep = refuse("cve:");
    assert!(
        sweep.contains("discovers its own work"),
        "a scheme that stands alone must be told so, or the bare form is a repair \
         the operator has no way to learn about: {sweep}"
    );
    assert!(
        sweep.contains("`cve`"),
        "the bare form is one of the two repairs and must be spelled: {sweep}"
    );
    assert!(
        valued_cve_mentions(&sweep).is_empty(),
        "the valued form is not implemented in this build, so offering it as a repair \
         sends a caller from one refusal to another: {sweep}"
    );

    let tracked = refuse("beans:");
    assert!(
        !tracked.contains("discovers its own work"),
        "`beans` names a work item and `beans` alone is refused, so offering the \
         bare form here would be advice that does not parse: {tracked}"
    );
    assert!(
        tracked.contains("beans:fiddle-m0-demo"),
        "the one repair a tracked scheme has is to name the work: {tracked}"
    );
}

#[test]
fn an_empty_value_after_an_unknown_scheme_is_not_told_its_scheme_is_recognised() {
    let out = support::fiddle_command()
        .args([
            "inspect",
            "notascheme:",
            "--config",
            "../../tests/fixtures/fiddle.toml",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8(out.stderr).unwrap();

    assert!(
        stderr.contains("must not be empty"),
        "the empty value is still the defect reported, before the scheme is \
         looked up: {stderr}"
    );
    assert!(
        !stderr.contains("the scheme is recognised"),
        "`notascheme` is not one of the schemes, so telling this caller it is \
         recognised is a confident and wrong diagnosis: {stderr}"
    );
    assert!(
        stderr.contains("not one fiddle knows"),
        "the defect the caller can act on is the scheme, and it has to be \
         named: {stderr}"
    );
    let advice = stderr.split_whitespace().collect::<Vec<_>>().join(" ");
    let Some((takes_a_value, needs_none)) = advice.split_once("discover their own work need none:")
    else {
        panic!(
            "the advice has to separate the schemes that require a value from the ones \
             that do not, or it is offering one shape to a set that has two: {advice}"
        );
    };
    for (scheme, stands_alone) in [
        ("beans", false),
        ("jira", false),
        ("scheduled", false),
        ("scanner", false),
        ("cve", true),
    ] {
        if stands_alone {
            assert!(
                needs_none.contains(scheme),
                "`{scheme}` discovers its own work, so a caller who meant it has to be \
                 told the bare form: {advice}"
            );
            assert!(
                !takes_a_value.contains(scheme),
                "`{scheme}` stands alone, so advising this caller to write \
                 `{scheme}:<identifier>` sends someone who wanted a sweep to a read of \
                 one named finding — different work, and the reason the scheme carries \
                 its own advice at all: {advice}"
            );
        } else {
            assert!(
                takes_a_value.contains(scheme),
                "`{scheme}` names a work item and is refused without one, so it belongs \
                 where a value is required: {advice}"
            );
            assert!(
                !needs_none.contains(scheme),
                "`{scheme}` written alone does not parse, so listing it among the \
                 schemes that need no value is advice refused when followed: {advice}"
            );
        }
    }
}

#[test]
fn the_valued_form_of_a_self_discovering_scheme_is_refused() {
    let s = Scenario::new();
    let before = s.project_tree();

    for extra in [Vec::new(), vec!["--capability", "stub_mark"]] {
        let out = s.run_raw_with(&extra, "cve:CVE-2026-1234");
        let stderr = String::from_utf8(out.stderr).unwrap();
        assert_eq!(
            out.status.code(),
            Some(2),
            "a form this build cannot act on is invalid input; extra={extra:?} \
             stderr={stderr}"
        );
        assert!(
            stderr.contains("not implemented in this build"),
            "the refusal has to say the form is unimplemented, or its reader takes it \
             for a configuration defect and goes looking for the table to add; \
             extra={extra:?} stderr={stderr}"
        );
        assert!(
            stderr.contains("cve:CVE-2026-1234"),
            "and it has to name the reference it refused; extra={extra:?} \
             stderr={stderr}"
        );
        assert!(
            stderr.contains("write `cve`"),
            "the one invocation this build does implement is the sweep, and it has to \
             be spelled; extra={extra:?} stderr={stderr}"
        );
        assert!(
            out.stdout.is_empty(),
            "a refused invocation must write nothing to stdout, got {}",
            String::from_utf8_lossy(&out.stdout)
        );
    }

    assert_eq!(
        s.project_tree(),
        before,
        "a refused form must leave the project tree byte for byte as it found it"
    );
    assert!(
        !s.report_dir().exists(),
        "a refused form must not bring `<report.dir>` into existence"
    );
}

fn valued_cve_mentions(text: &str) -> Vec<String> {
    valued_mentions(text, "cve")
}

fn valued_mentions(text: &str, scheme: &str) -> Vec<String> {
    let prefix = format!("{scheme}:");
    text.match_indices(&prefix)
        .filter(|(at, _)| {
            text[at + prefix.len()..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '<'))
        })
        .map(|(at, _)| text[at..].chars().take(32).collect())
        .collect()
}

fn shape_templates(text: &str, schemes: &[&str]) -> Vec<String> {
    let word = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '<' | '>');
    text.match_indices(':')
        .filter(|(at, _)| {
            text[at + 1..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '<'))
        })
        .filter_map(|(at, _)| {
            let head = text[..at].rsplit(|c: char| !word(c)).next()?;
            let scheme = head.trim_matches(|c| matches!(c, '<' | '>'));
            (!scheme.is_empty() && !schemes.contains(&scheme)).then(|| {
                let tail: String = text[at + 1..].chars().take_while(|c| word(*c)).collect();
                format!("{head}:{tail}")
            })
        })
        .collect()
}

#[test]
fn no_operator_facing_surface_promises_the_valued_form() {
    let text = |args: &[&str], expected: Option<i32>, stderr: bool| -> String {
        let out = support::fiddle_command().args(args).output().unwrap();
        if let Some(expected) = expected {
            assert_eq!(
                out.status.code(),
                Some(expected),
                "args={args:?} stdout={} stderr={}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
        String::from_utf8(if stderr { out.stderr } else { out.stdout }).unwrap()
    };
    let help = |args: &[&str]| text(args, Some(0), false);
    let refusal = |reference: &str, command: &str| {
        text(
            &[
                command,
                reference,
                "--config",
                "../../tests/fixtures/fiddle.toml",
            ],
            Some(2),
            true,
        )
    };

    for (surface, rendered) in [
        ("fiddle --help", help(&["--help"])),
        ("fiddle inspect --help", help(&["inspect", "--help"])),
        ("fiddle run --help", help(&["run", "--help"])),
        (
            "the `cve:` diagnostic from inspect",
            refusal("cve:", "inspect"),
        ),
        ("the `cve:` diagnostic from run", refusal("cve:", "run")),
    ] {
        let promised = valued_cve_mentions(&rendered);
        assert!(
            promised.is_empty(),
            "{surface} offers a valued `cve` reference, and no capability in this \
             build acts on one — an operator following it reaches a refusal: \
             {promised:?}"
        );
    }

    for command in ["inspect", "run"] {
        let refused = refusal("cve:CVE-2026-1234", command);
        assert!(
            !valued_cve_mentions(&refused).is_empty(),
            "the surface that refuses the form has to name it, or its reader cannot \
             tell which half of their reference was the problem: {command} {refused}"
        );
        assert!(
            refused.contains("not implemented in this build"),
            "and it has to say why, in the words every other surface is silent in: \
             {command} {refused}"
        );
    }
}

#[test]
fn every_scheme_that_needs_no_value_is_named_on_each_surface_and_in_each_colonless_refusal() {
    let text = |args: &[&str], expected: Option<i32>, stderr: bool| -> String {
        let out = support::fiddle_command().args(args).output().unwrap();
        if let Some(expected) = expected {
            assert_eq!(
                out.status.code(),
                Some(expected),
                "args={args:?} stdout={} stderr={}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
        String::from_utf8(if stderr { out.stderr } else { out.stdout }).unwrap()
    };
    let refusal = |reference: &str, command: &str| {
        text(
            &[
                command,
                reference,
                "--config",
                "../../tests/fixtures/fiddle.toml",
            ],
            Some(2),
            true,
        )
    };
    let flatten = |rendered: &str| -> String {
        rendered
            .replace(['│', '×'], " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };

    const VERDICT: &str = "the line that judges the reference";
    const ADVICE: &str = "the advice beneath that line";
    let parts = |rendered: &str| -> Vec<(&'static str, String)> {
        let Some((_, judged)) = rendered.split_once('×') else {
            return vec![("its text", flatten(rendered))];
        };
        let (verdict, advice) = judged.split_once("help:").unwrap_or((judged, ""));
        [(VERDICT, verdict), (ADVICE, advice)]
            .into_iter()
            .map(|(part, rendered)| (part, flatten(rendered)))
            .filter(|(_, rendered)| !rendered.is_empty())
            .collect()
    };

    let unknown = flatten(&refusal("mystery:x", "inspect"));
    let (_, tail) = unknown.split_once("expected one of ").unwrap_or_else(|| {
        panic!("the unknown-scheme diagnostic has to name the set it knows: {unknown}")
    });
    let listed = tail.split_once(" help:").map_or(tail, |(list, _)| list);
    let schemes = listed
        .split(',')
        .map(str::trim)
        .filter(|scheme| !scheme.is_empty())
        .collect::<Vec<_>>();
    assert!(
        schemes.len() > 1,
        "the set was not parsed off the diagnostic, so nothing below is being \
         checked: listed={listed:?}"
    );
    for scheme in &schemes {
        assert!(
            scheme
                .chars()
                .all(|c| c.is_ascii_lowercase() || matches!(c, '-' | '_')),
            "`{scheme}` is not a scheme spelling, so the list was mis-parsed and this \
             lane is checking the wrong words: listed={listed:?}"
        );
    }

    let (stand_alone, take_a_value): (Vec<&str>, Vec<&str>) = schemes.iter().partition(|scheme| {
        let out = support::fiddle_command()
            .args([
                "inspect",
                scheme,
                "--config",
                "../../tests/fixtures/fiddle.toml",
            ])
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        !(out.status.code() == Some(2) && stderr.contains("fiddle::invocation_ref::malformed"))
    });
    assert!(
        !stand_alone.is_empty() && !take_a_value.is_empty(),
        "with one kind of scheme there is no partition to hold anything to, and \
         every assertion below passes for the wrong reason: stand_alone={stand_alone:?} \
         take_a_value={take_a_value:?}"
    );

    let typo = |scheme: &str| -> String {
        let stem: String = scheme.chars().take(scheme.chars().count() - 1).collect();
        ('a'..='z')
            .map(|letter| format!("{stem}{letter}"))
            .find(|candidate| candidate != scheme && !schemes.contains(&candidate.as_str()))
            .unwrap_or_else(|| panic!("no one-letter typo of `{scheme}` falls outside {schemes:?}"))
    };
    let mut colonless: Vec<String> = stand_alone.iter().map(|scheme| typo(scheme)).collect();
    colonless.push("bogus".to_string());
    let unknown_with_no_value = "notascheme:";

    let root = text(&["--help"], Some(0), false);
    let (_, commands_block) = root.split_once("Commands:").unwrap_or_else(|| {
        panic!("the binary has to list its commands, or the surfaces below are unfound: {root}")
    });
    let listed_commands = commands_block
        .lines()
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .filter_map(|line| line.split_whitespace().next())
        .collect::<Vec<_>>();
    let probe = colonless
        .first()
        .expect("a scheme stands alone, so a typo of it exists");
    let commands = listed_commands
        .iter()
        .copied()
        .filter(|command| {
            let out = support::fiddle_command()
                .args([
                    command,
                    probe.as_str(),
                    "--config",
                    "../../tests/fixtures/fiddle.toml",
                ])
                .output()
                .unwrap();
            out.status.code() == Some(2)
                && String::from_utf8_lossy(&out.stderr)
                    .contains("fiddle::invocation_ref::malformed")
        })
        .collect::<Vec<_>>();
    assert!(
        !commands.is_empty() && commands.len() < listed_commands.len(),
        "the probe has to select some commands and reject some, or it is not \
         selecting on taking a reference and every surface below is the wrong set: \
         listed={listed_commands:?} selected={commands:?}"
    );

    struct Surface {
        what: String,
        rendered: String,
        enumerates: bool,
        colonless: bool,
    }
    let mut surfaces = Vec::new();
    for command in &commands {
        surfaces.push(Surface {
            what: format!("fiddle {command} --help"),
            rendered: text(&[command, "--help"], Some(0), false),
            enumerates: false,
            colonless: false,
        });
        for input in &colonless {
            let rendered = refusal(input, command);
            assert!(
                rendered.contains("fiddle::invocation_ref::malformed"),
                "`{input}` carries no colon and has to reach the malformed arm, or the \
                 colonless check below is holding a different diagnostic: {rendered}"
            );
            surfaces.push(Surface {
                what: format!("the `{input}` diagnostic from {command}"),
                rendered,
                enumerates: true,
                colonless: true,
            });
        }
        surfaces.push(Surface {
            what: format!("the `{unknown_with_no_value}` diagnostic from {command}"),
            rendered: refusal(unknown_with_no_value, command),
            enumerates: true,
            colonless: false,
        });
    }
    assert!(
        surfaces.iter().any(|surface| surface.colonless)
            && surfaces.iter().any(|surface| surface.enumerates)
            && surfaces.iter().any(|surface| !surface.enumerates),
        "each check below applies to one kind of surface, and a kind with no member \
         is a check that ran on nothing"
    );

    assert_eq!(
        shape_templates(
            "invocation reference must be <scheme>:<value>, got `x`",
            &schemes
        ),
        vec!["<scheme>:<value>"],
        "the detector no longer sees a shape template, so that half of the check \
         below holds nothing: schemes={schemes:?}"
    );
    assert!(
        shape_templates(
            "take a value, as in `beans:fiddle-m0-demo`: beans, cve",
            &schemes
        )
        .is_empty(),
        "the detector reads one scheme's own example as a claim about every scheme, \
         so the check below reds on correct text: schemes={schemes:?}"
    );

    for surface in &surfaces {
        let advice = flatten(&surface.rendered);
        for scheme in &stand_alone {
            assert!(
                advice.contains(*scheme),
                "{} says how a reference is written and never mentions `{scheme}`, \
                 which is a complete reference on its own — so a caller who meant it \
                 is shown only shapes that require a value and reads their own \
                 invocation as illegal: {advice}",
                surface.what
            );
            let valued = valued_mentions(&advice, scheme);
            assert!(
                valued.is_empty(),
                "{} shows `{scheme}` carrying a value, and the bare form is what this \
                 build acts on: {valued:?}",
                surface.what
            );
        }

        for (part, read) in parts(&surface.rendered) {
            let templates = shape_templates(&read, &schemes);
            let owed = if surface.colonless {
                Some("answers an input with no colon in it".to_string())
            } else if templates.is_empty() {
                None
            } else {
                Some(format!(
                    "gives {templates:?} as the shape a reference takes"
                ))
            };
            let Some(because) = owed else {
                continue;
            };
            for scheme in &stand_alone {
                assert!(
                    read.contains(*scheme),
                    "{}: {part} {because} and never names `{scheme}`, which is a \
                     complete reference on its own — so this part, read as an operator \
                     reads it, denies the bare form. Whatever is written elsewhere in \
                     the same output does not travel with it: {read}",
                    surface.what
                );
            }
        }
    }

    for surface in surfaces.iter().filter(|surface| surface.enumerates) {
        let (_, advice) = parts(&surface.rendered)
            .into_iter()
            .find(|(part, _)| *part == ADVICE)
            .unwrap_or_else(|| {
                panic!(
                    "{} offers the set of schemes and has no advice to offer it in: {}",
                    surface.what, surface.rendered
                )
            });
        let Some((valued_half, bare_half)) =
            advice.split_once("discover their own work need none:")
        else {
            panic!(
                "{} offers the set of schemes and has to separate the ones that \
                 require a value from the ones that do not, or it is offering one shape \
                 to a set that has two: {advice}",
                surface.what
            );
        };
        for scheme in &stand_alone {
            assert!(
                bare_half.contains(*scheme),
                "{} must offer `{scheme}` where no value is needed, because that is \
                 the invocation the binary accepts: {advice}",
                surface.what
            );
            assert!(
                !valued_half.contains(*scheme),
                "{} places `{scheme}` among the schemes that take a value, and the \
                 binary accepts it alone — advice that sends an operator who wanted a \
                 sweep to a read of one named item: {advice}",
                surface.what
            );
        }
        for scheme in &take_a_value {
            assert!(
                valued_half.contains(*scheme),
                "{} must offer `{scheme}` where a value is required, because the \
                 binary refuses it alone: {advice}",
                surface.what
            );
            assert!(
                !bare_half.contains(*scheme),
                "{} lists `{scheme}` among the schemes that need no value, and the \
                 binary refuses `{scheme}` written alone — advice refused when \
                 followed: {advice}",
                surface.what
            );
        }
    }
}
