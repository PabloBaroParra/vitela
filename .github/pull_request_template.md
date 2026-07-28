<!--
The PR title becomes the squashed commit message. Use Conventional Commits:
  feat(ios): SwiftUI shell over the shared Apple viewer core
-->

## What and why

<!-- What changes, and what problem it solves. The diff already says what;
     this is where you say why. Link the issue: Closes #123 -->

## How it was verified

<!-- Be specific and be honest. "Tests pass" says nothing; name them.
     If you could not run a gate, say so — that is expected, not a failing.
     See CONTRIBUTING.md, "You can only verify your own platform". -->

```
# paste the commands you ran and their result
```

## Checklist

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings` passes
- [ ] `cargo test --workspace --locked` passes
- [ ] Behaviour changes are covered by a test that fails without the fix
- [ ] Commits follow Conventional Commits
- [ ] I can license this under both Apache-2.0 and MIT

## Gates I could not run locally

<!-- e.g. "macOS and iOS — no Mac available; relying on CI." Delete if none. -->

## Guarantees

Tick only what applies, and explain in one line if any is unticked.

- [ ] Introduces **no** network calls of any kind
- [ ] Does not drop or weaken document encryption on save
- [ ] Does not invalidate existing digital signatures
- [ ] Does not log or expose passwords, PINs or key material
- [ ] Output remains renderable in Acrobat and Preview, not only in Vitela

## Documentation

- [ ] README status table updated — only for capabilities that ship **and**
      whose tests pass
- [ ] Shell layout still follows the no-monolithic-shells rule in CLAUDE.md
- [ ] `apps/apple/Shared/` still imports only Foundation and CoreGraphics

## Anything reviewers should look at first

<!-- The part you are least sure about. Pointing at it gets you a better
     review than hoping nobody notices. -->
