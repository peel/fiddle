#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TEMPLATE="${1:-$ROOT/docs/technical/host-workflow-fiddle-ai.yml}"
MAIN="${2:-$ROOT/crates/fiddle-cli/src/main.rs}"
HELD="${3:-$ROOT/docs/technical/host-workflow-fiddle-ai.sha256}"

[ -f "$TEMPLATE" ] || { echo "test-host-workflow-fiddle-ai: no template at $TEMPLATE, so nothing was checked" >&2; exit 2; }
[ -f "$MAIN" ] || { echo "test-host-workflow-fiddle-ai: no $MAIN, so fiddle's own handle cannot be read" >&2; exit 2; }
[ -f "$HELD" ] || { echo "test-host-workflow-fiddle-ai: no held sha256 at $HELD, so the template has nothing to equal" >&2; exit 2; }
command -v python3 >/dev/null 2>&1 \
  || { echo "test-host-workflow-fiddle-ai: python3 is not on the PATH, so nothing was checked" >&2; exit 2; }
python3 -c 'import yaml' >/dev/null 2>&1 \
  || { echo "test-host-workflow-fiddle-ai: python3 has no PyYAML, so the reasons cannot be named and nothing was checked" >&2; exit 2; }

python3 - "$TEMPLATE" "$MAIN" "$HELD" <<'PYTHON'
import copy
import hashlib
import re
import sys

import yaml

template_path, main_path, held_path = sys.argv[1:4]
NAME = "test-host-workflow-fiddle-ai"
MEASURES = ("the template's bytes must equal the reviewed copy's (its sha256 is held beside it); "
            "any edit to the template, a comment included, fails the gate until the held sha256 is changed")


def refuse(reason):
    print(f"{NAME}: {reason}", file=sys.stderr)
    sys.exit(2)


found = re.findall(r'^const MENTION: &str = "([^"]+)";$', open(main_path, encoding="utf-8").read(), re.M)
if len(found) != 1:
    refuse(f"{main_path} names {len(found)} MENTION constants, so fiddle's own handle cannot be compared")
HANDLE = found[0]

held = open(held_path, encoding="utf-8").read().split()
if not held or not re.fullmatch(r"[0-9a-f]{64}", held[0]):
    refuse(f"{held_path} holds no sha256, so the template has nothing to equal")
HELD = held[0]

TEMPLATE_BYTES = open(template_path, "rb").read()
TEMPLATE_TEXT = TEMPLATE_BYTES.decode("utf-8", errors="replace")


class UniqueKeys(yaml.SafeLoader):
    pass


def unique_mapping(loader, node, deep=False):
    seen = set()
    for key_node, _ in node.value:
        key = loader.construct_object(key_node, deep=deep)
        if key in seen:
            raise yaml.constructor.ConstructorError(None, None, f"the key {key!r} is written twice", key_node.start_mark)
        seen.add(key)
    return loader.construct_mapping(node, deep)


UniqueKeys.add_constructor(yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, unique_mapping)

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
)
""".split())
NUMBER = "${{ github.event.issue.number || github.event.pull_request.number }}"
CHECKOUT = {
    "uses": "actions/checkout@v4",
    "with": {
        "ref": "${{ github.event.repository.default_branch }}",
        "fetch-depth": 0,
        "persist-credentials": False,
    },
}


def reasons(text):
    try:
        document = yaml.load(text, Loader=UniqueKeys)
    except yaml.YAMLError as error:
        return [f"it does not parse as YAML: {error}"]
    if not isinstance(document, dict):
        return ["it is not a mapping"]
    found = []
    if True in document and "on" in document:
        found.append("it holds both a bare on and a quoted \"on\"")
    jobs = document.get("jobs") if isinstance(document.get("jobs"), dict) else {}
    job = jobs.get("fiddle") if isinstance(jobs.get("fiddle"), dict) else {}
    if " ".join(str(job.get("if", "")).split()) != EXPECTED_IF:
        found.append(f"the job's if is not the guard on @{HANDLE}, an entitled association and a sender that is no bot")
    if job.get("concurrency") != {"group": f"fiddle-ai-{NUMBER}", "cancel-in-progress": False}:
        found.append("the job does not queue one run per pull request without cancelling one")
    steps = job.get("steps") if isinstance(job.get("steps"), list) else []
    if not steps or steps[0] != CHECKOUT:
        found.append("the first step is not a checkout of the default branch that persists no credential")
    if f"'@{HANDLE}'" not in str(job.get("if", "")):
        found.append(f"the guard's handle is not @{HANDLE}, the handle main.rs names in MENTION")
    return found


def check(data):
    if hashlib.sha256(data).hexdigest() == HELD:
        return []
    return [MEASURES + "; this template's sha256 is " + hashlib.sha256(data).hexdigest()] + [
        f"  and {reason}" for reason in reasons(data.decode("utf-8", errors="replace"))
    ]


failures = check(TEMPLATE_BYTES)
for failure in failures:
    print(f"{NAME}: FAIL: {failure}", file=sys.stderr)
if not failures:
    print("the template equals the reviewed copy")

TEMPLATE = yaml.load(TEMPLATE_TEXT, Loader=UniqueKeys) if not failures else None
if isinstance(TEMPLATE, dict) and True in TEMPLATE:
    TEMPLATE["on"] = TEMPLATE.pop(True)


def job_of(document):
    return document["jobs"]["fiddle"]


def steps_of(document):
    return job_of(document)["steps"]


def step_named(document, name):
    matching = [step for step in steps_of(document) if step.get("name") == name]
    if len(matching) != 1:
        raise LookupError(f"no single step named {name!r}")
    return matching[0]


RUN = "Run fiddle on the pull request"
DOWNLOAD = "Download fiddle and verify it before it runs"
MINT = "Mint the fiddle-ai installation token"
REVIEW_ASSOCIATION = "&& contains(fromJSON('[\"OWNER\", \"MEMBER\", \"COLLABORATOR\"]'), github.event.review.author_association)"
REVIEW_BODY = f"&& contains(github.event.review.body, '@{HANDLE}')"
SHA_CHECK = """if [ "$measured" != "$FIDDLE_SHA256" ]; then
  echo "${FIDDLE_TAG} publishes a binary this job does not pin." >&2
  echo "  published ${measured}" >&2
  echo "  pinned    ${FIDDLE_SHA256}" >&2
  echo "Read the release. Then update FIDDLE_TAG and FIDDLE_SHA256 together." >&2
  exit 1
