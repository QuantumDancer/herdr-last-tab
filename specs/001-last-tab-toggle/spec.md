# Feature Specification: Per-Workspace Last Tab Toggle

**Feature Branch**: `001-last-tab-toggle`

**Created**: 2026-08-02

**Status**: Draft

**Input**: User description: "This project is going to be a herdr plugin to provide a last tab feature to herdr similar to last-window in tmux. Last tabs should be remembered on a per workspace level, not cross workspace. Right now, this feature is not part of herdr. it was requested by the community and rejected by the maintainer, pointing to the plugin system (https://github.com/herdrdev/herdr/discussions/588). It should be built similarly to herdr-last-workspace (similar story https://github.com/herdrdev/herdr/discussions/665 but somebody already built a plugin for it). this plugin should be easily installable with pre-built binaries if that makes sense. it should follow strict semantic versioning. users should be able to configure the hotkey that will be used to switch between tabs."

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Jump back to the tab I was just in (Priority: P1)

A user working in a workspace moves from tab 1 to tab 3, does something there, then wants to get
back to tab 1 without hunting for it. They press their bound key and land on tab 1. They press it
again and return to tab 3. This is the tmux `last-window` reflex, applied to herdr tabs.

**Why this priority**: This is the mechanism everything else refines — without it there is nothing to
scope, bind, or ship. It is deliberately _not_ the thing that justifies the plugin: shipped alone,
this story only reproduces what `herdr-recent-navigator` already does (see Prior Art). It is the
foundation that User Story 2 turns into a reason to install this plugin instead of that one.

**Independent Test**: Open a workspace with three tabs, focus tab 1, focus tab 3, invoke the toggle
action, and confirm tab 1 is focused. Invoke it again and confirm tab 3 is focused.

**Acceptance Scenarios**:

1. **Given** a workspace where the user focused tab A then tab B, **When** the user invokes the
   toggle, **Then** tab A becomes the focused tab.
2. **Given** the toggle just moved focus from tab B to tab A, **When** the user invokes the toggle
   again, **Then** tab B becomes the focused tab.
3. **Given** a workspace where the user has focused only one tab since the plugin was installed,
   **When** the user invokes the toggle, **Then** focus does not change and no error is shown.
4. **Given** the remembered tab was renamed or moved to a different position in the tab bar,
   **When** the user invokes the toggle, **Then** focus still lands on that same tab.
5. **Given** the remembered tab has been closed, **When** the user invokes the toggle, **Then**
   focus does not change, no error is shown, and the stale target is forgotten.

---

### User Story 2 - Each workspace remembers its own last tab (Priority: P2)

A user keeps several workspaces open, one per project. They toggle between tabs in the "api"
workspace, switch to the "web" workspace, and toggle there. The "web" toggle moves between the two
tabs they last used _in "web"_ — it never jumps them into "api", and returning to "api" later finds
its own history intact.

**Why this priority**: Without this, the toggle is actively harmful in a multi-workspace setup: it
would yank the user out of their current project. It is the property that distinguishes this plugin
from a global most-recent-tab jump, and it is the behavior the user explicitly asked for. Prior Art
confirms this is not a hypothetical distinction: the one existing plugin with a last-tab action
resolves it against a global list and does exhibit the cross-workspace jump. P2 rather than P1 only
because it cannot be built or tested before the toggle exists.

**Independent Test**: With two workspaces each holding two used tabs, toggle in workspace 1, switch
to workspace 2, toggle there, and confirm each toggle stayed inside its own workspace. Return to
workspace 1 and confirm its history was not disturbed by the activity in workspace 2.

**Acceptance Scenarios**:

1. **Given** two workspaces each with their own tab history, **When** the user invokes the toggle in
   one of them, **Then** the focused workspace does not change and only a tab inside it is focused.
2. **Given** the user has been switching tabs in workspace W2, **When** they return to W1 and invoke
   the toggle, **Then** W1's toggle target is the tab they last used in W1.
3. **Given** a workspace in which the user has never switched tabs, **When** they invoke the toggle
   there, **Then** nothing happens, regardless of how much history other workspaces hold.
4. **Given** workspace W2 whose tabs have never been switched, while W1 holds a full history,
   **When** the user activates W2 — which re-reports W2's already-active tab as focused — and
   invokes the toggle there, **Then** nothing happens: activating a workspace does not seed it with
   history, and W1's history is untouched.
