# Project Guidelines

This file contains repository-wide conventions for contributors and coding
agents. Add new project conventions here as they are established.

## Commit messages

- Use the [Conventional Commits](https://www.conventionalcommits.org/) format:
  `<type>(<optional-scope>): <description>`.
- Use a concise, imperative, lowercase description without a trailing period.
- For non-trivial changes, include a concise commit body that explains why the
  change is needed instead of merely restating the diff.
- Prefer the types `feat`, `fix`, `docs`, `refactor`, `test`, `build`, `ci`,
  `chore`, `perf`, `style`, and `revert`.
- Mark breaking changes with `!` before the colon and explain them in the commit
  body or a `BREAKING CHANGE:` footer.
