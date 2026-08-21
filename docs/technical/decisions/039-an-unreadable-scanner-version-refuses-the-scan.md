# 039 — An unreadable scanner version refuses the scan

Status: accepted
Cites: scanner/wizcli.rs::scanner_version, ScanError::Unparseable, Recurrence::Permanent, Outcome::NoArtefact, RescanVerdict::Provisional, RescanVerdict::Cleared, Repair::scanned_at, evaluate::judge, extraInfo.clientVersion

## Context

The scan document records its scanner in `extraInfo.clientVersion`, and the rescan compares that version against `Repair::scanned_at`. `scanner_version` read the field and ended with a default value, so an absent field became an empty string. Two empty strings compare equal, so `Provisional` never fired and a rescan at an unknown version reported proof.

## Decision

Return a `Result` from `scanner_version`. Refuse a document that records no version, or a blank one, as `ScanError::Unparseable`. Leave the comparison in `evaluate::judge` alone, because `Repair::scanned_at` is only ever filled from a report's version, so no unreadable version reaches it.

## Consequences

- `Unparseable` is `Recurrence::Permanent`, so nothing retries the refusal. The scan check fails as `Outcome::NoArtefact`, and the tree is rejected rather than provisional.
- A scanner release that renames or removes the field stops every sweep. An unattended run then repairs nothing until a person reads the report.
- The alternative, if that happens: make the version an `Option`, and let `RescanVerdict::Provisional` fire. `Provisional` already states what fiddle cannot prove, which is what an absent version states, and the sweep continues without calling the result proof.
- This is a new rule, not an application of ADR 016. `fiddle-t36b` and `fiddle-je9h` refused when the thing they needed was unreadable; here the findings are readable and only the provenance is absent.
- What was given up: a sweep that continues through a scanner release fiddle did not expect. `RescanVerdict::Cleared` is now unreachable from a document that cannot name its scanner.
