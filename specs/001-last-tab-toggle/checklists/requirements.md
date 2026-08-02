# Specification Quality Checklist: Per-Workspace Last Tab Toggle

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-08-02
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs)
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
- [x] Requirements are testable and unambiguous
- [x] Success criteria are measurable
- [x] Success criteria are technology-agnostic (no implementation details)
- [x] All acceptance scenarios are defined
- [x] Edge cases are identified
- [x] Scope is clearly bounded
- [x] Dependencies and assumptions identified

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria
- [x] User scenarios cover primary flows
- [x] Feature meets measurable outcomes defined in Success Criteria
- [x] No implementation details leak into specification

## Notes

- Items marked incomplete require spec updates before `/speckit-clarify` or `/speckit-plan`
- **How "no implementation details" is read here**: as no *chosen* implementation — no language,
  framework, library, file format, storage mechanism, or internal structure. It is not read as "no
  external contract". The spec deliberately names herdr's event names, the action identifier
  `quantumdancer.last-tab.toggle`, the storage locations herdr designates, the release target matrix,
  and the declared minimum herdr version, because every one of those is a promise to a user or to
  herdr rather than a decision the plan is free to make differently. A spec that omitted them would
  leave User Story 3 and User Story 4 untestable, which is the failure this checklist exists to
  prevent. The template's wording is kept verbatim rather than edited, so this note is where the
  reading lives.

### Validation notes (iteration 4 — all items pass)

A third automated pass found five more, all of them gaps between the spec and the constitution rather
than internal contradictions — the spec described user-visible behavior where the constitution also
fixes a process contract:

- **No exit-code contract.** FR-009 and FR-010 described what the user sees and never said what the
  process returns, while the constitution reserves a non-zero exit for real failures and requires
  every no-op to exit `0` silently. Both now carry it, with the toast-fatigue reasoning that makes
  it a requirement rather than a convention.
- **`link` was missing from FR-017.** The constitution requires install, link, and uninstall to be
  documented; FR-017 listed install, list, verify, and remove. `link` is now named, and User Story
  4's build-from-source scenario exercises it rather than gesturing at "the documented local path".
- **SC-010 was unfalsifiable.** It spanned install and uninstall, which are herdr's own operations on
  its plugin root and registry — writes there are not the plugin's doing. Scoped to the window
  between them, and widened to allow the config directory the constitution also designates.
- **FR-021's floor was stated as if derived when it was observed.** 0.7.5 is the version this plugin
  was verified against, not a version anything is known to require; herdr's plugin system dates to
  0.7.0, and the reference plugins declare 0.7.0, 0.7.4, and 0.7.5 according to what each actually
  needs. FR-021 now says so, and names closing the gap as a testing exercise.
- **The "no implementation details" item needed a stated reading**, now in Notes above.

### Validation notes (iteration 3)

A second automated pass found seven more gaps, five of them created or left standing by iteration 2's
own edits. Recorded in the same shape, since the pattern — a requirement tightened in one place and
not followed through to the places that quote it — is the one this checklist keeps failing to catch:

- **FR-013 and FR-015 had no acceptance criteria.** Concurrency-without-corruption and
  storage-confinement were stated as requirements and then never made falsifiable, which is exactly
  what the "all functional requirements have clear acceptance criteria" box claims is not the case.
  SC-009 and SC-010 now cover them, both phrased as observations rather than intentions.
- **The action identifier was never named.** FR-016 and FR-019 made it a compatibility contract that
  a major version bump protects, while User Story 3 said only "the documented action identifier" —
  nothing a test could bind. It is now `quantumdancer.last-tab.toggle` everywhere it appears.
- **FR-020 froze every future release to four targets.** Iteration 2 fixed the Linux/macOS-vs-three
  contradiction by over-correcting: Assumptions called the four-target matrix a first-release
  decision while FR-020 made it permanent, so a later release adding a target would have been
  non-compliant. FR-020 now states the *rule* and Assumptions supplies the first release's instance.
