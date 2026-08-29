#!/usr/bin/env bash
set -euo pipefail

fail() { echo "live-jira-file-verdict: FAIL: $*" >&2; exit 2; }
note() { echo "live-jira-file-verdict: $*"; }

needs() {
  local name="$1" why="$2"
  [ -n "${!name:-}" ] || fail "this lane needs $name. $why It fails rather than skips, because a silently-skipped lane cannot be told from a passing one."
}

needs JIRA_USER_EMAIL "It is the account the writes are made as."
needs JIRA_API_TOKEN "It is the credential, and it is read from the environment and never written to a file this lane generates."
needs JIRA_SITE "It is the site origin, as in JIRA_SITE=https://snplow.atlassian.net."
needs JIRA_WRITE_PROJECT "It is the project FileVerdict files a test ticket in and this lane then CLOSES. It never deletes: deletion is refused by policy in ISP."
needs JIRA_LEDGER_ISSUE "It is an existing issue in JIRA_WRITE_PROJECT that carries the claim ledger. The lane reads and writes properties on it and never closes it."

if [ -n "${JIRA_ISSUE:-}" ] && [ "${JIRA_ISSUE%%-*}" = "$JIRA_WRITE_PROJECT" ]; then
  fail "JIRA_WRITE_PROJECT is $JIRA_WRITE_PROJECT, the project JIRA_ISSUE ($JIRA_ISSUE) is read from. A project this lane writes to is not the project a read lane observes. Unset JIRA_ISSUE for this invocation rather than weakening the guard."
fi

cd "$(dirname "$0")/.." || fail "this lane runs from the repository it tests"

note "this lane drives fiddle's own build: ticket_proposals builds the proposal, TicketProposal::operation builds FileVerdict, and the same Executor CveMitigate::file uses executes it against $JIRA_SITE."
note "scripts/live-jira-write.sh sends the same requests by hand and measures Atlassian. This one measures the build."

exec cargo test --package fiddle-runtime --test live_jira_filing -- --ignored --nocapture --exact \
  a_ticket_file_verdict_filed_is_found_by_a_later_inspect_against_the_real_site
