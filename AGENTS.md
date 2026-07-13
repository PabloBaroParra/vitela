# Repository workflow

- Leave all implementation and documentation changes uncommitted in the working tree.
- Do not run `git commit`, `git commit --amend`, or `git push` for this repository.
- Preserve unrelated local changes and do not stage them for another actor.
- Run the relevant tests, formatting checks, and linters, then hand the uncommitted diff to Fable for Code Review.
- Fable is the only actor that should create commits, and only after completing the Code Review.
