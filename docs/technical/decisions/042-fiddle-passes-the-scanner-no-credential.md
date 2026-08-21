# 042 — Fiddle passes the scanner no credential

Status: accepted
Cites: scanner/wizcli.rs, INHERITED, Wizcli::new, ScanError::Failed, Recurrence::Correctable, RunOutcome::Retryable, deny_unknown_fields, CredentialPurpose, wiz_stub.rs::a_named_config_dir_holds_no_login, Rescan

## Context

Run 32499343373 was the first run to reach the scanner. It got no report.

`Wizcli::authenticate` created an empty directory under the scan's scratch. It exported `WIZ_CLIENT_ID`, `WIZ_CLIENT_SECRET` and `WIZ_CONFIG_DIR`, and it returned. The real tool needs a login. The host's `setup-wiz` action runs `wizcli auth --id --secret`, which writes credentials into wizcli's default directory. fiddle pointed wizcli at its own empty directory, so that login was out of scope. wizcli printed its usage banner, exited 1, and wrote no report.

No test measured the claim. The scripted stub wrote a report whatever the environment held.

## Decision

The step is deleted. fiddle sets none of the three variables, creates no configuration directory, and holds no scanner credential. The adapter builds a command and runs it.

`INHERITED` names the two variables the child receives from fiddle's own environment: `HOME` and `WIZ_CONFIG_DIR`. `command` still calls `env_clear`, so this is a pass-through of what the caller set and not a value fiddle builds. Without it wizcli reads no login, because `env_clear` hides the caller's directory as completely as the old override did.

`[scanner] client_id` and `[scanner] client_secret` are removed from the schema. `Wizcli::new` takes no credential. `Rescan` carries none. `CredentialPurpose` names the model and the forge, and no scanner.

fiddle adds no check that the login is present. An unauthenticated wizcli exits non-zero and writes no report, and `ScanError::Failed` already reports that as `Recurrence::Correctable`, which reaches `RunOutcome::Retryable` and exit 11. An external tool that cannot run reports itself.

## Consequences

- **This is a breaking configuration change.** `Config` uses `deny_unknown_fields`, so a document that still names `client_id` or `client_secret` under `[scanner]` fails to load and the run exits 2. Every deployment document must drop both keys.
- `Wizcli::redact` is deleted. It replaced the client secret in scanner stderr, and fiddle no longer reads a client secret, so there was nothing left for it to replace.
- What that gives up: wizcli holds the caller's secret from the login, and it can quote that secret in stderr, which fiddle publishes. fiddle can no longer mask a value it never receives. The remedy belongs to whatever prints the secret, not to fiddle.
- `Wizcli::scan` creates its own scratch directory. `authenticate` created it as a side effect of one `create_dir_all`, and the report write failed without it.
- The scanner's configuration leaves the project tree. `authenticate` wrote it under `[report] dir`, which `land()` can commit. `fiddle-xjjp` asks for a leak assertion over the same credential; fiddle now has no such credential to leak, and the forge half of that bean stands.
- `wiz_stub.rs::a_named_config_dir_holds_no_login` is what makes the deletion falsifiable. A `WIZ_CONFIG_DIR` that names a directory holding no login means a banner on stdout, exit 1, and no report, which is what the failing run observed. Any change that points the scanner at a fresh directory fails every scripted scan.
- The stub keys on `WIZ_CONFIG_DIR` alone. It does not model an absent `HOME/.wiz`, because the suite would then depend on the machine that runs it. The observed failure was a named directory that held nothing, and that is the case the stub reproduces.
- What was given up: a fiddle run depends on ambient state a caller established. Every other credential in this product is named in `fiddle.toml` and read from a variable. The alternative was fiddle running `wizcli auth` itself, which puts the secret on a command line the process table can read.
