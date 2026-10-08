#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TEMPLATE="${1:-$ROOT/docs/technical/host-workflow-fiddle-ai.yml}"
MAIN="${2:-$ROOT/crates/fiddle-cli/src/main.rs}"

[ -f "$TEMPLATE" ] || { echo "test-host-workflow-fiddle-ai: no template at $TEMPLATE, so nothing was checked" >&2; exit 2; }
[ -f "$MAIN" ] || { echo "test-host-workflow-fiddle-ai: no $MAIN, so fiddle's own handle cannot be read" >&2; exit 2; }
command -v python3 >/dev/null 2>&1 \
  || { echo "test-host-workflow-fiddle-ai: python3 is not on the PATH, so the template cannot be parsed and nothing was checked" >&2; exit 2; }
python3 -c 'import yaml' >/dev/null 2>&1 \
  || { echo "test-host-workflow-fiddle-ai: python3 has no PyYAML, so the template cannot be parsed and nothing was checked" >&2; exit 2; }

MUTANTS=$(mktemp -d "${TMPDIR:-/tmp}/host-workflow-fiddle-ai-XXXXXX") || exit 2
trap 'rm -rf "$MUTANTS"' EXIT INT TERM

python3 - "$TEMPLATE" "$MAIN" "$MUTANTS" <<'PYTHON'
import os
import re
import sys

import yaml

template_path, main_path, mutants_dir = sys.argv[1:4]
NAME = "test-host-workflow-fiddle-ai"


def refuse(reason):
    print(f"{NAME}: {reason}", file=sys.stderr)
    sys.exit(2)


main_text = open(main_path, encoding="utf-8").read()
found = re.findall(r'^const MENTION: &str = "([^"]+)";$', main_text, re.M)
if len(found) != 1:
    refuse(f"{main_path} names {len(found)} MENTION constants, so fiddle's own handle cannot be compared")
HANDLE = found[0]

ASSOCIATIONS = """fromJSON('["OWNER", "MEMBER", "COLLABORATOR"]')"""
EXPECTED_IF = " ".join(f"""
github.event.sender.type != 'Bot'
&& (
  (
    github.event_name == 'issue_comment'
    && github.event.issue.pull_request
    && contains(github.event.comment.body, '@{HANDLE}')
    && contains({ASSOCIATIONS}, github.event.comment.author_association)
  )
  || (
    github.event_name == 'pull_request_review'
    && contains(github.event.review.body, '@{HANDLE}')
    && contains({ASSOCIATIONS}, github.event.review.author_association)
  )
  || (
    github.event_name == 'pull_request_review_comment'
    && contains(github.event.comment.body, '@{HANDLE}')
    && contains({ASSOCIATIONS}, github.event.comment.author_association)
  )
)
""".split())
NUMBER = "${{ github.event.issue.number || github.event.pull_request.number }}"
DEFAULT_BRANCH = "${{ github.event.repository.default_branch }}"
EVENTS = {
    "issue_comment": ["created"],
    "pull_request_review": ["submitted"],
    "pull_request_review_comment": ["created"],
}
HEADER_LINES = [
    "#   contents: write ",
    "#   pull-requests: write ",
    "#   issues: write ",
    "#   checks: read ",
    "#   actions: read ",
    "#   issue_comment",
    "#   pull_request_review",
    "#   pull_request_review_comment",
    '#   account = "fiddle-ai[bot]"',
    '#   name = "fiddle-ai[bot]"',
    '#   email = "<app-user-id>+fiddle-ai[bot]@users.noreply.github.com"',
]


def grants_write(permissions):
    if isinstance(permissions, str):
        return permissions != "read-all"
    if isinstance(permissions, dict):
        return any(value not in ("read", "none") for value in permissions.values())
    return True


def every_permissions(node, path="workflow"):
    if isinstance(node, dict):
        for key, value in node.items():
            if key == "permissions":
                yield path, value
            yield from every_permissions(value, f"{path}.{key}")
    elif isinstance(node, list):
        for index, value in enumerate(node):
            yield from every_permissions(value, f"{path}[{index}]")


