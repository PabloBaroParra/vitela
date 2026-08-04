# Maintainability review prompts

`scripts/check_maintainability.py` gives reviewers small, reproducible signals
for maintained Rust, Kotlin, Swift, and C# source. The `maintainability`
workflow runs it with Python's standard library only. It emits GitHub warning
annotations for large files, decision-dense files, and repeated normalized
eight-line blocks; findings are advisory and exit successfully.

## Run locally

```sh
python3 scripts/check_maintainability.py
```

Use `--skip-duplication` while iterating on the size and control-flow signals.
The check is intentionally heuristic: it is not a measure of design quality or
semantic duplication. Reviewers decide whether a finding requires a change.

## Review an exception

When retaining a finding, add this to the PR description and, when relevant,
the commit body:

```text
Maintainability exception: <path or finding>
Scope: <the responsibility that must remain together>
Reason: <why extracting it would duplicate behaviour or obscure a boundary>
Review/removal condition: <when this must be reconsidered>
```

The author must explain why the code remains cohesive, why the work is not
shared core behaviour, and how observable behaviour was preserved and verified.
An exception does not waive human review or platform-specific tests.

## Exclusions

`.maintainabilityignore` contains root-relative POSIX glob patterns for code
that the generic heuristics cannot assess fairly: generated bindings, the FFI
boundary, third-party code, experimental code, and build output. It is not a
general suppression list.

To add or change an exclusion:

1. Keep the pattern as narrow as possible and add a comment naming the generated
   or boundary reason.
2. Include the pattern, its affected paths, and why a source-level exception is
   safer than a refactor in the PR and commit body.
3. Ask reviewers to confirm that no maintained application logic is hidden by
   the pattern and record when the exclusion should be reconsidered.

Changes to the script, workflow, or exclusion list receive repository-owner
review through `CODEOWNERS` where applicable. Do not replace this script with a
downloaded analyzer unless its version, source, and integrity are reproducibly
locked. A future proposal may add such a tool only with its lockfile or checksum
and an explicit review of its false-positive and maintenance cost.