5. **Given** workspace W holds remembered history, **When** W is closed, **Then** W's history is
   discarded and is neither retained nor restored.

---

### User Story 3 - Bind it to the key I want (Priority: P3)

A user installs the plugin and wants it on their own chord — a tmux refugee wants `prefix+tab`,
someone else wants `cmd+shift+l`. They add a few lines to their herdr config, reload, and the key
works. Nothing about the plugin dictates which key they must use.

**Why this priority**: The action is useless until it is bound to something, and muscle memory is
personal — a fixed key would be wrong for most users. It ranks below the core behavior only because
the binding mechanism is herdr's, so this story is mostly about exposing a stable, documented action
identifier and showing users how to reach it.

**Independent Test**: Follow the README's keybinding instructions with a chosen key, reload the herdr
config, press the key, and confirm the toggle runs. Change the key to a different chord, reload, and
confirm the new key works.

**Acceptance Scenarios**:

1. **Given** the plugin is installed, **When** the user binds `quantumdancer.last-tab.toggle` to any
   key their terminal delivers to herdr and reloads the config, **Then** pressing that key performs
   the toggle.
2. **Given** the user changes their bound key, **When** they reload the config, **Then** the toggle
   responds to the new key and not the old one.
3. **Given** the user has not bound any key, **When** they list the plugin's actions, **Then**
   `quantumdancer.last-tab.toggle` is present in that listing and can be invoked by name, so it can
   be verified before binding.

---

### User Story 4 - Install without a build toolchain (Priority: P4)

A user finds the plugin, runs a single install command, and it works. They are not asked to install a
compiler first, and the install does not spend minutes compiling.

**Why this priority**: This widens who can use the plugin, but it changes nothing about what the
plugin does. The feature is complete and usable before this lands, with a build-from-source install
for users who already have the toolchain.

**Independent Test**: On a machine with no compiler toolchain installed, run the documented install
command and confirm the plugin's action is registered and works.

**Acceptance Scenarios**:

1. **Given** a supported platform with no build toolchain present, **When** the user runs the install
   command, **Then** the plugin installs successfully and its action appears in the action list.
2. **Given** a released version, **When** a user installs it, **Then** the installed artifact matches
   that release and its version is reported by the plugin listing.
3. **Given** a platform outside the target set that release declares, **When** the user attempts to
   install, **Then** they get a clear message enumerating that exact set — four targets for the first
   release — rather than a confusing build failure.
4. **Given** a user who prefers to build from source, **When** they follow the documented local
   development path — building the checkout and linking it with the documented `link` command —
   **Then** they get a working plugin without going through a release artifact.
5. **Given** a herdr older than the plugin's declared minimum version, **When** the user attempts to
   install or link the plugin, **Then** herdr refuses it and names the version requirement, rather
   than installing something that will misbehave at runtime.

---

### Edge Cases

- **No history yet in this workspace**: the very first invocation after install has nothing to go
  back to. Focus does not move, and the user sees no error.
- **Remembered tab was closed**: the target no longer exists. Focus does not move, no error, and the
  dead target is dropped so the next invocation is not stuck retrying it.
- **Remembered tab is the current tab**: history has collapsed onto itself. Focus does not move and
  the redundant target is cleared.
- **Tab moved between workspaces** (if herdr permits it): a remembered tab that is no longer in the
  workspace that remembers it is not a valid target for that workspace and is dropped.
- **Rapid switching**: the user changes tabs faster than the plugin observes the changes. History
  must never end up pointing at a tab that is neither current nor recently used; observations that no
  longer match reality are discarded rather than applied.
- **Toggle pressed twice in quick succession**: the second press may run before the first has been
  observed. This is best-effort, but it must not corrupt history or move focus somewhere the user
  never was.
- **herdr restarted**: identifiers remembered from a previous session may no longer exist, or may
  belong to different tabs. History that cannot be validated against the live session is discarded
  rather than acted on.
- **Workspace activated without a tab switch**: activating a workspace re-reports the tab that was
  already active there as focused. That observation names the tab the workspace already had as
  current, so it records no transition — a workspace the user has never switched tabs in still has
  no history afterwards, and a workspace that does have history keeps the target it had.
- **Many workspaces opened and closed over time**: remembered history must not grow without bound;
  entries for workspaces that no longer exist are pruned. A workspace can close while the plugin is
  not running, and that closure produces no event the plugin will ever see, so pruning cannot rely on
  close events alone — the remembered set is also reconciled against herdr's live workspace list.