def check(text):
    failures = []
    checked = 0

    def holds(what, condition):
        nonlocal checked
        checked += 1
        if not condition:
            failures.append(what)

    try:
        document = yaml.safe_load(text)
    except yaml.YAMLError as error:
        return [f"the template does not parse as YAML: {error}"], 1
    if not isinstance(document, dict):
        return ["the template is not a mapping"], 1

    triggers = document.get("on", document.get(True))
    holds("the workflow names exactly the keys name, on, permissions and jobs",
          set(document) == {"name", True, "permissions", "jobs"} or set(document) == {"name", "on", "permissions", "jobs"})
    holds("it runs on the three events and on nothing else", isinstance(triggers, dict) and set(triggers) == set(EVENTS))
    for event, types in EVENTS.items():
        holds(f"{event} runs only on {types}",
              isinstance(triggers, dict) and isinstance(triggers.get(event), dict) and triggers[event] == {"types": types})

    holds("the workflow holds no concurrency, where an event the guard skips would replace a waiting mention",
          "concurrency" not in document)
    holds("the workflow token reads contents and nothing else", document.get("permissions") == {"contents": "read"})
    for path, permissions in every_permissions(document):
        holds(f"{path}.permissions grants no write", not grants_write(permissions))

    jobs = document.get("jobs")
    holds("the workflow holds exactly one job, and it is fiddle", isinstance(jobs, dict) and list(jobs) == ["fiddle"])
    job = jobs.get("fiddle") if isinstance(jobs, dict) else None
    if not isinstance(job, dict):
        failures.append("there is no fiddle job to check")
        return failures, checked + 1

    holds("the job holds only if, runs-on, timeout-minutes, concurrency and steps",
          set(job) == {"if", "runs-on", "timeout-minutes", "concurrency", "steps"})
    guard = " ".join(str(job.get("if", "")).split())
    holds(f"the job's if is exactly the guard on @{HANDLE}, an entitled association and a sender that is no bot",
          guard == EXPECTED_IF)
    holds("the job queues one run per pull request and never cancels one",
          job.get("concurrency") == {"group": f"fiddle-ai-{NUMBER}", "cancel-in-progress": False})

    steps = job.get("steps")
    if not isinstance(steps, list) or not all(isinstance(step, dict) for step in steps):
        failures.append("the job's steps are not a list of mappings")
        return failures, checked + 1

    checkouts = [step for step in steps if str(step.get("uses", "")).startswith("actions/checkout")]
    holds("there is a checkout", len(checkouts) >= 1)
    for index, step in enumerate(checkouts):
        given = step.get("with") if isinstance(step.get("with"), dict) else {}
        holds(f"checkout {index + 1} names the default branch, so no event checks out the merge ref",
              given.get("ref") == DEFAULT_BRANCH)
        holds(f"checkout {index + 1} persists no credential", given.get("persist-credentials") is False)

    minted = [step for step in steps if str(step.get("uses", "")).startswith("actions/create-github-app-token@")]
    holds("one step mints the App token", len(minted) == 1)
    if len(minted) == 1:
        given = minted[0].get("with") if isinstance(minted[0].get("with"), dict) else {}
        holds("the App token step is the one the run reads", minted[0].get("id") == "app")
        holds("the App id comes from a secret", str(given.get("app-id", "")).startswith("${{ secrets."))
        holds("the private key comes from a secret", str(given.get("private-key", "")).startswith("${{ secrets."))

    runs = [step for step in steps if "run" in step]
    for index, step in enumerate(runs):
        script = str(step["run"])
        expressions = re.findall(r"\$\{\{.*?\}\}", script)
        holds(f"run step {index + 1} interpolates nothing an event supplies; it reads the event through env",
              all(expression == "${{ github.repository }}" for expression in expressions))
        holds(f"run step {index + 1} passes no --capability", "--capability" not in script)

    fiddles = [step for step in runs
               if '"$fiddle" run "pr:${{ github.repository }}#${PULL_REQUEST}"' in str(step["run"])]
    holds("one step runs fiddle on the pr: reference", len(fiddles) == 1)
    if len(fiddles) == 1:
        environment = fiddles[0].get("env") if isinstance(fiddles[0].get("env"), dict) else {}
        holds("the pull request number comes from the event", environment.get("PULL_REQUEST") == NUMBER)
        holds("fiddle runs with the minted token",
              environment.get("FIDDLE_GITHUB_TOKEN") == "${{ steps.app.outputs.token }}")

    header = []
    for line in text.splitlines():
        if not line.startswith("#"):
            break
        header.append(line)
    for wanted in HEADER_LINES:
        holds(f"the header names {wanted.strip('# ').strip()}",
              any(line == wanted.rstrip() or line.startswith(wanted) for line in header))

    return failures, checked


template = open(template_path, encoding="utf-8").read()

print("the template, parsed")
failures, checked = check(template)
for failure in failures:
    print(f"{NAME}: FAIL: {failure}", file=sys.stderr)

CHECKOUT = """      - uses: actions/checkout@v4
        with:
          ref: ${{ github.event.repository.default_branch }}
          fetch-depth: 0
          persist-credentials: false
"""
MINT_WITH = """        with:
          app-id: ${{ secrets.FIDDLE_AI_APP_ID }}
"""
CONCURRENCY = """    concurrency:
      group: fiddle-ai-${{ github.event.issue.number || github.event.pull_request.number }}
      cancel-in-progress: false
"""
FIDDLE_RUN = '"$fiddle" run "pr:${{ github.repository }}#${PULL_REQUEST}" --config "$FIDDLE_CONFIG"'
REVIEW_ASSOCIATION = "&& contains(fromJSON('[\"OWNER\", \"MEMBER\", \"COLLABORATOR\"]'), github.event.review.author_association)"
REVIEW_BODY = f"&& contains(github.event.review.body, '@{HANDLE}')"
BOT = "      github.event.sender.type != 'Bot'\n      && (\n"

