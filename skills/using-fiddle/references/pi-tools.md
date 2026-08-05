# Pi Mapping

- Pi loads Agent Skills from package, project, global, or explicit skill paths.
- Use `/skill:<name>` when a skill must be forced into context.
- Read/search/edit/run commands through Pi's available local tools.
- Treat `Skill(...)` as "load and follow the named skill"; do not expect a literal callable.
- For parallel work, use Pi's available subagent package or run reviewers sequentially if subagents are unavailable.
- Before an internal subagent dispatch, run the model resolver. Pass its `model` when the available subagent package supports model selection; otherwise omit it and retain session inheritance.
- Replace Claude plugin-root environment variables with paths relative to the repository/plugin root.
