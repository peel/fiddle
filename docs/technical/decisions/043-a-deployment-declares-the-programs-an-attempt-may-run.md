# 043 — A deployment declares the programs an attempt may run, and the model names one of them

Status: accepted
Cites: fiddle_runtime::workspace::declared::resolve, DeclaredCommand, Extend, Undeclared, RunCommand, ToolHost, `[[workspace.commands]]`, a_derived_file_a_declared_command_wrote_reaches_clean_when_the_attempt_declares_it, a_model_asking_for_a_shell_is_refused_because_no_deployment_declared_one

## Context

The first real repair failed for want of an instrument. The attempt chose correctly — `github.com/golang-jwt/jwt/v4` from 4.5.0 to 4.5.2 for CVE-2025-30204 — and then had to produce a valid `go.sum`. Its only writing tool was `write_file`, so it transcribed checksums by hand and corrupted the file. Its own note named the remedy: regenerate with `go mod tidy`.

The tool surface was `read_file`, `write_file`, `list_files` and `run_check`. `run_check` takes `NoArgs` and runs the configured check. Nothing could run a program an ecosystem needs, and a dependency repair in any ecosystem is not a text edit.

The step this capability replaced carried `--allowedTools "Bash(go *)"`. The manual claimed fiddle needs no such entry.

## Decision

A deployment declares the programs an attempt may run, under `[[workspace.commands]]`, in the shape `[[workspace.checks]]` already uses. A fifth tool, `run_command`, runs one of them and returns what it printed.

**The declaration is a prefix. The model names the program and the whole argument list.** `resolve` accepts the call when a declaration names that program and its arguments are a prefix of what the model wrote. The longest matching prefix decides which declaration applies.

**What the model may vary is the deployment's choice, one declaration at a time.** `extend = "none"` is the default: the declaration is the whole command and an appended argument is refused. `extend = "arguments"` permits an append, bounded by a stated rule — at most eight arguments, each at most 256 bytes, each one line of printable text, none beginning with `/` and none carrying a `..` segment. Fixed arguments cannot express `go mod edit -require=pkg@v1.2.3`; free arguments approach a shell; a prefix the model may append to is the middle, and the rule above is what makes it narrower than a shell.

**There is no shell.** A program and an argument list reach `tokio::process::Command`. Nothing is expanded and one argument cannot become two. `sh` is refused for the one reason that holds in every ecosystem: no deployment declared it.

**fiddle names no ecosystem.** No program name appears in Rust, in a default, in a tool schema or in a preamble. A deployment that repairs Go declares Go.

**A declared command is a workspace command.** It runs through `Workspace::run`, so it inherits the worktree as its working directory, the scratch `HOME` beside it, `env_clear` plus the four-name allowlist, `run_bounded`, cancellation, and the path relativisation ADR 034 requires. `[workspace] command_timeout` bounds it, reduced by `[agent] tool_timeout` the way the check already is.

## Consequences

- **The manual's claim that fiddle needs no tool allowlist is false, and is corrected in the same change.** fiddle now has one. It is narrower than `Bash(go *)` — declared programs, a fixed prefix, no shell, no credential, one bound — and it is an allowlist. `SYSTEM.md` says five tools rather than four, the configuration requirement that put bounded tools in Rust names its second exception, and `host-workflow-m4b.patch` says what replaced `Bash(go *)` beside what replaced `Bash(docker build *)`.
- **The declared set is not model-visible, and the refusal is what teaches.** The tool schema names no program and the preamble names none, so ADR 034's rule stands unweakened: `no_tool_advertises_a_host_fact` covers the fifth tool with no change to what it forbids. An undeclared program is refused with a message naming what it asked for and every declaration the project carries, so a wrong guess costs one turn and never repeats. What was given up: the first call may be that wrong guess. The alternative — a schema enumerating the programs — would have put an ecosystem's names on the menu and made a declared program indistinguishable from a host fact.
- **The attempt still declares the files a command wrote.** ADR 026 compares the diff against the report's `changed_files`, and a file `go mod tidy` wrote is in that diff. Knowing that a bump implies a checksum file is ecosystem knowledge, so it is the agent's to declare. The exclusion ADR 026 names is not widened: it still covers the paths the run edited before the attempt began and nothing beside them. `a_derived_file_a_declared_command_wrote_is_a_breach_when_the_attempt_omits_it` is the falsifying case.
- **A deployment that declares nothing is offered no fifth tool.** `attempt_briefed` registers `run_command` only where the host carries a declaration, and appends the sentence describing it to the preamble on the same condition. A model told about a tool that refuses everything would spend turns on it.
- **A declaration that repeats itself is refused at load.** Two entries spelling the same program and the same arguments answer "what may this vary" twice, and `deny_unknown_fields` cannot see that. The refusal names the declaration. Two entries sharing a program and differing in arguments are two declarations and both stand.
- **A declaration's own arguments are not bounded by the append rule.** The deployment is trusted with an absolute path where the model is not, which is what lets a deployment point a declared program at a file outside the project.
- **What this does not bound.** Network egress is unchanged: a declared program runs with a real `PATH` and nothing stops it opening a socket, exactly as the check already could. `docs/BACKLOG.md` records that gap for the check, and it now has a second door. A deployment that declares a program which itself takes a script is declaring a shell by another name, and no rule in Rust can see that.
