# Repository workflow

`main` is protected: it accepts no direct pushes, from anyone, including the
repository owner. Every change reaches it through a pull request. The ruleset
that enforces this is versioned at
[.github/rulesets/protect-main.json](.github/rulesets/protect-main.json).

## For implementation agents

- Leave all implementation and documentation changes uncommitted in the working
  tree.
- Do not run `git commit`, `git commit --amend`, or `git push` for this
  repository.
- Preserve unrelated local changes and do not stage them for another actor.
- Run the relevant tests, formatting checks, and linters, then hand the
  uncommitted diff to Fable, Opus or Orchestrator for Code Review.

## For Fable, Opus and Orchestrator

These are the only actors that may create commits, and only after completing the
Code Review.

- **Never commit on `main`, and never push to it.** A push will be rejected by
  the ruleset; treat an attempt as a mistake, not as something to work around.
- Branch from `main` first: `feat/…`, `fix/…`, `ci/…`, `docs/…`.
- Commit with [Conventional Commits](https://www.conventionalcommits.org/),
  using the scopes listed in [CONTRIBUTING.md](CONTRIBUTING.md).
- Push the branch and open a pull request against `main`, filling in the
  template honestly — in particular, **state which gates you could not run**.
  A shell can only be type-checked on its own platform, so "macOS gate
  unverified locally, relying on CI" is the expected answer, and a far better
  one than implying a gate passed.
- **Do not merge.** The maintainer reviews and merges. Do not enable
  auto-merge, and do not merge your own pull request even when the button is
  available.
- If CI is red, report what failed and why. Do not disable, skip, or path-filter
  a check to turn it green — a gate you weakened is worse than no gate, because
  it still looks like evidence.

## Reporting results

State plainly what was verified and what was not. "Tests pass" is not a result;
name the command and paste the outcome. If a workflow never ran — because it was
skipped, cancelled, or the account was blocked — then that commit has **no CI
evidence**, and it must be described that way rather than as green.

## Architecture and maintainability protocol

### Before changing code

1. State the responsibility of the area you intend to change and the behaviour
   it owns.
2. Trace its direct dependencies and callers, including platform and FFI
   boundaries. Do not infer a shared contract from similarly named code.
3. Define the boundary of the change: what remains UI, use-case orchestration,
   I/O or FFI, and state. Keep unrelated cleanup out of the same change.

### Structure and ownership

- Keep UI rendering and event wiring separate from use cases. Use cases own
  application decisions; I/O, native APIs, and FFI remain at explicit adapters
  or boundaries; state has a single clear owner.
- Cross-platform behaviour belongs in the Rust core or an intentionally shared
  module. Do not copy it between platform shells. A platform-specific adapter is
  acceptable only when the platform API or presentation genuinely differs; say
  why in the PR.
- File and function size are review signals, not arbitrary quotas. When a unit
  is large or branch-heavy, either split by a real responsibility or justify why
  the unit is cohesive. Do not create tiny forwarding modules, one-use wrappers,
  or artificial layers solely to satisfy a metric.

### Controlled refactoring

- Refactor separately from behaviour changes whenever practical. If they must
  be together, identify the preserved behaviour and why separating the work
  would make the change less safe.
- Preserve observable behaviour, public contracts, error handling, and platform
  parity unless the change explicitly intends otherwise. Add or retain tests
  that demonstrate the preserved behaviour, then run the relevant platform and
  core gates.
- Treat maintainability findings as review prompts. Record any accepted
  exception in the PR description and commit body, with scope, reason, and the
  removal or review condition. See `docs/maintainability.md`.