- **No workspace or no tab is focused**: herdr answers, and its answer is that there is no current
  context. Nothing happens and no error is shown — this is a no-op, not a failure.
- **herdr is unreachable, or the runtime context it owes the plugin is absent or malformed**: the
  plugin cannot get an answer at all. This is a real failure, and it is the one case where the user
  is told something is wrong.

## Requirements _(mandatory)_

### Functional Requirements

**Core toggle**

- **FR-001**: The plugin MUST expose a single user-invokable toggle action that moves focus to the
  previously focused tab within the currently focused workspace.
- **FR-002**: The plugin MUST observe tab focus changes as they happen and maintain, per workspace,
  the currently focused tab and the one focused before it. Only a change in *which* tab is current
  inside a workspace records a transition: an observation naming the tab that workspace already had
  as current — which is what workspace activation reports — MUST leave that workspace's history
  exactly as it was, neither creating history for a workspace that had none nor overwriting the
  target of one that did.
- **FR-003**: Invoking the toggle MUST result in the previously focused tab becoming current and the
  formerly current tab becoming the toggle target, so that repeated invocations alternate between
  exactly two tabs.
- **FR-004**: The plugin MUST identify tabs by their stable herdr-assigned identifier. Tab position,
  displayed number, and label MUST NOT be used to identify a remembered tab, so reordering or
  renaming tabs MUST NOT change which tab the toggle targets.

**Per-workspace scoping**

- **FR-005**: Focus history MUST be recorded and resolved per workspace. The toggle MUST only ever
  focus a tab belonging to the workspace that is focused when it is invoked.
- **FR-006**: The toggle MUST NOT change which workspace is focused.
- **FR-007**: Tab activity in one workspace MUST NOT alter any other workspace's remembered history.
- **FR-008**: History for a workspace that no longer exists MUST be discarded. Workspace-close events
  MUST NOT be the only pruning path, because a workspace closed while the plugin is not running
  produces no event it can ever observe: the plugin MUST also reconcile its remembered workspaces
  against herdr's live workspace list before acting on history, dropping every entry naming a
  workspace herdr no longer reports.

**Robustness and quiet failure**

- **FR-009**: A no-op means the focused tab does not change and the user is told nothing. It does
  *not* mean the plugin's own history is left untouched: repairing history during a no-op — dropping
  a dead or foreign target, clearing a target that has collapsed onto the current tab, discarding an
  entry for a workspace that no longer exists — is required rather than a violation, so that the
  next invocation is not stuck on the same dead end. The plugin MUST treat every one of the
  following as such a no-op: no history for this workspace, the remembered tab no longer exists, the
  remembered tab is already the current tab, the remembered tab is not in this workspace, and herdr
  reporting that no workspace or no tab is currently focused. A no-op MUST exit successfully and
  write nothing to the error stream, because herdr surfaces a failing action as a toast — an
  unsuccessful exit on a case the user hits routinely would train them to ignore the notification
  that FR-010 depends on.
- **FR-010**: The plugin MUST report an error only when it genuinely cannot do its job: it cannot
  reach herdr at all, or the runtime context herdr owes it is absent or unparseable. A session in
  which herdr answers correctly that nothing is focused is not this case — that is the FR-009 no-op,
  and the two MUST NOT be conflated. Only this case MAY exit unsuccessfully, and when it does it MUST
  write to the error stream a message naming what was missing in terms the user can act on. The
  unsuccessful exit is what makes herdr surface the failure at all; pairing it with a bare or generic
  message would produce a toast the user cannot act on, which is no better than silence.
- **FR-011**: The plugin MUST validate an observed focus change against herdr's live state before
  recording it, and discard observations that no longer reflect reality.
- **FR-012**: The plugin MUST validate a remembered target against herdr's live state before acting
  on it, and repair its history instead of acting on a target that fails validation.
- **FR-013**: Remembered history MUST survive normal use across separate invocations, and MUST
  tolerate concurrent updates from a focus observation and a toggle invocation without corruption.
- **FR-014**: Unreadable or malformed stored history MUST be treated as no history and repaired,
  never surfaced as a crash or an error to the user.
- **FR-015**: The plugin MUST NOT write anywhere outside the storage locations herdr designates for
  it.

**Configuration and distribution**

