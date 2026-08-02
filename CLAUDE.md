# Herdr Last Tab

A [herdr](https://herdr.dev) plugin that toggles focus between the current tab and the previously
focused tab, scoped per workspace — the tmux `last-window` reflex for herdr tabs. A small,
single-purpose binary that herdr spawns as an action and as event hooks. There is no UI: the whole
feature is one keybinding.

Status: specification only. No source, no manifest, no build yet.

## Where things live

| What | Where |
| --- | --- |
| Project rules — binding, wins over any instruction that conflicts | `.specify/memory/constitution.md` |
| Current feature spec and its quality checklist | `specs/001-last-tab-toggle/` |
| Deferred work and incidental findings (never committed) | `SESSION.local.md` |
| Automated-review configuration | `.coderabbit.yaml` |

Read the constitution before proposing anything structural. Its four principles — herdr is the whole
API, stable identity with owned state, quiet no-ops with loud failures, and verifiable without herdr
running — are what most design questions here reduce to.

## How work happens

This repo uses [spec-kit](https://github.com/github/spec-kit). Features go
spec → clarify → plan → tasks → implement, each through its `speckit-*` skill. `.specify/` holds the
templates and scripts; don't hand-edit what those skills generate.

Two workflow rules from the constitution are easy to trip over:

- Every change lands through a pull request, reviewed by CodeRabbit before a human sees it. Every
  finding is fixed or answered — in its own thread where it has one, and otherwise in a PR-level
  comment naming the review and the finding it answers. **An agent must never merge.**
- Commit messages follow Conventional Commits, with a body explaining any non-obvious trade-off.

For working a CodeRabbit review to completion, use the `pr-review-loop` skill — it carries the
mechanics (amending into the right commit, replying in-thread, the `gh` flags that silently
misbehave) so they don't have to live here.

## Scratch space

`tmp/` is gitignored and is the right place for intermediate artifacts. Prefer it over `/tmp`.
