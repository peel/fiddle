# 092 — A relative path in the configuration is resolved when it is read

Status: accepted
Cites: crates/fiddle-cli/src/config.rs, absolutized, absolute, ConfigError, Unresolvable, crates/fiddle-acceptance/tests/toil.rs, a_relative_workspace_root_lists_the_change_an_absolute_one_lists, names_its_workspace_root_relative_to_its_directory, run_toil_from_its_directory, an_m0_shaped_document_produces_exactly_its_payload_with_its_paths_resolved, config_check_reports_the_workspace_table_it_accepted

## Context

`fiddle-pgtk`. OBSERVED in run 32508803188: a relative `[workspace] root` made every change listing fail. `Workspace::new` wrote the baseline ignore file under the root, resolved against fiddle's working directory, and `git ls-files --exclude-from` read the same relative name from inside the worktree it had just created. The default root, `.fiddle/workspaces`, is relative, so a document that names no root met it too.

No lane ran with a relative root. Every acceptance document names a path under a `TempDir`, which is absolute.

## Decision

**`load` resolves every relative path the configuration names against the working directory, once, before anything uses it.**

- `absolutized` resolves `[stub] root`, `[report] dir`, `[workspace] root` and `fixture`, and `[github] config_dir` and `work`. A relative path stays legal, because a person writing a document by hand writes one, and it means what they expect: relative to where fiddle runs.
- `[github] git` is a program name that the search path resolves, not a path, and is left as written.
- When the working directory cannot be read, `load` refuses with `Unresolvable`, naming the path, and exits as a configuration error. It does not guess.

`fiddle config check` reports the resolved paths, because they are what the run will use. Three rows that expected the paths as written now expect them resolved, and `an_m0_shaped_document_produces_exactly_its_payload_with_its_paths_resolved` replaces the row that said an M0 document reports what it always did.

Refusing a relative root was the other choice. It costs the natural form of a document, and the paths would still be resolved inconsistently for any author who meets the refusal late.

## The rows

`a_relative_workspace_root_lists_the_change_an_absolute_one_lists` runs one document with `root = "workspaces"` and one with the absolute path, both from the scenario's directory. Both exit 0 and publish the agent's change. Without `absolutized`, the relative one fails with exit 2 at the first path it uses.
