#!/usr/bin/env bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SHAPE="$SCRIPT_DIR/live-jira-search-shape.sh"
WRITE="$SCRIPT_DIR/live-jira-write.sh"

UNREACHABLE="https://127.0.0.1:1"
FAILED=0
CHECKED=0

fail() { echo "test-live-jira-lanes: $*" >&2; FAILED=$((FAILED + 1)); }

ran() {
  local lane="$1"; shift
  OUT=$(env -i PATH="$PATH" HOME="$HOME" "$@" "$lane" 2>&1)
  CODE=$?
  CHECKED=$((CHECKED + 1))
}

refuses_without() {
  local lane="$1" absent="$2"; shift 2
  ran "$lane" "$@"
  if [ "$CODE" -eq 0 ]; then
    fail "$(basename "$lane") exited 0 with $absent absent; a lane that skips silently cannot be told from one that passed"
    return
  fi
  case "$OUT" in
    *"this lane needs $absent"*) ;;
    *) fail "$(basename "$lane") refused without $absent and did not name it: $OUT" ;;
  esac
}

for absent in JIRA_USER_EMAIL JIRA_API_TOKEN JIRA_SITE JIRA_SEARCH_PROJECT; do
  args=()
  [ "$absent" = JIRA_USER_EMAIL ] || args+=("JIRA_USER_EMAIL=bot@example.invalid")
  [ "$absent" = JIRA_API_TOKEN ] || args+=("JIRA_API_TOKEN=not-a-real-token")
  [ "$absent" = JIRA_SITE ] || args+=("JIRA_SITE=$UNREACHABLE")
  [ "$absent" = JIRA_SEARCH_PROJECT ] || args+=("JIRA_SEARCH_PROJECT=IDENT")
  refuses_without "$SHAPE" "$absent" "${args[@]}"
done

for absent in JIRA_USER_EMAIL JIRA_API_TOKEN JIRA_SITE JIRA_WRITE_PROJECT; do
  args=()
  [ "$absent" = JIRA_USER_EMAIL ] || args+=("JIRA_USER_EMAIL=bot@example.invalid")
  [ "$absent" = JIRA_API_TOKEN ] || args+=("JIRA_API_TOKEN=not-a-real-token")
  [ "$absent" = JIRA_SITE ] || args+=("JIRA_SITE=$UNREACHABLE")
  [ "$absent" = JIRA_WRITE_PROJECT ] || args+=("JIRA_WRITE_PROJECT=DISPOSABLE")
  refuses_without "$WRITE" "$absent" "${args[@]}"
done

ran "$SHAPE" JIRA_USER_EMAIL=bot@example.invalid JIRA_API_TOKEN=not-a-real-token \
  JIRA_SITE=http://insecure.example.invalid JIRA_SEARCH_PROJECT=IDENT
[ "$CODE" -ne 0 ] || fail "the shape lane accepted a plaintext origin, and a credential rides every request it sends"
case "$OUT" in *"must be an https origin"*) ;; *) fail "the shape lane refused a plaintext origin without saying why: $OUT" ;; esac

ran "$WRITE" JIRA_USER_EMAIL=bot@example.invalid JIRA_API_TOKEN=not-a-real-token \
  JIRA_SITE="$UNREACHABLE" JIRA_WRITE_PROJECT=ISP JIRA_ISSUE=ISP-1
[ "$CODE" -ne 0 ] || fail "the write lane accepted the project its read lane observes as a disposable one"
case "$OUT" in *"is not the project a read lane observes"*) ;; *) fail "the write lane refused a non-disposable project without saying why: $OUT" ;; esac

ran "$WRITE" JIRA_USER_EMAIL=bot@example.invalid JIRA_API_TOKEN=not-a-real-token \
  JIRA_SITE="$UNREACHABLE" JIRA_WRITE_PROJECT="not a key"
[ "$CODE" -ne 0 ] || fail "the write lane accepted a project key that is not one"
case "$OUT" in *"must be a bare project key"*) ;; *) fail "the write lane refused a malformed key without saying why: $OUT" ;; esac

for lane in "$SHAPE" "$WRITE"; do
  case "$lane" in
    "$SHAPE") ran "$lane" JIRA_USER_EMAIL=bot@example.invalid JIRA_API_TOKEN=not-a-real-token \
                 JIRA_SITE="$UNREACHABLE" JIRA_SEARCH_PROJECT=IDENT ;;
    *)        ran "$lane" JIRA_USER_EMAIL=bot@example.invalid JIRA_API_TOKEN=not-a-real-token \
                 JIRA_SITE="$UNREACHABLE" JIRA_WRITE_PROJECT=DISPOSABLE ;;
  esac
  [ "$CODE" -ne 0 ] || fail "$(basename "$lane") exited 0 against a site that answers nothing, so it reported a measurement it never took"
  case "$OUT" in
    *"this lane needs"*)
      fail "$(basename "$lane") reported a missing variable when every variable was given, so the refusals above would pass for a lane that refuses everything: $OUT"
      ;;
    *"would not answer"*) ;;
    *) fail "$(basename "$lane") gave every variable and an unreachable site, and said neither: $OUT" ;;
  esac
done

printf 'test-live-jira-lanes: %d cases run, %d failed\n' "$CHECKED" "$FAILED"
[ "$FAILED" -eq 0 ] || exit 1
printf 'Live jira lane refusal tests passed\n'