- **FR-016**: The toggle action MUST be reachable through the identifier
  `quantumdancer.last-tab.toggle`, which users bind to a key of their choosing through herdr's own
  keybinding configuration. That exact string is the public contract — it is what the action listing
  shows, what the documented keybinding example uses, and what FR-019 protects. The plugin MUST NOT
  require or assume any particular key.
- **FR-017**: Documentation MUST include a working, copy-pasteable keybinding example that uses
  `quantumdancer.last-tab.toggle` verbatim, plus the commands to install, link, list, verify, and
  uninstall the plugin. `link` is not optional documentation: it is how the plugin is developed
  against a local checkout, and it is the path User Story 4's build-from-source scenario exercises.
- **FR-018**: Installation MUST NOT require the user to have a compiler or language toolchain
  installed on a supported platform.
- **FR-019**: Released versions MUST follow semantic versioning strictly. Any change to
  `quantumdancer.last-tab.toggle`, the keybinding contract, or observable toggle behavior that would
  break an existing user's setup MUST be a major version increase; new capability that leaves
  existing setups working MUST be a minor increase; everything else MUST be a patch increase.
- **FR-020**: Each release MUST declare the exact set of targets it was actually exercised on, and
  MUST NOT declare one it was not. Whatever that set is for a given release, it is the single list
  the install documentation covers and the unsupported-platform message enumerates, so release
  declaration, installation, and the unsupported-platform acceptance test can never disagree. The
  first release's set is the four-target matrix named in Assumptions; a later release MAY declare a
  different set, and adding a target is the minor version increase FR-019 describes.
- **FR-021**: Each release MUST declare the oldest herdr version it has actually been verified
  against, so that herdr's own compatibility check refuses to install or link the plugin onto an
  older herdr rather than letting it fail at runtime. The floor is derived from what the plugin
  depends on — every event, command, and payload field it uses — and it MUST NOT be set below a
  version the plugin has been exercised on, on the same "declare only what was exercised" principle
  FR-020 applies to platforms. The first release declares 0.7.5 — the version the Assumptions were
  verified against, and the only one this plugin has been run on. That is a *verified* floor, not a
  known-required one: herdr's plugin system dates to 0.7.0, and nothing identified so far needs
  anything newer, so the true floor is probably lower. Closing that gap is a testing exercise rather
  than a documentation change, and it is exactly the derivation this requirement asks for. Raising
  the floor is a release decision recorded with the change that needs it.

### Key Entities

- **Workspace focus history**: what the plugin remembers for one workspace — which tab is currently
  focused there and which tab was focused immediately before. Keyed by workspace identifier. The
  toggle target is the "before" tab.
- **Remembered tab reference**: a stable tab identifier plus the workspace it belongs to. Valid only
  while herdr still reports that tab as existing in that workspace.
- **Focus observation**: a notification that a tab became focused, or that a tab or workspace closed.
  Treated as a prompt to re-read live state, not as trusted data in itself.
- **Toggle action**: the single user-facing entry point, identified by
  `quantumdancer.last-tab.toggle` — the stable name users bind to a key.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: After focusing two distinct tabs in a workspace, invoking the toggle focuses the other
  one, and invoking it again returns to the first — 20 consecutive trials, 100% of them, each
  invocation issued only after the preceding focus change has been observed. (Sequences that outrun
  observation are deliberately excluded here; Scope boundaries says why.)
- **SC-002**: The toggle completes fast enough to feel instant: measured from invoking the action to
  herdr reporting the new tab as focused, the slowest of 20 consecutive invocations stays under
  150 ms. It has to be usable as a repeated reflex rather than a deliberate command. The conditions
  that move this number are fixed here: a herdr no older than the declared minimum, a workspace of 10
  tabs, history already stored for 20 workspaces, and warm invocations — the first invocation after
  install or after a herdr restart is excluded, since it pays one-time costs the reflex case does
  not. Host class and the timing harness are named in the plan rather than here; a criterion that
  pins hardware is not technology-agnostic, and the plan is where the number gets a baseline machine.
- **SC-003**: With at least three workspaces holding independent tab histories, 100% of toggle
  invocations stay within the workspace they were invoked from, and none disturbs another
  workspace's remembered history.
- **SC-004**: Every expected no-op case listed in the edge cases produces no visible error — zero
  error notifications across a session of normal use, including deliberately toggling with no
  history and with a closed target.
