# 041 — The scan is published whole, in a second file beside the bounded set

Status: accepted
Cites: cve/verdict.rs, cve/project.rs, CompleteFindings, Disposition::write_findings, Disposition::write_report, FINDINGS_FILE, REPORT_FILE, RunDisposition, Budget::apply, findings.json, verdicts.json

## Context

The host's CVE feed reports every finding still present in the build. fiddle scans the whole image, keeps `max_findings` of the projection and defers the rest, so `verdicts.json` is a bounded subset by design. `TreeObservation` carries the scanned digest and no findings, so the host cannot read what fiddle found and still runs its own `wizcli`.

## Decision

Publish the whole projection as `findings.json` in `[report] dir`, beside `verdicts.json`. Keep `Budget::apply` and the bound exactly as they are. State the count in both files: `findings.json` carries `projected`, and `RunDisposition` carries the same number beside its verdict count.

## Consequences

- `verdicts.json` keeps its shape. It is a bare array that the remediation gate and the Jira step already read, so a second consumer gets a second file rather than an object wrapping the array.
- `write_report` stays the only writer of `verdicts.json`. `write_findings` is the only writer of `findings.json`, and `CveMitigate::publish_reports` calls both and publishes a receipt for each.
- The document distinguishes a scan that found nothing from a scan that did not run. `CompleteFindings::Scanned` carries a list and a count; `CompleteFindings::Unusable` carries neither and names the failure. `RunDisposition::projected` is absent for the second, not zero.
- `findings.json` publishes the projection, which is the grades `severities` names. A deployment that wants a lower grade in the feed names it in `severities`, as ADR 021 requires. This file does not widen the selection.
- `ProjectedFinding` gains `Serialize` in the spelling it already reads, so a host deserializes the file fiddle wrote into the type that wrote it.
- What the project gave up: one file for one run. A reader of `[report] dir` must now ask which of two questions each file answers — what the scanner found, or what fiddle worked on.