MUTANTS = [
    ("a negated association", [(REVIEW_ASSOCIATION, REVIEW_ASSOCIATION.replace("&& contains", "&& !contains"))]),
    ("a negated mention", [(REVIEW_BODY, REVIEW_BODY.replace("&& contains", "&& !contains"))]),
    ("a second job under a quoted key", [(FIDDLE_RUN, FIDDLE_RUN + '\n\n  "other":\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo anyone\n')]),
    ("persist-credentials moved under another step's with", [("          persist-credentials: false\n", ""),
                                                            (MINT_WITH, MINT_WITH + "          persist-credentials: false\n")]),
    ("ref moved under another step's with", [("          ref: ${{ github.event.repository.default_branch }}\n", ""),
                                            (MINT_WITH, MINT_WITH + "          ref: ${{ github.event.repository.default_branch }}\n")]),
    ("job-level permissions: write-all", [("    runs-on: ubuntu-latest\n", "    runs-on: ubuntu-latest\n    permissions: write-all\n")]),
    ("a second checkout with a quoted uses", [(CHECKOUT, CHECKOUT + '\n      - uses: "actions/checkout@v4"\n')]),
    ("the head ref injected into a run line", [(FIDDLE_RUN, FIDDLE_RUN + '\n          echo "${{ github.event.pull_request.head.ref }}"')]),
    ("concurrency on the workflow", [(CONCURRENCY, ""), ("jobs:\n", "concurrency:\n  group: fiddle-ai-${{ github.event.issue.number || github.event.pull_request.number }}\n  cancel-in-progress: false\n\njobs:\n")]),
    ("a running run cancelled", [("      cancel-in-progress: false\n", "      cancel-in-progress: true\n")]),
    ("CONTRIBUTOR entitled", [('"MEMBER", "COLLABORATOR"]\'), github.event.review', '"MEMBER", "COLLABORATOR", "CONTRIBUTOR"]\'), github.event.review')]),
    ("the bot exclusion dropped", [(BOT, "      (\n")]),
    ("an extra event", [("  pull_request_review_comment:\n    types: [created]\n",
                         "  pull_request_review_comment:\n    types: [created]\n  issues:\n    types: [opened]\n")]),
    ("pull_request_target", [("  pull_request_review_comment:\n    types: [created]\n",
                              "  pull_request_review_comment:\n    types: [created]\n  pull_request_target:\n    types: [opened]\n")]),
    ("--capability toil", [(FIDDLE_RUN, FIDDLE_RUN + " --capability toil")]),
    ("an alternative that is always true", [("      )\n    runs-on:", "      ) || true\n    runs-on:")]),
    ("another handle", [(f"'@{HANDLE}'", "'@fiddle'")]),
    ("the workflow token writes contents", [("permissions:\n  contents: read\n", "permissions:\n  contents: write\n")]),
    ("the run reads the workflow token", [("${{ steps.app.outputs.token }}", "${{ secrets.GITHUB_TOKEN }}")]),
    ("the header loses checks: read", [("#   checks: read ", "#   checks: none ")]),
]

print("each widening mutant is refused")
refused = 0
escaped = []
for name, edits in MUTANTS:
    mutated = template
    applied = True
    for old, new in edits:
        if old not in mutated:
            applied = False
            break
        mutated = mutated.replace(old, new)
    if not applied or mutated == template:
        escaped.append(f"the mutant '{name}' did not apply to the template, so it measured nothing")
        continue
    path = os.path.join(mutants_dir, re.sub(r"[^a-z0-9]+", "-", name.lower()) + ".yml")
    with open(path, "w", encoding="utf-8") as out:
        out.write(mutated)
    caught, _ = check(open(path, encoding="utf-8").read())
    if caught:
        refused += 1
    else:
        escaped.append(f"the mutant '{name}' passed every check")

for failure in escaped:
    print(f"{NAME}: FAIL: {failure}", file=sys.stderr)
failures += escaped

print(f"{NAME}: {refused} of {len(MUTANTS)} mutants refused")
if failures:
    print(f"{NAME}: FAIL ({len(failures)} failures; {checked} checks on the template)", file=sys.stderr)
    sys.exit(1)
print(f"{NAME}: PASS ({checked} checks on the template, {refused} of {len(MUTANTS)} mutants refused)")
PYTHON
