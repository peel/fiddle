# 044 — A deployment declares the programs an attempt may run, and that declaration is not a sandbox

Status: accepted; amended in M4b by [047](047-the-brief-names-the-declarations-the-model-could-have-written-itself.md), which leaves the decision standing.
Cites: fiddle_runtime::workspace::declared::resolve, DeclaredCommand, Extend, Undeclared, RunCommand, ToolHost, WorkspacePath, Isolation, a_workspace_command_inherits_no_credential, `[[workspace.commands]]`, a_derived_file_a_declared_command_wrote_reaches_clean_when_the_attempt_declares_it, a_model_asking_for_a_shell_is_refused_because_no_deployment_declared_one

## Context

The first real repair failed for want of an instrument. The attempt chose correctly — `github.com/golang-jwt/jwt/v4` from 4.5.0 to 4.5.2 for CVE-2025-30204 — and then had to produce a valid `go.sum`. Its only writing tool was `write_file`, so it transcribed checksums by hand and corrupted the file. Its own note named the remedy: regenerate with `go mod tidy`.

The tool surface was `read_file`, `write_file`, `list_files` and `run_check`. `run_check` takes `NoArgs` and runs the configured check. Nothing could run a program an ecosystem needs, and a dependency repair in any ecosystem is not a text edit.

The step this capability replaced carried `--allowedTools "Bash(go *)"`. The manual claimed fiddle needs no such entry.

## Decision

A deployment declares the programs an attempt may run, under `[[workspace.commands]]`, in the shape `[[workspace.checks]]` already uses. A fifth tool, `run_command`, runs one of them and returns what it printed.

**The declaration is a prefix. The model names the program and the whole argument list.** `resolve` accepts the call when a declaration names that program and its arguments are a prefix of what the model wrote. The longest matching prefix decides which declaration applies.

**What the model may vary is the deployment's choice, one declaration at a time.** `extend = "none"` is the default: the declaration is the whole command and an appended argument is refused. `extend = "arguments"` permits an append, bounded by a stated rule — at most eight arguments, each at most 256 bytes, each one line of printable text, none beginning with `/` and none carrying a `..` segment. Fixed arguments cannot express `go mod edit -require=pkg@v1.2.3`, and free arguments are a larger surface for no gain, so a prefix the model may append to is what the tool takes.

**There is no interpreter.** A program and an argument list reach `tokio::process::Command`. Nothing is expanded and one argument cannot become two. `sh` is refused because no declaration names it, which is the same reason `curl` is refused.

**fiddle names no ecosystem.** No program name appears in Rust, in a default, in a tool schema or in a preamble. A deployment that repairs Go declares Go.

**A declared command is a workspace command.** It runs through `Workspace::run`, which is the one seam every child process in the worktree passes through. It therefore inherits the worktree as its working directory, the scratch `HOME` beside it, `env_clear` plus the four-name allowlist, `run_bounded`, cancellation, and the path relativisation ADR 034 requires. `[workspace] command_timeout` bounds it, reduced by `[agent] tool_timeout` the way the check already is.

## This is not a security boundary, and it must not be read as one

The declaration list is an allowlist of program names. It is not isolation, and the earlier draft of this record implied that it was.

**Declaring one build tool grants arbitrary code execution as the invoking user.** `go test` compiles and runs code out of the repository under repair. `go generate` runs whatever the source names. `make` runs a file in the tree. A declared prefix restricts the first few arguments of the first process; it says nothing about what that process then executes, and no argument rule can take that back. Against a hostile or compromised repository the allowlist buys close to nothing.

**Where that bites, and where it does not.** On a GitHub runner the job already runs in a disposable virtual machine, so the deployment supplies the isolation and fiddle's lack of it costs nothing. The exposure is local attended execution — the PRD's M6, where the same binary runs against a developer's own machine — and it is the same exposure the check command has had since M1. This record does not add it. It adds a second door to it, and one the model chooses to walk through.

## The boundary that holds today

A declared program runs on the host. These four things hold.

1. **The working directory is a git worktree fiddle made for one attempt**, under `[workspace] root`, and a `Drop` guard removes it on every path. `HOME` points at a scratch directory beside that worktree, never inside it.
2. **The child receives no credential.** `Workspace::run` calls `env_clear` and passes four names: `HOME`, `PATH`, `LANG`, and `RUSTUP_HOME` where the parent has one. The model credential, the forge token and the scanner's login reach no declared program. `a_workspace_command_inherits_no_credential` pins that set exactly, and a fifth name cannot be added without changing it.
3. **fiddle's own file tools cannot address anything outside the worktree.** `WorkspacePath` refuses an escaping path and refuses `.git` at any depth, which bounds `read_file` and `write_file`. The appended-argument rule refuses an absolute path and a `..` segment for the same reason.
4. **Execution is bounded in time and is cancellable.** `command_timeout`, reduced by `tool_timeout`, kills the process group. The cancellation token reaches the child.

**Nothing else holds.** A declared program is an ordinary process carrying the invoking user's identity and permissions. It can read any file that user can read, write any file that user can write, and open any socket. `PATH` is the real one. Item 3 bounds fiddle's tools and not the program's syscalls, so the argument rule stops the model from *naming* a path outside the project and stops nothing from *reaching* one. A reader must not infer confinement from the fact that the program was declared.

