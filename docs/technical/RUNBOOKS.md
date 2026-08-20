# Runbooks

Fiddle is a binary and a skills library, not a service. Nothing is deployed. What
needs operating is the credentials and the two lanes that use them. Everything else
is credential-free.

## Credentials

`fiddle.toml` holds the *name* of an environment variable. A document holding a
literal secret does not load.

| variable | used by | note |
| --- | --- | --- |
| `LITELLM_API_KEY` | the real-model tiers | opt-in, costs money, never gates |
| `FIDDLE_GITHUB_TOKEN` | the local live forge lane | opt-in, writes to a disposable repository |
| `FIDDLE_EFFECTS_TOKEN` | the dispatched forge lane | the same value, as a repository secret |

Each lives in `.env` in the worktree you run from. `.envrc` is tracked, so
`dotenv_if_exists` resolves the `.env` beside it. `.env` is gitignored;
`.env.example` is the template.

### Mint the GitHub token

Use a **fine-grained** token scoped to one repository. A classic `repo` token
reaches every repository you can push to, including one whose whole argument is
that reading it needs no credential.

github.com/settings/personal-access-tokens/new

```
Resource owner        peel
Repository access     Only select repositories -> peel/fiddle-effects-acceptance
Expiration            90 days
```

Grant these five repository permissions and no others.

```
Contents          Read and write   # push the branch, read refs
Pull requests     Read and write   # look up by head/base, POST /pulls
Actions           Read and write   # POST .../dispatches, GET .../runs
Workflows         Read and write   # write files under .github/workflows/**
Metadata          Read-only        # mandatory, auto-selected
```

`Actions` dispatches a workflow and `Workflows` writes a workflow file. They are
different grants and both are needed. `Contents: write` alone returns 403 for any
path under `.github/workflows/**`.

Store it without letting it reach a shell history or a command line.

```sh
# local: paste after the = in the worktree's .env
FIDDLE_GITHUB_TOKEN=github_pat_...

# CI: reads stdin, so the value never enters argv
gh secret set FIDDLE_EFFECTS_TOKEN --repo peel/fiddle
```

### Verify the scope

A successful read proves nothing, because `peel/fiddle` is public. Use a
permission-gated endpoint and expect `200`, `403`, `403`.

```sh
set -a; . ./.env; set +a
for r in peel/fiddle-effects-acceptance peel/fiddle-acceptance peel/fiddle; do
  printf '%-34s ' "$r"
  GH_TOKEN="$FIDDLE_GITHUB_TOKEN" gh api "repos/$r/collaborators" --jq 'length' 2>&1 | tail -1
done
```

Anything else means the selection is wider than one repository. Do not read
`.permissions` off the repository payload instead: it reports *your* rights as
owner and says `admin=true` for a repository the token cannot write.

### Rotate

```sh
gh auth logout --hostname github.com && gh auth login    # the gh CLI's own token
```

For the fine-grained token: revoke it at
github.com/settings/personal-access-tokens, mint a new one, then update `.env`
**and** the `FIDDLE_EFFECTS_TOKEN` secret together. Permissions are editable
without re-minting, so adding a missing grant does not invalidate the secret.

## Run the lanes

```sh
# the gate: offline, credential-free, this is what must be green
scripts/gate.sh --full

# the local live lane
nix develop -c cargo build --release
FIDDLE_BIN="$PWD/target/release/fiddle" scripts/live-github.sh

# the same walk from a runner; --ref is required, see Common issues
gh workflow run github-effects.yml --repo peel/fiddle \
  --ref ci/github-effects-dispatch-proof
```

Read the gate's coverage off its `TOTALS` line. It says `N of M binaries`, and
`N < M` means the run stopped short, so the counts beside it are a floor.

Neither forge lane gates. Both write to `peel/fiddle-effects-acceptance`, which
exists to be dirtied and holds no secret. `docs/technical/effects-repository.md`
describes it.

## Common issues

**`HTTP 404: workflow github-effects.yml not found on the default branch`.**
The file is not on the default branch, so no workflow entity exists. Land it there;
no `--ref` works around this.

**`could not find Cargo.toml` at `cargo build --release`.** The dispatched ref
carries no Rust workspace. Pass a milestone branch. The lane's preflight refuses
this by name and prints the ref to use.

**The lane exits 1 naming `FIDDLE_EFFECTS_TOKEN`.** The secret is unset or empty. A
reference to a missing secret renders as the empty string, which is why the guard
tests for emptiness rather than existence.

**A dispatch 403s.** `Actions: write` is missing. Adding `Workflows` does not fix a
dispatch.

**Residue at the disposable repository.** Expect branches to be exactly `main` and
open pull requests `0`. Closed pull requests are permanent, so a non-zero closed
count is normal.

```sh
gh api repos/peel/fiddle-effects-acceptance/branches --jq '[.[].name]|join(", ")'
gh pr list --repo peel/fiddle-effects-acceptance --state open
gh api repos/peel/fiddle-effects-acceptance/branches --jq '.[].name' | grep '^fiddle/' \
  | xargs -r -I{} gh api "repos/peel/fiddle-effects-acceptance/git/refs/heads/{}" --method DELETE
```

**`config check` exits 2 naming `go` as an unknown field under
`[orchestration.cve]`.** M4c deleted that key and the table is strict, so a
document that loaded yesterday fails today. Delete the key. The table admits
`image`, `severities` and `max_findings` and nothing else.

**A run exits 20 rather than 11.** That is a permanent refusal: a `[github.policy]`
deny, a duplicate remote state, a diverged payload, or an unanswerable
human-decision requirement. Repeating the same invocation does the same thing. Fix
what the reason names first.

---
Last reviewed: 2026-08-20
