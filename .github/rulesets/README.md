# Branch rulesets

`protect-main.json` is the ruleset that protects `main`. It is versioned here so
the protection is reviewable and reproducible instead of living only in the
web UI, where a change leaves no trace in the repository.

GitHub does **not** read this directory automatically. Apply it by hand:

> Settings → Rules → Rulesets → New ruleset → **Import a ruleset** → upload
> `protect-main.json`

Requires the repository to be **public**, or GitHub Pro. On a private repo
without Pro the API answers:

```
403: Upgrade to GitHub Pro or make this repository public to enable this feature.
```

If you edit the ruleset in the UI, export it again and commit the result, or
this file becomes a lie.

## What it enforces, and why

| Rule | Effect |
| --- | --- |
| `pull_request` | No direct pushes to `main`. Every change arrives as a pull request — including the maintainer's. |
| `required_approving_review_count: 0` | See "the solo-maintainer trap" below. |
| `dismiss_stale_reviews_on_push` | A new push invalidates earlier approvals, so nobody approves one diff and merges another. |
| `required_review_thread_resolution` | Open review conversations block the merge. |
| `allowed_merge_methods: ["squash"]` | One commit per PR. The PR title becomes that commit message. |
| `required_linear_history` | No merge commits; the history stays readable. |
| `non_fast_forward` | Force-pushing to `main` is refused. |
| `deletion` | `main` cannot be deleted. |
| `required_status_checks` | The nine jobs below must pass. |
| `bypass_actors: []` | **Nobody** bypasses any of it, owner included. |

## The solo-maintainer trap

GitHub does not let you approve your own pull request. With a single
maintainer, setting `required_approving_review_count: 1` — or turning on
`require_code_owner_review` — means no pull request can ever be merged,
including your own. You lock yourself out of your own repository.

So approvals are set to `0`. That is not a hole: outside contributors have no
write access, so they cannot merge regardless of approvals. Their pull requests
still wait for a maintainer to press the button. What the `0` buys is that the
maintainer is not blocked on an approval that can never arrive.

`CODEOWNERS` therefore acts as automatic reviewer assignment rather than an
enforced gate. When a second maintainer joins, raise the count to `1`, set
`require_code_owner_review` to `true`, and CODEOWNERS starts enforcing.

## Why these nine status checks and no others

A workflow that is **skipped by a path filter reports no status at all** — not
success, not failure. Marking such a workflow required means any pull request
that does not touch its paths waits forever for a check that will never arrive,
and nothing can be merged.

Required here are only jobs from workflows with **no** path filter:
`core.yml`, `docs.yml` and `windows.yml`.

Deliberately **not** required, because they are path-filtered:
`ios.yml`, `macos.yml`, `android.yml`, `security.yml`. They still run and are
still visible on the pull request; they just cannot block a merge. To make one
of them blocking, add a job that always runs and reports a status even when the
real work is skipped.

`codeql.yml` is also left out: its job name is templated
(`Analyze ${{ matrix.language }}`), so the context is resolved at run time, and
it currently fails on every run because code scanning needs Advanced Security —
free only on public repositories. Add it once the repository is public and the
job is green, using the resolved name.

## `strict_required_status_checks_policy` is off

Turning it on requires every branch to be up to date with `main` before it can
merge, which re-runs all nine jobs after each merge. That is the safer setting
and the more expensive one. With a single maintainer merging serially it buys
little, so it is off. Turn it on when the project has enough concurrent pull
requests for two green branches to conflict semantically.