## What the allowlist is for, since it is not for that

- **It keeps fiddle ecosystem-agnostic.** The alternative that actually ran a program — a shell string, or a program name in Rust — would put an ecosystem back on the deterministic side of ADR 025.
- **It makes the surface legible.** `fiddle config check` prints every program an attempt may run and which of them it may add to, so an operator reads the whole of it in one place rather than inferring it from a prompt.
- **It stops casual misuse and a wasted turn.** A model that guesses at a program gets a refusal naming what is available, rather than a side effect. That is a correctness property, not a safety one.
- **It is the deployment's written record of what a repair needs.** A repository that cannot be repaired without a lockfile generator says so in its own document.

## The network line

Before this change the attempt had no network at all. Its four tools were local, and the only child that could reach a socket was the deployment's own check program.

`go mod tidy` fetches from a module proxy. So the first deployment that declares a dependency tool gives the attempt an egress path it directs itself, and that is a line which is never afterwards uncrossed: a sandbox variant cannot answer "no network" without breaking the repair this record exists to enable. The honest question a later bean inherits is which hosts, named by whom, and through what proxy — which is another declaration and another decision.

## What a sandbox variant would need, without building it

`Isolation` is an enum with one variant, `GitWorktree`, destructured exhaustively by three `let` bindings in `main.rs`. The seam exists and is empty. A second variant would need, at minimum:

- **Three `let` bindings become `match` arms**, and `render`'s `isolation()` gains a name. That part is trivial and is not the work.
- **A decision about what the worktree is.** `.git` inside the worktree is a *file* holding a `gitdir:` pointer into the fixture repository, which is a sibling on the host. `changed_files()`, `edits()` and `InWorktree` all run `git` in the worktree and follow that pointer. A container that mounts the worktree alone has no change set and no commit. So the fixture must be mounted too, or the worktree materialised without a gitdir link — and the second is a different `Workspace::create`, not a mount option.
- **One boundary, not two.** `[[workspace.checks]]`, `run_command` and the worktree's own `git` all go through `Workspace::run`. Whatever wraps that function moves all three inside together, which is why the seam is worth keeping single.
- **A statement about the network**, per the section above. Deny-all breaks dependency repair; allow-all makes the sandbox a filesystem measure only.
- **The credential-carrying adapters stay outside.** The scanner, the forge and the model gateway hold secrets and must not sit inside a sandbox with repository code. They already run outside `Workspace::run`, so that separation is structural rather than something to add.

## Consequences

- **The manual's claim that fiddle needs no tool allowlist is false, and is corrected in the same change.** fiddle now has one. It is narrower in surface than `Bash(go *)` and it is not narrower in what a declared program can do. `SYSTEM.md` says five tools rather than four and carries the boundary as a known limitation rather than as an invariant; the PRD's configuration requirement that put bounded tools in Rust names its second exception; `host-workflow-m4b.patch` says what replaced `Bash(go *)`, and says that on a runner the disposable job is what isolates.
- **The declared set is not model-visible, and the refusal is what teaches.** The tool schema names no program and the preamble names none, so ADR 034's rule stands unweakened: `no_tool_advertises_a_host_fact` covers the fifth tool with no change to what it forbids. An undeclared program is refused with a message naming what it asked for and every declaration the project carries, so a wrong guess costs one turn and never repeats. What was given up: the first call may be that wrong guess. The alternative — a schema enumerating the programs — would have put an ecosystem's names on the menu and made a declared program indistinguishable from a host fact.

  > The preamble names the declarations the model could have written itself. A run that never guessed never learned the tool was usable; the schema still names no program. [047](047-the-brief-names-the-declarations-the-model-could-have-written-itself.md)
- **The attempt still declares the files a command wrote.** ADR 026 compares the diff against the report's `changed_files`, and a file `go mod tidy` wrote is in that diff. Knowing that a bump implies a checksum file is ecosystem knowledge, so it is the agent's to declare. The exclusion ADR 026 names is not widened: it still covers the paths the run edited before the attempt began and nothing beside them. `a_derived_file_a_declared_command_wrote_is_a_breach_when_the_attempt_omits_it` is the falsifying case.
- **A deployment that declares nothing is offered no fifth tool.** `attempt_briefed` registers `run_command` only where the host carries a declaration, and appends the sentence describing it to the preamble on the same condition. A model told about a tool that refuses everything would spend turns on it.
- **A declaration that repeats itself is refused at load.** Two entries spelling the same program and the same arguments answer "what may this vary" twice, and `deny_unknown_fields` cannot see that. The refusal names the declaration. Two entries sharing a program and differing in arguments are two declarations and both stand.
- **A declaration's own arguments are not bounded by the append rule.** The deployment is trusted with an absolute path where the model is not, which is what lets a deployment point a declared program at a file outside the project. That is a deliberate asymmetry and it is also a reminder that the rule is about the model's words rather than about the program's reach.
- **No test in this change asserts isolation, because none could.** The tests assert refusal, the declaration rule, the absence of a credential from the child, and the time bound. A test named for containment would be the defect this record exists to avoid.
