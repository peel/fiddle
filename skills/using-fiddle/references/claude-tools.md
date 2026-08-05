# Claude Code Mapping

- `Read`, `Grep`, `Glob`, `Bash`, `Edit`, and `Write` are Claude Code tool names.
- `Skill(...)` means invoke or follow the named Fiddle skill.
- `Agent(run_in_background: true, ...)` means start a Claude subagent in the background and collect its result.
- Before an internal subagent dispatch, run the model resolver. When its JSON includes `model`, pass that value as the subagent `model:`; otherwise omit `model:` to inherit the session.
- Claude plugin root environment variables may be used by Claude hook config, but skill instructions should prefer repo-relative script paths when possible.
