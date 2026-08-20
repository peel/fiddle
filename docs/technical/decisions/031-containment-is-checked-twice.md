# 031 — Containment is checked syntactically, then against the resolved path

Status: accepted
Cites: fiddle_runtime::workspace::WorkspacePath::parse, Workspace::resolve, Workspace::prepared, Workspace::read, WorkspaceError::Escape, WorkspaceError::NotProject, workspace/path.rs, workspace/mod.rs

## Context

A model asks to read and write paths inside a workspace by name. A syntactic check cannot see where a symlink points, and a canonicalizing one races the filesystem. A checkout also holds things that are not the project: its own metadata, and whatever the committed rules exclude.


## Decision

Validate syntactically first, then canonicalize and re-check. Walk the requested path one component at a time, and refuse any component that resolves outside. Refuse `.git` at any depth, case insensitively, for reading and writing alike.


## Consequences

- A model can create a file in a directory the project does not have yet. Below the first absent component there is nothing to follow. The remaining names join onto a proven path.
- A directory is made by the workspace, never by a model working around a refusal. `prepared` canonicalizes the parent again after `create_dir_all` and rebuilds the leaf on the proven path.
- A dangling symlink is refused explicitly, rather than falling through canonicalization's error path as an I/O failure.
- `.git` is refused rather than read. In a linked worktree it is a file holding an absolute host path. ADR 034 keeps that off the model-visible surface.
- What was given up: the walk costs one syscall per component. A single canonicalize would be cheaper and would answer a different question.

## The refusals name which containment failed

`resolve` distinguishes a leaf that resolves outside from a parent that does. The two carry different reasons, because an author debugging one is misled by the other.

The alternative to the ancestor walk reached the model as "writing the file did not succeed". A refusal that cannot say what it refused burns a turn.

`Workspace::read` refuses what the project's committed rules exclude, and it does that through `list` rather than through a denylist. A build tree's dependency files carry absolute host paths and have no name worth enumerating. That refusal is `NotProject`, not `Escape`, because the path is inside and is not the project.
