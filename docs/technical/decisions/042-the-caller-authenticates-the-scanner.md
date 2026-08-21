# 042 — The caller authenticates the scanner

Status: accepted
Cites: scanner/wizcli.rs::require_login, scanner/wizcli.rs::holds_a_login, WizLogin::ConfigDir, WizLogin::Home, DEFAULT_CONFIG_DIR, ScanError::Unauthenticated, Recurrence::Permanent, RunOutcome::Failed, Wizcli::redact, WizCredential, scanner.client_id, wiz_stub.rs::usage

## Context

Run 32499343373 was the first run to reach the scanner. It got no report.

`Wizcli::authenticate` created an empty directory under the scan's scratch. It exported `WIZ_CLIENT_ID`, `WIZ_CLIENT_SECRET` and `WIZ_CONFIG_DIR`, and it returned. The real tool needs a login. The host's `setup-wiz` action runs `wizcli auth --id --secret`, which writes credentials into wizcli's default directory. fiddle pointed wizcli at its own empty directory, so that login was out of scope. wizcli printed its usage banner, exited 1, and wrote no report. Two exported variables are not a login.

No test measured the claim. The scripted stub wrote a report whatever the environment held.

## Decision

fiddle does not log in. It reads the login the caller left.

`WizLogin` names where that login is. `WizLogin::ConfigDir` carries the directory the caller named in `WIZ_CONFIG_DIR`. `WizLogin::Home` carries `HOME`, and the login is `HOME/.wiz`, which `DEFAULT_CONFIG_DIR` names. The scanner receives exactly one of the two variables, so wizcli reads the directory fiddle read.

`require_login` runs before the scanner starts. It refuses with `ScanError::Unauthenticated` when that directory holds no entry. The message names the directory, names `wizcli auth --id --secret`, and names the caller as the actor.

`Unauthenticated` is `Recurrence::Permanent`. A retry starts the same scanner against the same absent login.

The alternative was fiddle owning the login. That puts the secret on a command line fiddle builds, where the process table can read it.

## Consequences

- This cause moves from exit 11 to exit 20. `Recurrence::Correctable` reaches `RunOutcome::Retryable`, and `Recurrence::Permanent` reaches `RunOutcome::Failed`. The run that found the defect exited 11, and no retry writes a login.
- fiddle sends the credential to no scanner. The child environment is `PATH`, `NO_COLOR`, and one of `HOME` or `WIZ_CONFIG_DIR`.
- `Wizcli::redact` stays load-bearing. wizcli holds the caller's secret from the login, and it can quote that secret in stderr, and fiddle publishes that stderr. The guard now covers a secret fiddle never sent.
- `WizCredential` keeps `client_secret` and drops `client_id`. `[scanner] client_secret` stays required, because `redact` reads it. `[scanner] client_id` becomes optional, and nothing reads it. `Config` denies unknown fields, so removing the field would stop every document that names it from loading.
- `holds_a_login` counts entries. It does not read a named file, because this repository has not measured which file `wizcli auth` writes. A directory that holds unrelated state passes the check, and the scan's own exit then reports the failure.
- `DEFAULT_CONFIG_DIR` is `.wiz`, and that is fiddle's assumption about wizcli's default directory. A wizcli that keeps its login elsewhere makes fiddle refuse a caller who did log in. The refusal names the directory fiddle read, so a reader sees which directory was wrong.
- The scanner's configuration leaves the project tree. `Wizcli::authenticate` wrote it under `[report] dir`, which `land()` can commit. Nothing fiddle writes now holds a wiz credential.
- `Wizcli::scan` creates its own scratch directory. `authenticate` created it as a side effect of one `create_dir_all`.
- The stub refuses the same way. `wiz_stub.rs::usage` prints the banner on stdout and exits 1 when the login is absent, so a fiddle that skipped the check would report `the scanner produced no report (exit 1)` again, and the test for the refusal would redden.
- What was given up: a fiddle run now depends on ambient state a caller established. Every other credential in this product is named in `fiddle.toml` and read from a variable.
