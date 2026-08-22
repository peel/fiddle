# 046 — One credential reaches the remote, and the local runner cannot

Status: accepted
Cites: git/publish.rs, capability/cve.rs, GitCli::offer_credential, cve::local_only, git_publish::the_fetch_offers_the_credential_the_push_offers, git_credential_path::the_local_runner_refuses_every_subcommand_that_reaches_a_remote

## Context

fiddle reached the network as git in two ways, and only one carried a credential. `GitCli::publish` injected an `http.https://github.com/.extraHeader` through `GIT_CONFIG_*`. The fetch in `capability/cve.rs` went through `Git::run`, and neither `InRepository` nor `InWorktree` held a token.

The fetch worked because `actions/checkout` persists a credential by default. That is a side effect of the caller. fiddle never stated it as a contract.

No host configuration satisfied both paths.

| checkout | fetch | push |
| --- | --- | --- |
| `persist-credentials: true`, the default | works, on the caller's persisted header | fails: `remote: Duplicate header: "Authorization"` |
| `persist-credentials: false` | fails: `could not read Username for 'https://github.com'` | works |

Run 32533261303 is the first row. Run 32578049819 is the second row. Both ran in `snowplow-incubator/snowplow-identities`.

## Decision

`GitCli` owns every git operation that reaches a remote. `GitCli::offer_credential` writes the five environment names, and the fetch and the push both call it. `InRepository` and `InWorktree` hold a reference to `GitCli` and delegate the fetch to it. They never hold the token.

The local runners refuse a subcommand that reaches a remote. `local_only` reads the subcommand and returns an error for `clone`, `fetch`, `ls-remote`, `pull` and `push`.

## Consequences

- fiddle now states the contract. A deployment sets `persist-credentials: false`, and fiddle authenticates the fetch and the push itself.
- The two sets are kept apart twice. `Git::fetch` is the one network method on the trait, and `Git::run` refuses the five subcommands that reach a remote.
- `grep '"fetch"\|"push"' crates/*/src` finds two call sites. Both are in `git/publish.rs`. That is the whole network surface of the runtime.
- The fetch redacts its own failures, because `GitCli::redact` is the only code that knows the token. Before this record the fetch carried nothing to leak.
- The fetch validates its branch name with the guard the push uses. A name that could change the command is refused before git is spawned.
- The local runner in `dedup::Local` clears no environment, so it inherits the operator's names. The guard, and not the environment, is what keeps that runner local. ADR 029 states three cleared sets, and this is a fourth site that clears nothing.
- What was given up: the fetch no longer runs under `Workspace::run`. It takes the `GitCli` timeout instead of the workspace tool timeout, and it no longer relativises paths in its stderr. A fetch is a network operation, so the network budget is the right one.

## The rejected option

`InWorktree` runs its git through `Workspace::run`. That function clears the environment and passes four names. To inject `GIT_CONFIG_*` there, `WorkspaceCommand` would need an environment field.

`WorkspaceCommand` is also the struct that carries a model's declared command to `agent/tools.rs`. A credential field on it would sit one field away from every command a model asks for. The network operation moved to the adapter for that reason, and not because the change was smaller.

## What the tests prove, and what they do not

The acceptance harness pushes to a private bare repository on the filesystem. A path remote needs no credential, so neither row of the table above exists in any offline world. The suite cannot express a remote that refuses an unauthenticated read. This is the eighth failure in M4b whose cause the stubs cannot state.

`the_fetch_offers_the_credential_the_push_offers` compares the two environments by name and by value, and it decodes the fetch's header. Names alone are not enough: a header keyed to another host carries the same five names and reaches nothing. `the_repository_adapter_offers_the_credential_when_it_fetches` reads the header out of a recording git. `the_local_runner_refuses_every_subcommand_that_reaches_a_remote` shows that no local call reaches the credentialed adapter.

The three tests prove that the credential is offered. They do not prove that authentication succeeds. Only a live run against `github.com` proves that.
