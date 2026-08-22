# 029 — A locator may be inherited, an authority may not

Status: accepted
Cites: fiddle_runtime::process::run_bounded, workspace/command.rs, github/cli.rs, git/publish.rs, workspace::a_workspace_command_inherits_no_credential, github_cli::the_gh_environment_is_exactly_five_names_and_no_home, git_publish::the_push_environment_is_exactly_seven_names_and_no_home

## Context

Fiddle spawns a child process from three places: a workspace check, the `gh` adapter, and the network git. ADR 046 gives the fetch the same set as the push. Each needs a different environment, and an inherited environment leaks whatever the operator's shell holds. Four documents once stated the workspace set four different ways, each true of the fragment its author was arguing about.

## Decision

Clear the environment at every spawn site and pass a named set. Inherit a name that says **where** a tool is. Never inherit a name that says **what may be done** with it.

## Consequences

- The three sets are never reconciled. A workspace check must see no credential, and a credentialed site must see exactly one.
- `HOME`'s absence is what makes the credential claim true. With `HOME` gone and `GH_CONFIG_DIR` empty, `gh` answers "please run: gh auth login" instead of reading the operator's keyring.
- The emptied `credential.helper` is load-bearing. Dropping `HOME` does not close a helper set in system git configuration. A push would then authenticate from the keychain.
- A credential travels through the environment and never `argv`, because `/proc/<pid>/cmdline` is world-readable on Linux.
- What was given up: a fifth name is now a test edit. Each set is asserted exactly, so an author with a good reason pays one extra file.

## The three sets

Each set is stated once, in the test that pins it.

| site | names | `HOME` |
| --- | --- | --- |
| workspace command | `HOME`, `LANG`, `PATH`, `RUSTUP_HOME` | a scratch directory beside the worktree |
| `gh` | `PATH`, `GH_TOKEN`, `GH_CONFIG_DIR`, `GH_PROMPT_DISABLED`, `NO_COLOR` | absent |
| `git push` and `git fetch` | `PATH`, `GIT_TERMINAL_PROMPT`, and five names carrying one header and an emptied `credential.helper` | absent |

What all three share is the bound. The process group, the deadline and the cancellation are written once, in `run_bounded`.

Adding `HOME` back to a credentialed site, even pointed at scratch, reopens `~/.config/gh`. A green test would then prove only that some credential existed, not that this one did.

## No trust-store locator is inherited, and that may have to change

A cleared environment completed a TLS handshake against `github.com` on this toolchain. If a live lane ever fails certificate verification, the fix is inheriting a CA *path*. That is a locator under this rule, and it is not a wider set.