- **SC-008 claimed an invariant that FR-008 cannot deliver.** "At every point" is false while the
  plugin is not running, because a workspace closed offline leaves a stale entry no one can remove
  until the next invocation. The invariant is now scoped to the window where the plugin acts.
- **No declared minimum herdr version.** The Assumptions were verified against 0.7.5 and the
  constitution requires `min_herdr_version`, but no requirement carried it. FR-021 does, with a
  User Story 4 scenario for the rejection path.
- **"Workspaces are out of scope" contradicted FR-005 through FR-008.** Only *changing workspace
  focus* is out of scope; workspace identity, activation, and lifecycle are load-bearing. Reworded.
- **SC-002's environment** was tightened as far as a technology-agnostic criterion can go — herdr
  version floor, workload, and warm-invocation exclusion. Host class and timing harness are
  deliberately left to the plan; see the response on that review thread.

### Validation notes (iteration 2)

Iteration 1 marked every item complete while six contracts were still ambiguous enough that a test
author would have had to invent them. Automated review caught all six; the spec now settles each,
so the boxes below are claims the spec can actually back:

- **Workspace activation vs. tab transition**: FR-002 now states that an observation naming the tab a
  workspace already had as current records no transition. Without that sentence, activating a
  workspace could seed or overwrite history with no user tab switch, contradicting User Story 2 —
  which now has a scenario for exactly that case.
- **No-op vs. repair**: FR-009 defines a no-op as "focus does not change, user is told nothing" and
  states explicitly that repairing history *during* a no-op is required. It previously read "changes
  nothing", which the edge cases contradicted by requiring dead targets to be dropped.
- **Offline pruning**: FR-008 now requires reconciliation against the live workspace list, because a
  workspace closed while the plugin is not running emits no event it can ever see — event-only
  pruning could not satisfy SC-008.
- **No-op vs. failure boundary**: FR-010 now separates "herdr answers, and the answer is that nothing
  is focused" (no-op) from "herdr cannot be reached, or its runtime context is absent or
  unparseable" (the one error case). Both previously read as "missing runtime context".
- **Release targets**: one four-entry matrix is named in Assumptions and referenced by FR-020, User
  Story 4, and the unsupported-platform message. The spec previously said "Linux and macOS" in one
  place and "the same three targets" in another.
- **Reproducible success criteria**: SC-001, SC-002, SC-007, and SC-008 named no workload, no
  measurement point, and no bound — "settled session", "typical session", and "proportional" are not
  things a test can fail. They now carry trial counts, a measured interval, a stated workload, an
  invariant (stored entries never exceed live workspaces), and a requirement that SC-007's two
  recovery focuses be distinct tabs, which the wording had left open.

### Validation notes (iteration 1)

- **Implementation details**: the spec names herdr concepts (workspace, tab, focus, plugin action,
  keybinding configuration) because they are the product domain, not an implementation choice. No
  language, framework, library, file format, or storage mechanism appears in any requirement. The
  herdr event and command surface appears only in the Assumptions section, correctly scoped as a
  verified external dependency rather than a design decision.
- **No clarification markers**: every gap in the feature description had a defensible default, so all
  were resolved and recorded under Assumptions rather than deferred as questions. The three that
  carried real scope weight — strict two-tab alternation vs. an MRU ring, keybinding owned by herdr
  config vs. a plugin setting, and prebuilt distribution vs. build-from-source — are each stated with
  the reasoning that selected them, so a reviewer can overturn any of them without re-deriving it.
- **Testability**: FR-009 enumerates its no-op cases exhaustively rather than saying "handles errors
  gracefully", and FR-004 names what must *not* be used for identity, which is what makes the
  reorder/rename scenarios falsifiable.
- **Open product risk, not a spec defect**: the maintainer's reply on discussion 588 names an
  existing plugin (`herdr-recent-navigator`) that already offers last-tab. The spec records this at
  the end of Assumptions and states the differentiator. This is a go/no-go input for the user, not a
  missing requirement, so it does not block planning.
