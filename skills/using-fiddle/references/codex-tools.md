# Codex Mapping

- Read files with normal file inspection commands; prefer `rg` for search.
- Run shell commands with the command execution tool.
- Edit files with `apply_patch` for manual changes.
- Treat `Skill(...)` as "load and follow the named skill"; do not expect a literal callable.
- Use Codex subagents only when the user explicitly asked for delegation or parallel agent work. Otherwise run sequentially and report any reduced review coverage.
- Before an internal subagent dispatch, run the model resolver. Pass its `model` when the harness supports an explicit subagent model; otherwise omit it and report the inherited-session fallback.
- Replace Claude plugin-root environment variables with paths relative to the repository/plugin root.