- **SC-005**: Reordering and renaming tabs between recording and invoking leaves the toggle target
  unchanged in 100% of trials.
- **SC-006**: A new user on a supported platform goes from discovering the plugin to a working bound
  key in under 5 minutes, following only the README, on a machine with no compiler toolchain.
- **SC-007**: Corrupting or truncating the stored history never prevents the plugin from working —
  starting from deliberately corrupted history, the next invocation is a clean no-op, and after
  focusing two *distinct* tabs in the workspace the toggle alternates between exactly those two
  again.
- **SC-008**: Stored history never outgrows the live session: whenever the plugin acts on history —
  after it has reconciled against the live workspace list, and before it resolves a target — the
  number of stored workspace entries is at most the number of workspaces herdr reports as live, and
  every stored entry names one of them. The invariant is deliberately scoped to that window rather
  than to "at all times": a workspace closed while the plugin is not running leaves a stale entry in
  the file that nothing can remove until the plugin next runs, and requiring otherwise would demand a
  cleanup mechanism outside the plugin. Checked after opening and closing 20 workspaces, including
  some closed while the plugin was not running.
- **SC-009**: A focus observation and a toggle invocation that run concurrently never leave history
  corrupt or half-written: after 100 forced overlapping pairs, stored history is readable every time
  and always names a tab that herdr reports as live in the workspace it is filed under. This is what
  makes FR-013 falsifiable rather than aspirational.
- **SC-010**: Between the end of installation and the start of uninstallation — across toggling,
  focus observation, and workspace churn — the plugin creates or modifies no file outside the state
  and configuration directories herdr designates for it, verified by watching the filesystem rather
  than by inspecting the plugin's intent. The window excludes install and uninstall on purpose:
  those are herdr's own operations on its plugin root and registry, so writes there are neither the
  plugin's doing nor a violation, and including them would make the criterion unfalsifiable. This is
  what makes FR-015 falsifiable.

## Assumptions

**Resolved against a live herdr 0.7.5 during specification** (recorded so the plan can rely on them
and re-verify if the floor moves):

- herdr emits `tab.focused`, `tab.closed`, `tab.created`, `tab.renamed`, and `tab.moved` events, plus
  the corresponding `workspace.*` events, which the plugin can subscribe to. This was the one
  assumption that could have invalidated the whole design; it is confirmed.
- herdr reports, for every workspace, which workspace is focused and which tab is active inside it,
  and reports for every tab its stable identifier and owning workspace. Per-workspace scoping is
  therefore directly supported and does not require inferring workspace membership.
- Tab identifiers are stable strings that already encode their workspace, and are distinct from the
  displayed tab number and label — which do change on reorder and rename. Keying on the identifier is
  what makes FR-004 achievable.
- A request against a tab that no longer exists fails distinguishably, so a dead target can be told
  apart from an unreachable herdr. This is what lets FR-009 and FR-010 be separated.
- Plugin actions receive the focused workspace and tab as runtime context, and herdr provides the
  plugin a dedicated durable storage location.

**Chosen defaults where the description left room**:

- "Last tab" means a strict two-tab alternation, matching tmux `last-window` — not a longer
  most-recently-used ring or a history stack. This follows directly from the stated comparison.
- The hotkey is configured through herdr's existing keybinding configuration rather than a
  plugin-specific setting, because keybindings belong to herdr's config and a plugin cannot claim a
  key on its own. "Configurable hotkey" is therefore satisfied by a stable action identifier plus
  documentation, with no plugin-side key setting to maintain.
- The supported target matrix for the first release is exactly four entries: Linux x86_64, Linux
  aarch64, macOS x86_64 (Intel), and macOS aarch64 (Apple Silicon). Both architectures are carried on
  both operating systems because ARM Linux and Intel Macs are each still ordinary developer machines,
  and a target that ships a binary costs little beyond the release matrix entry. This is the
  *first release's* set, not a permanent one: FR-020 fixes the rule that a release declares exactly
  what it exercised and reuses that list everywhere, so a later release adding a target stays
  compliant instead of contradicting this paragraph. Windows is out of scope until it can actually be
  exercised, and is therefore an unsupported platform for the purposes of that message.
- History persists across herdr restarts but is validated before use, so stale identifiers from a
  previous session degrade to a no-op rather than a wrong jump.
- The plugin ships no user-facing settings of its own in the first release. Everything configurable
  is the keybinding.
