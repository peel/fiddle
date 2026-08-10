# Runbooks

<!-- How to operate this thing. Write for 2am-you or an agent that needs to
     deploy/rollback/debug without full context. Commands, not prose. -->

Fiddle is a binary and a skills library, not a service, so there is nothing to
deploy. What needs operating is **credentials** and the **two lanes that use
them**. Everything here is credential-free unless a section says otherwise.

## Credentials

Two, unrelated, both resolved by name and never by value. `fiddle.toml` holds the
*name* of an environment variable; a document containing a literal secret does not
load.

| Variable | Used by | Needed for |
| --- | --- | --- |
| `LITELLM_API_KEY` | M1 Tier 1 / Tier 2 real-model lanes | opt-in, cost money, never gate |
| `FIDDLE_GITHUB_TOKEN` | M2 local live lane | opt-in, writes to a disposable repo |
| `FIDDLE_EFFECTS_TOKEN` | M2 Actions lane | the same value as above, as a repo secret |

Both live in `.env` **in the worktree you run from** — `.envrc` is a tracked file,
so each worktree has its own and `dotenv_if_exists` resolves `.env` beside it.
`.env` is gitignored; `.env.example` is the tracked template.

### Minting the GitHub token

A **fine-grained** token, scoped to one repository. Not a classic `repo` token:
that would reach every repository you can push to, including
`peel/fiddle-acceptance`, whose whole argument is that reading it needs no
credential.

github.com/settings/personal-access-tokens/new

```
Resource owner        peel
Repository access     Only select repositories -> peel/fiddle-effects-acceptance
Expiration            90 days   # it is a test credential; bound it
```

Repository permissions — these five, and no others:

```
Contents          Read and write   # git push of the branch, reading refs
Pull requests     Read and write   # lookup by head/base, POST /pulls
Actions           Read and write   # POST .../dispatches, GET .../runs
Workflows         Read and write   # writing files under .github/workflows/**
Metadata          Read-only        # mandatory, auto-selected
```

`Actions` and `Workflows` are different grants and both are needed: `Actions`
dispatches a workflow, `Workflows` writes a workflow *file*. `Contents: write`
alone returns 403 for any path under `.github/workflows/**`.

Then, without letting it reach a shell history or a command line:

```sh
# local: paste after the = in the worktree's .env
FIDDLE_GITHUB_TOKEN=github_pat_...

# CI: reads from stdin, so it never enters argv
gh secret set FIDDLE_EFFECTS_TOKEN --repo peel/fiddle
```

### Verifying the scope — 403 is the evidence, not 404

A successful read proves nothing: `peel/fiddle` is public, so *any* credential
reads it. Use a permission-gated endpoint.

```sh
set -a; . ./.env; set +a
for r in peel/fiddle-effects-acceptance peel/fiddle-acceptance peel/fiddle; do
  printf '%-34s ' "$r"
  GH_TOKEN="$FIDDLE_GITHUB_TOKEN" gh api "repos/$r/collaborators" --jq 'length' 2>&1 | tail -1
done
```

Expected: `200`, then `403`, then `403`. Anything else means the selection is
wider than one repository — narrow it and re-run. Do **not** read
`.permissions` off the repository payload to check this: it reports *your* rights
as owner and says `admin=true` for a repository the token cannot write.

### Rotating

```sh
gh auth logout --hostname github.com && gh auth login    # the gh CLI's own gho_ token
# fine-grained PAT: revoke at github.com/settings/personal-access-tokens,
# mint a new one, update .env AND the FIDDLE_EFFECTS_TOKEN secret together.
```

Permissions on a fine-grained token are editable **without** re-minting, so
adding a missing grant does not invalidate the secret.

## Running the lanes

Neither gates. Both write to `peel/fiddle-effects-acceptance`, which exists to be
dirtied and holds no secrets.

```sh
# the gate: offline, credential-free, this is what must be green
scripts/gate.sh --full

# the local live lane
nix develop -c cargo build --release
FIDDLE_BIN="$PWD/target/release/fiddle" scripts/live-github.sh

# the same walk from a real runner. --ref is REQUIRED and must name a branch
# carrying the Rust workspace; see Common issues.
gh workflow run github-effects.yml --repo peel/fiddle \
  --ref ci/github-effects-dispatch-proof
```

## Common issues

**`gh workflow run` answers `HTTP 404: workflow github-effects.yml not found on the default branch`.**
The file is not on the default branch. `workflow_dispatch` resolves *entities*
there and nowhere else, so no entity exists — and `workflow_dispatch` **is** the
manual trigger, so the Actions UI button is absent too and no `--ref` works
around it. Land the file on the default branch.

**The run fails at `cargo build --release` with `could not find Cargo.toml`.**
The dispatched ref carries no Rust workspace. `actions/checkout` is bare, so the
`--ref` you passed decides which code is built, and `main` has no workspace until
the milestone stack merges. Pass a milestone branch. The lane's own preflight
refuses this by name before the build and prints the ref to use.

**The lane exits 1 naming `FIDDLE_EFFECTS_TOKEN`.** The secret is unset *or
empty* — a reference to a nonexistent secret renders as the empty string and
GitHub does not error, which is why the guard tests for emptiness. It fails rather
than skips because a silently skipped lane is indistinguishable from a passing one.

**A dispatch 403s.** `Actions: write` is missing. Adding `Workflows` does not fix
a dispatch; they are different grants.

**Residue at the disposable repository.** The lane cleans up on every exit path,
scoped to the `fiddle/` namespace. To check and clear by hand:

```sh
gh api repos/peel/fiddle-effects-acceptance/branches --jq '[.[].name]|join(", ")'
gh pr list --repo peel/fiddle-effects-acceptance --state open
gh api repos/peel/fiddle-effects-acceptance/branches --jq '.[].name' | grep '^fiddle/' \
  | xargs -r -I{} gh api "repos/peel/fiddle-effects-acceptance/git/refs/heads/{}" --method DELETE
```

Expect branches to be exactly `main` and open PRs `0`. **Closed** pull requests
are permanent — GitHub has no API that deletes one — so a non-zero closed count is
normal and is the fingerprint of past runs, not residue.

**A run exits 20 rather than 11.** That is a *permanent* refusal: a
`[github.policy]` deny, a duplicate remote state, a diverged payload, or an
unanswerable human-decision requirement. Repeating the same invocation will do the
same thing. Exit 11 is the retryable row — fix what the reason names, then re-run.

---
Last reviewed: 2026-08-10
