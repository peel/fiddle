# 031 — Containment is checked syntactically, then against the resolved path

Status: accepted

Cites: fiddle_runtime::workspace::WorkspacePath::parse, Workspace::resolve,
workspace/path.rs, workspace/mod.rs

## Context

A model asks to read and write paths inside a workspace by name. A syntactic check
alone cannot see where a symlink points; a canonicalizing check alone races the
filesystem it is asking about. A checkout also holds things that are not the
project: the repository's own metadata, and whatever the project's committed rules
exclude.

## Decision

Validate syntactically first, then canonicalize and re-check. Walk the requested
path one component at a time to the deepest **existing** ancestor, and refuse any
component that resolves outside, leaf or not. Refuse `.git` at any depth, case
insensitively, for reading and writing alike.

## Consequences

**A model can create a file in a directory the project does not have yet.** Below
the first absent component there is nothing to follow, so the remaining names are
joined onto a path already proven inside. The alternative reached the model as
"writing the file did not succeed".

**A directory is made by the workspace, never by a model working around a
refusal.** After `create_dir_all` the parent is canonicalized **again** and the
leaf rebuilt on the proven path, so a directory the check would refuse is refused
before anything is written through it.

**A dangling symlink is refused explicitly**, rather than falling through
canonicalization's error path as an I/O failure.

**`.git` is refused rather than read.** In a linked worktree it is a *file* whose
contents are an absolute host path, which is exactly the host fact ADR 034 keeps
off the model-visible surface.

**What the project's committed rules exclude is refused by `Workspace::read`**, not
by a denylist. A build tree's dependency files carry absolute host paths and have
no name worth enumerating.

**What was given up: the walk costs one syscall per component.** A single
canonicalize would be cheaper and would answer a different question.
