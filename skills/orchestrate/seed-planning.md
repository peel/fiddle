# Seed planning protocol

This is a private orchestrate protocol, not a public skill. Use it only when the phase resolver returns `SEED` for an explicit epic containing exactly one direct child tagged `planning`.

## Context reconstruction

Use the canonical main-worktree Beans path from [resumption](resumption.md). Load:

- the selected epic and planning seed;
- its parent milestone and referenced RFC or design source;
- its immediate `blocked_by` predecessor and marker-delimited Milestone Handoff, when present;
- current Git revision and working-tree state;
- current repository docs, calibration, acceptance evidence, and recorded debt relevant to the epic.

The predecessor handoff is the restart boundary. Do not replay all older milestone beans or rely on chat history. Promote cross-cutting knowledge that must outlive one milestone into permanent repository documentation or the RFC.

## Execute or resume

1. Mark a `todo` seed `in-progress`. If already `in-progress`, reuse existing local design/plan artifacts and generation-tagged beans rather than starting over.
2. Establish baseline evidence with read-only checks. Record the revision, dirty state, verification result, and external assumptions; do not repair unrelated failures during planning.
3. Derive a topic from the epic and invoke `fiddle:discover <topic>`, then `fiddle:brainstorm --from-orchestrate`, `fiddle:challenge --phase define`, and finally `fiddle:write-plan --from-orchestrate --epic <epic-id>`. Pass the reconstructed context and canonical Beans path invariant through every call.
4. Require every materialized implementation bean to identify this seed with `generated-by:<seed-id>` and its stable plan position with `plan-task:<position>`. Every bean body must link the original RFC/design source when one exists.
5. Validate bean bodies, parentage, dependencies, unique generation identities, and acceptance/evaluation instructions. Rerun `scripts/resolve-orchestrate-phase.sh`; do not complete the seed until generated work exists and validation passes.

## Durable seed evidence

Replace, rather than append, exactly one block in the seed body:

```markdown
<!-- seed-evidence:start -->
## Seed Evidence

- Revision and dirty state: ...
- Baseline verification: ...
- External assumptions: ...
- Design and plan: ...
- Calibration and generated beans: ...
- Validation: ...
<!-- seed-evidence:end -->
```

Update through a temporary body file and the canonical Beans path. Keep decisions and references, not raw model transcripts or secrets. After the block and generated beans validate, mark the seed completed and return to orchestrate so it can resolve `DEVELOP`.