fi
"""


def replace_in(text, old, new):
    if old not in text:
        raise LookupError(f"{old[:40]!r} is not in the template")
    return text.replace(old, new, 1)


def set_if(document, old, new):
    job_of(document)["if"] = replace_in(job_of(document)["if"], old, new)


def append_run(document, name, line):
    step = step_named(document, name)
    step["run"] = step["run"] + line + "\n"


def add_step(document, step, at=None):
    steps = steps_of(document)
    steps.insert(len(steps) if at is None else at, step)


def set_env(document, name, key, value):
    step_named(document, name).setdefault("env", {})[key] = value


MUTANTS = [
    ("a negated association", lambda d: set_if(d, REVIEW_ASSOCIATION, REVIEW_ASSOCIATION.replace("&& contains", "&& !contains"))),
    ("a negated mention", lambda d: set_if(d, REVIEW_BODY, REVIEW_BODY.replace("&& contains", "&& !contains"))),
    ("a second job under a quoted key", lambda d: d["jobs"].__setitem__("other", {"runs-on": "ubuntu-latest", "steps": [{"run": "echo anyone"}]})),
    ("persist-credentials moved under another step's with", lambda d: (steps_of(d)[0]["with"].pop("persist-credentials"), step_named(d, MINT)["with"].__setitem__("persist-credentials", False))),
    ("ref moved under another step's with", lambda d: (step_named(d, MINT)["with"].__setitem__("ref", steps_of(d)[0]["with"].pop("ref")))),
    ("job-level permissions: write-all", lambda d: job_of(d).__setitem__("permissions", "write-all")),
    ("a second checkout with a quoted uses", lambda d: add_step(d, {"uses": "actions/checkout@v4"})),
    ("the head ref injected into a run line", lambda d: append_run(d, RUN, 'echo "${{ github.event.pull_request.head.ref }}"')),
    ("concurrency on the workflow", lambda d: d.__setitem__("concurrency", job_of(d).pop("concurrency"))),
    ("a running run cancelled", lambda d: job_of(d)["concurrency"].__setitem__("cancel-in-progress", True)),
    ("CONTRIBUTOR entitled", lambda d: set_if(d, '"COLLABORATOR"]\'), github.event.review', '"COLLABORATOR", "CONTRIBUTOR"]\'), github.event.review')),
    ("the bot exclusion dropped", lambda d: set_if(d, "github.event.sender.type != 'Bot' && ", "")),
    ("an extra event", lambda d: d["on"].__setitem__("pull_request_review_comment", {"types": ["created"]})),
    ("pull_request_target", lambda d: d["on"].__setitem__("pull_request_target", {"types": ["opened"]})),
    ("--capability toil", lambda d: step_named(d, RUN).__setitem__("run", replace_in(step_named(d, RUN)["run"], '--config "$FIDDLE_CONFIG"\n', '--config "$FIDDLE_CONFIG" --capability toil\n'))),
    ("an alternative that is always true", lambda d: job_of(d).__setitem__("if", job_of(d)["if"] + " || true")),
    ("another handle", lambda d: set_if(d, f"'@{HANDLE}'", "'@fiddle'")),
    ("the workflow token writes contents", lambda d: d["permissions"].__setitem__("contents", "write")),
    ("the run reads the workflow token", lambda d: set_env(d, RUN, "FIDDLE_GITHUB_TOKEN", "${{ secrets.GITHUB_TOKEN }}")),
    ("a second issue_comment type", lambda d: d["on"]["issue_comment"]["types"].append("edited")),
    ("an expression across lines in a run", lambda d: append_run(d, RUN, 'echo "${{\n  github.event.pull_request.head.ref\n}}"')),
    ("event text in github-script", lambda d: add_step(d, {"uses": "actions/github-script@v7", "with": {"script": "core.info(`${{ github.event.comment.body }}`)"}})),
    ("event text in a docker step", lambda d: add_step(d, {"uses": "docker://alpine:3", "with": {"args": "sh -c \"${{ github.event.comment.body }}\""}})),
    ("a checkout spelled Actions/checkout", lambda d: steps_of(d)[0].__setitem__("uses", "Actions/checkout@v4")),
    ("a checkout of the head repository", lambda d: steps_of(d)[0]["with"].__setitem__("repository", "${{ github.event.pull_request.head.repo.full_name }}")),
    ("the merge ref fetched and checked out", lambda d: append_run(d, RUN, 'git fetch origin "refs/pull/${PULL_REQUEST}/merge" && git checkout FETCH_HEAD')),
    ("env text evaluated", lambda d: (set_env(d, RUN, "ASK", "${{ github.event.comment.body }}"), append_run(d, RUN, 'eval "$ASK"'))),
    ("the whole event in env", lambda d: set_env(d, RUN, "EVENT", "${{ toJSON(github.event) }}")),
    ("a working directory from event text", lambda d: step_named(d, RUN).__setitem__("working-directory", "${{ github.event.pull_request.head.ref }}")),
    ("the pinned digest check removed", lambda d: step_named(d, DOWNLOAD).__setitem__("run", replace_in(step_named(d, DOWNLOAD)["run"], SHA_CHECK, ""))),
    ("a step that may fail", lambda d: step_named(d, DOWNLOAD).__setitem__("continue-on-error", True)),
    ("a job that may fail", lambda d: job_of(d).__setitem__("continue-on-error", True)),
    ("event text in job env", lambda d: job_of(d).__setitem__("env", {"ASK": "${{ github.event.comment.body }}"})),
    ("a local action from the checkout", lambda d: add_step(d, {"uses": "./.github/actions/fiddle"})),
    ("fiddle from another repository", lambda d: set_env(d, DOWNLOAD, "FIDDLE_REPO", "someone/fiddle")),
    ("a self-hosted runner", lambda d: job_of(d).__setitem__("runs-on", "self-hosted")),
]


def edit(old, new):
    def apply(text):
        if old not in text:
            raise LookupError(f"{old[:40]!r} is not in the template")
        return text.replace(old, new, 1)
    return apply


TEXT_MUTANTS = [
    ("a comment edited", edit("# One run at a time per pull request.", "# Any number of runs per pull request.")),
    ("cancel-in-progress written off", edit("      cancel-in-progress: false\n", "      cancel-in-progress: off\n")),
    ("timeout-minutes written 1:15", edit("    timeout-minutes: 75\n", "    timeout-minutes: 1:15\n")),
    ("on written ON", edit("\non:\n", "\nON:\n")),
    ("a quoted on beside the bare one, with pull_request_target", edit("\npermissions:\n", '\n"on":\n  pull_request_target:\n    types: [opened]\n\npermissions:\n')),
    ("a value tagged !!str", edit("    runs-on: ubuntu-latest\n", "    runs-on: !!str ubuntu-latest\n")),
    ("persist-credentials written no", edit("          persist-credentials: false\n", "          persist-credentials: no\n")),
]

refused = 0
drifted = []
escaped = []
total = len(MUTANTS) + len(TEXT_MUTANTS)
if TEMPLATE is None:
    print(f"{NAME}: the mutants were not run, because the template is not the reviewed copy", file=sys.stderr)
else:
    print("each mutant of the reviewed template must be refused")
    for name, mutate in MUTANTS:
        mutant = copy.deepcopy(TEMPLATE)
        try:
            mutate(mutant)
        except (KeyError, IndexError, LookupError, TypeError, AttributeError) as error:
            drifted.append(f"{name}: {error}")
            continue
        if mutant == TEMPLATE:
            drifted.append(f"{name}: it changed nothing")
            continue
        if check(yaml.safe_dump(mutant, sort_keys=False).encode("utf-8")):
            refused += 1
        else:
            escaped.append(name)
    for name, mutate in TEXT_MUTANTS:
        try:
            mutated = mutate(TEMPLATE_TEXT)
        except LookupError as error:
            drifted.append(f"{name}: {error}")
            continue
        if check(mutated.encode("utf-8")):
            refused += 1
        else:
            escaped.append(name)
    print(f"{NAME}: {refused} of {total} mutants refused by the byte pin")

for name in escaped:
    print(f"{NAME}: FAIL: the mutant '{name}' passed every check", file=sys.stderr)
if drifted:
    for line in drifted:
        print(f"{NAME}: the suite's fixture drifted from the template, so this mutant measured nothing: {line}", file=sys.stderr)
    sys.exit(2)
if failures or escaped:
    print(f"{NAME}: FAIL", file=sys.stderr)
    sys.exit(1)
print(f"{NAME}: PASS (the template equals the reviewed copy; {refused} of {total} mutants refused by the byte pin alone)")
PYTHON
