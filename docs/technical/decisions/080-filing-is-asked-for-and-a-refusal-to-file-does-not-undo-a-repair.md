# 080 — Filing is asked for, and a refusal to file does not undo a repair

Status: accepted
Cites: TicketFiling, FiledTickets, TicketFiled, FILINGS_FILE, ticket_proposals, TicketProposal, FileVerdict, MitigateConfig, JiraFiling, filing_client, JiraError, CapabilityError::Write, JIRA_ISSUE_FILED, a_verdict_carrying_a_legacy_label_files_one_ticket_and_a_second_run_files_none, the_claim_is_the_only_thing_standing_between_one_ticket_and_two, a_deployment_with_no_jira_table_completes_the_run_and_files_nothing, a_reachable_site_with_no_filing_configured_files_nothing, a_create_the_site_refuses_is_named_as_a_filing_refusal_and_the_repair_still_lands, a_ledger_the_site_does_not_hold_is_named_and_no_create_is_sent, the_filing_table_names_its_own_project_and_never_borrows_the_observed_one, a_ledger_issue_in_another_project_is_refused, a_tracker_that_files_nothing_asks_for_no_credential_to_file_with, a_tracker_that_files_is_refused_when_nothing_exports_its_credential, a_deployment_that_configured_no_filing_does_not_read_as_one_that_filed_nothing, crates/fiddle-runtime/src/capability/mitigate.rs, crates/fiddle-runtime/src/cve/verdict.rs, crates/fiddle-runtime/tests/cve_filing.rs, crates/fiddle-runtime/tests/live_jira_filing.rs, docs/technical/decisions/079-a-ticket-is-deduplicated-by-a-marker-it-carries.md

## Context

ADR 079 fixed the mechanism that files a ticket exactly once and left two
questions open: where a deployment names the project it files into, and what a
mitigate run does when the site refuses. `ticket_proposals` reached no run path,
so neither question had an answer in the tree.

## Decision one — filing is a separate table, and a deployment has to ask for it

**`[jira.filing]` names `project`, `issue_type` and `ledger_issue`. Filing
happens only when that table is present.**

`[jira].project` is the project fiddle **reads** work items from. It is not
reused, for three reasons.

- A deployment that configures `[jira]` for observation has not asked for
  tickets to be created. Reusing the observed project would start writing into
  it on the next run. `[jira].project` cannot express "observe here, file
  nowhere", so filing needs a table of its own whatever else is decided.
- The ledger issue must live in the project the tickets are filed into, because
  it is read with the same credential and the claim search is scoped to that
  project. `JiraFiling` refuses a `ledger_issue` outside its own `project`, so
  the invariant is checked once, where both values are written.
  `scripts/live-jira-write.sh` asserts the same relation between
  `JIRA_LEDGER_ISSUE` and `JIRA_WRITE_PROJECT` before it writes anything.
- The same script refuses to write into the project `JIRA_ISSUE` is read from.
  The two projects being different is the shape the only live evidence in this
  repository was gathered in.

The Jira client reaches `EffectContext` only when this table is present.
A deployment that files nothing is never refused for a credential it never
sends; a deployment that asked to file and exported no credential is refused
before the run, rather than filing nothing on every run and saying so only in
the report.

## Decision two — a filing failure is recorded beside the repair, and does not fail the run

**A refusal from Jira is written as `TicketFiled::Refused` in `filings.json`.
The run still completes.**

Filing happens after the repair has landed: the branch is pushed, the pull
request is open, and `verdicts.json` and `findings.json` are written. Failing
the run at that point would report all of that as not done and invite the
orchestrator to repeat it. Requirement 22 already says a mitigation does not
require Jira, and a tracker that will not take the ticket is the same kind of
absence.

The refusal is its own state and never a `CapabilityError::Write`. That error
carries a path and a `std::io::Error`; a Jira refusal has neither, and reporting
one as the other would send a reader to the disk.

`FiledTickets::NotConfigured` and `FiledTickets::Attempted { tickets: [] }` are
different documents. A deployment that files nowhere and a run that had nothing
to file are different facts.

## Consequences

- **Given up: a refused filing does not stop anything.** A misconfigured
  project or an expired token files no tickets for as long as nobody reads
  `filings.json`. Nothing in this milestone alerts on it.
- A refusal that sent a create and one that never sent one read differently:
  the acceptance lane separates them by counting create requests, not tickets.
- `[jira.filing]` is a fourth place a deployment can be wrong. The refusals
  name the key and the value.
- **Evidence class.** Everything above is measured against the loopback stub in
  `crates/fiddle-runtime/tests/cve_filing.rs`. No part of this record has been
  driven against a real site, and ADR 079's grading of the milestone's central
  claim is unchanged by it. A green stub run is evidence about this code and
  not about Atlassian.

> Amended 2026-08-29. `FileVerdict` has since been driven against
> `snplow.atlassian.net` by `crates/fiddle-runtime/tests/live_jira_filing.rs`,
> and ADR 079 carries that record. It does not change the paragraph above. That
> lane builds a `TicketFiling` itself and calls `ticket_proposals` and the
> executor the way `CveMitigate::file` does; it never reads a `fiddle.toml`, so
> `JiraFiling`, `filing_client` and `CveMitigate::file_tickets` remain measured
> against the loopback stub and against nothing else.
