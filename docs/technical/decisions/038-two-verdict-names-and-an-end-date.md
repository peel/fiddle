# 038 — fiddle writes its own verdict and a compatibility label, and the label has an end date

Status: accepted
Cites: cve/verdict.rs, docs/technical/style.md, Row::legacy_label, Judgement::NeedsWork, Row::AttemptBoundReached, write_report, verdicts.json, non-patchable.json, cve-upstream-blocked, cve-needs-work, ticket_proposals, TICKET_LABEL_PREFIX

## Context

fiddle's report must reach a Jira step this project does not own. That step builds its label as `cve-<verdict>`, and the JQL in the host's scan job closes `cve-upstream-blocked` and `cve-needs-work`. fiddle's own `verdict` field serialises `needs_work` with an underscore.

## Decision

Write both names into `verdicts.json`. Keep `verdict` for fiddle's own disposition, and add `legacy_label` carrying the name the Jira contract already closes. Remove `legacy_label` when M5 owns the Jira step.

## Consequences

- Writing `verdict` into the Jira field would file `cve-needs_work`. `Judgement::NeedsWork` serialises under `snake_case`, and no JQL in the host looks for that label. A ticket carrying it would never close.
- `Row::legacy_label` answers `None` for six of its eight rows. Only `pull_request` and `unsafe_without_direction` carry a Jira meaning. `Row::AttemptBoundReached` and `Row::ChecksUnreadable` are both among the six, and the mapping drops a null rather than file `cve-null`.
- The end date is explicit: the label goes when M5 owns Jira. A compatibility field with no named end date becomes permanent by default. Nothing later asks whether it is still needed.
- Two names for one outcome breaks the rule of one word for one meaning. `docs/technical/style.md` asks that a broken rule is declared, and this is the declaration. The field is named `legacy_label` and not `label` for the same reason.
- What was given up: `verdicts.json` and `non-patchable.json` disagree about the word for one outcome. One file says `needs_work` and the other says `needs-work`. A reader comparing them must know which contract each file serves.

> Amended 2026-08-29 by `fiddle-2690`, the M5b record. The end date has not arrived and `legacy_label` stays. M5b gave fiddle the create: `ticket_proposals` reads `legacy_label` and labels the ticket `cve-<legacy_label>` through `TICKET_LABEL_PREFIX`, so the field is now what fiddle builds its own label from rather than what the host's jq program reads. The host still owns the close, which is a JQL on `cve-upstream-blocked` and `cve-needs-work` in its `scan` job. The field goes when that JQL goes. `non-patchable.json` is no longer written; the disagreement above now stands between `verdicts.json` and the label on the ticket.