- Prebuilt binaries are worth the release machinery: the reference plugin `herdr-reviewr` already
  distributes this way successfully, and it removes the toolchain requirement that the other
  reference plugin, `herdr-last-workspace`, imposes. Building from source stays available for local
  development.

**Scope boundaries**:

- Panes are out of scope. herdr already has a last-pane behavior, and this plugin is tab-only.
- *Changing workspace focus* is out of scope — `herdr-last-workspace` already covers that, and this
  plugin never moves the user between workspaces. Workspaces themselves are very much in scope:
  workspace identity keys the history, workspace activation is an event the plugin must interpret
  correctly, and workspace lifecycle drives the pruning in FR-008. Only the focus-changing behavior
  is excluded, not the concept.
- Any interactive UI is out of scope. There is no popup, no list, no fuzzy finder — the toggle is a
  single keypress with no visible surface of its own. A user who wants to _browse_ recent tabs is
  better served by `herdr-recent-navigator`; this plugin exists for the eyes-closed reflex, and
  staying UI-less is what keeps it small enough to justify alongside that plugin.
- Recency beyond a depth of two is out of scope, including any most-recently-used ordering across
  tabs, panes, workspaces, or agents. Those are `herdr-recent-navigator`'s territory.
- Seeding history before the plugin has observed anything is out of scope; the first focus change
  after install is what starts a workspace's history.
- Guaranteeing correct ordering when focus changes faster than they can be observed is out of scope
  as a hard guarantee; the requirement is that history is never left in a wrong state, not that every
  rapid sequence produces a particular target.

## Prior Art

The maintainer's reply on discussion 588 closed the request by pointing at the plugin ecosystem,
naming `herdr-recent-navigator` as already doing last-tab. That claim was checked against the source
before planning, because if it held in full there would be no reason to build this.

**Checked 2026-08-02 against `beyondlex/herdr-recent-navigator` v0.6.2** (MIT, actively maintained).
It does expose a no-UI `beyondlex.herdr-recent-navigator.focus-previous-tab` action, bindable to any
key, distributed as prebuilt binaries covering both Linux and macOS. User Stories 1, 3, and 4 are
therefore already available to users today. Three gaps remain, and they are this plugin's entire
reason to exist:

- **It is not per-workspace scoped.** Its previous-tab handler filters its recency list by entity
  kind only and takes the second entry of a single global list; the stored entries carry a workspace
  identifier, but the handler never reads it. Because it also re-records the focused tab on every
  event — including workspace focus changes — switching workspaces leaves the previous workspace's
  tab as the toggle target. The toggle then moves the user across workspaces, which User Story 2
  exists to prevent and FR-005/FR-006 forbid — and it is why FR-002 states outright that an
  observation naming the tab a workspace already had as current records no transition.
- **It does not validate the target before acting.** A remembered tab that has since closed is passed
  straight to herdr and the resulting failure surfaces to the user. FR-009 and FR-012 require a
  silent no-op and self-repair instead.
- **It does not prune.** Recency entries are capped at a fixed count rather than reconciled against
  live workspaces, so history for workspaces that no longer exist persists. FR-008 and SC-008 require
  otherwise.

It is also pre-1.0, so it offers no stability promise on its action identifier — FR-019 commits this
plugin to one.

**Decision: proceed.** Two reasons, and the second is the durable one.

First, the gaps above are real and all three are load-bearing requirements here. Second, and
independent of whether those gaps are ever closed upstream, the two plugins are aimed at different
things. `herdr-recent-navigator` is a recency _browser_ — a fuzzy-searchable popup over workspaces,
tabs, panes, and agents — and its previous-tab action is a shortcut derived from that machinery.
Installing it to get a single keybinding means taking on a TUI, a theme system, and a recency model
that the user does not want. This plugin is the keybinding and nothing else.

The narrower alternative — contributing per-workspace scoping upstream — was considered and rejected
as the primary path. It would be a small change, but it asks another maintainer to adopt a scoping
opinion that conflicts with a deliberately global recency model, and the outcome is outside this
project's control. It stays available as a follow-up: nothing here forecloses offering the behavior
upstream later.

**What this means for planning**: the value of this plugin is concentrated in User Story 2 and
FR-009. User Story 1 alone ships a duplicate of something that already exists. A release that
implements the toggle without per-workspace scoping is not a viable first release, even though the
story ordering permits building it first.
