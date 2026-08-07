# Releasing Herdr Last Tab

A release is the only way a user ever gets this plugin, and the install path has no compiler in it —
just bytes fetched from a release and checked against a digest. So the steps below are ordered
around one idea: nothing is declared that was not exercised, and nothing is announced before it
exists.

## The number lives in three places

For a manifest `version` of `X.Y.Z`, the release tag is **`vX.Y.Z`** — the version with a single
leading `v`, and nothing else. That mapping is the contract in
[contracts/plugin-manifest.md](../specs/001-last-tab-toggle/contracts/plugin-manifest.md#the-version-to-tag-rule),
and it is load-bearing rather than cosmetic: `herdr/install.sh` builds its download URL from the
manifest version, so a tag that applies any other transformation is a 404 at install time on a
release that already looks published.

| Where | What it is | Who checks it |
| --- | --- | --- |
| `Cargo.toml`, `[package] version` | the crate version | `tests/cli.rs` asserts it equals the manifest version |
| `herdr-plugin.toml`, `version` | what herdr reports and what `install.sh` downloads | `tests/cli.rs`, and the release workflow's guard job |
| the git tag `vX.Y.Z` | what the release assets hang off | the guard job, before anything is built |

All three move in the same commit. CI cannot see the tag, which is why the guard job exists.

## The bump and the release are one operation

`herdr plugin install QuantumDancer/herdr-last-tab` **clones the default branch** at a pinned
commit. It does not check out the release tag. So `install.sh` reads the manifest version out of a
default-branch checkout and constructs `v<version>` from it — which means:

> The version named on the default branch must always be a version that is **already published**.

Between merging a version bump and publishing its release, every fresh install resolves to a tag
that does not exist yet and fails with a 404. The window cannot be eliminated — the guard job
requires the tagged commit to be an ancestor of the default branch, so the bump must be merged
before it can be tagged — but it must be minutes, not days:

1. Prepare the release PR: bump `Cargo.toml` and `herdr-plugin.toml` together, add the
   `## [X.Y.Z]` section to `CHANGELOG.md` with the date you expect to release, update the README
   if the platform set or the herdr floor moved. **Do not merge it until you are ready to tag.**
2. Merge it to the default branch. *The window opens here.*
3. Immediately tag that commit and push the tag:

   ```bash
   git switch main && git pull
   git tag v0.1.0
   git push origin v0.1.0
   ```

4. Watch the run. *The window closes when the `publish` job succeeds.*
5. Install from the published release on a host, VM, or container of **every** declared
   architecture, and fill in the [release record](#release-record) as each one lands. This is the
   one check that cannot happen any earlier: a draft's assets are not downloadable, so until step 4
   finishes there is nothing to install.
6. If the release cannot be completed — a target fails to build, or step 5 cannot record a
   platform — either fix forward at once or **revert the version bump on the default branch** so
   installs resolve to the previous published release again. Never leave the default branch naming
   a version that has no release.

## Prerequisites a repository administrator must set up

These are repository settings, not code in this repository, and they cannot be done from a pull
request. Both are recorded here so they are visible rather than assumed.

- **Immutable releases.** Enable GitHub's immutable-releases setting for this repository. The
  draft-then-publish flow already treats a published release as final; this makes it actually final,
  so a published asset cannot be swapped for different bytes after users have started installing it.
- **A ruleset restricting who may create `v*` tags.** The release workflow triggers on any `v*` tag
  push, and the guard job's ancestor check means the *code* in a release always came from the
  default branch — but without a tag ruleset, anyone with write access can start a release. Restrict
  tag creation to the maintainers who are allowed to publish.

## Before you tag

- CI is green on the exact default-branch commit you are about to tag.
- `CHANGELOG.md` has a `## [X.Y.Z]` section, with the release date. The draft job parses this file
  and **fails when the tag has no matching section**, so a forgotten entry stops the release.
- The three version copies agree.
- `platforms` in `herdr-plugin.toml`, the README's install documentation, and the target list in the
  workflow all describe the same set — see [Declaring platforms](#declaring-platforms-fr-020).
- `min_herdr_version` names a herdr this release was actually exercised against, and if it moved up,
  the `### Compatibility` entry and the README minimum moved with it — see
  [The herdr floor](#the-herdr-floor-fr-021).

## What the workflow does

`.github/workflows/release.yml`, on a `v*` tag push, in four jobs. Everything hangs off `guard` by
`needs:`, so nothing runs until it passes.

- **guard** — the tag equals `v` plus the manifest version; the tagged commit is an ancestor of the
  default branch (so a release can only contain reviewed code); no *published* release already
  exists for the tag; and it emits the target list every later job derives its asset names from.
- **draft** — creates the release as a **draft**, with its body taken from the `CHANGELOG.md`
  section for this tag.
- **build** — one leg per target, each on a native runner of that architecture. Builds with
  `--locked`, archives with a `.sha256` sidecar, uploads to the draft, attaches Sigstore build
  provenance, then **re-downloads the uploaded asset**, verifies it exactly the way `install.sh`
  will, extracts it, and runs the binary with no subcommand (which must exit 2 with the usage
  message).
- **publish** — asserts the draft carries every expected asset — four archives and four sidecars —
  and only then flips the draft to published.

A leg that fails leaves a draft sitting there for inspection and nothing installable, which is the
point of drafting first: a published release is immutable, so one published with an asset missing is
a permanent 404 for those users.

**Re-running after a failure**: delete the leftover draft first — `gh release delete vX.Y.Z` — and
then re-run the workflow. The draft job creates the release and will not reuse an existing one.

## Declaring platforms (FR-020)

A release declares exactly the targets it exercised, and that same set is what the README documents
and what herdr's unsupported-platform message enumerates. `platforms = ["linux", "macos"]` in the
manifest is a claim about **two target triples each**:

| Declared platform | Triples it covers |
| --- | --- |
| `linux` | `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl` |
| `macos` | `x86_64-apple-darwin`, `aarch64-apple-darwin` |

**The tagged manifest is what declares a platform.** `platforms` in `herdr-plugin.toml` *at the
tagged commit* is what the guard job reads, what the build matrix is derived from, and what herdr
checks before it will install — so the declaration is fixed before a single asset is built, which is
why [Before you tag](#before-you-tag) requires the manifest, the README's install documentation, and
the workflow's target list to agree at that moment. Nothing after publication adds to that set.

What arrives after publication is **evidence**, not declaration. Step 5 of
[the procedure](#the-bump-and-the-release-are-one-operation) installs each triple from the published
release and records it below, and the record then settles one of two ways:

- every triple under a declared platform is recorded — the declaration was true, and FR-020 holds
  for that release.
- one of them is not — the declaration was false. It cannot be edited, because the release is
  immutable and its manifest is fixed at its tag. It is corrected by *withdrawing* the platform in a
  new release, per [the withdrawal procedure](#release-record).

So FR-020 is enforced **retroactively** here rather than prevented, and that is worth naming as the
weakness it is rather than dressing up. The mechanism leaves no alternative: the manifest that
declares a platform is read at tag time, and the install evidence that would justify it cannot exist
until publication has already happened. What the record buys is that a false declaration is *visible
and dated* rather than silent, and that the correction has a defined shape.

Nothing is ever declared on the strength of the other triple in its pair: if one triple cannot be
exercised, the remedy is to drop the whole platform from `platforms`, from the README, and from the
release's declared set. Removing a declared platform is a **major** version change; adding one is
minor.

## The herdr floor (FR-021)

`min_herdr_version` is the oldest herdr this release has actually been *verified* against, not the
oldest it is believed to work on. herdr enforces it and refuses to install or link onto an older
host with `plugin_requires_newer_herdr`.

Raising it is a **minor** bump that carries an obligation: the release must have a `### Compatibility`
entry in `CHANGELOG.md` naming the old floor, the new floor, and the fact that the previous version
keeps working, and the README's stated minimum moves in the same change. The template is at the top
of `CHANGELOG.md`. Lowering the floor is a patch. Either way it moves with the change that needs it,
never speculatively.

## Verifying a downloaded asset

Two independent checks, and a reviewer should run both.

```bash
# 1. The digest the release advertises — the same comparison install.sh makes.
curl -fsSLO https://github.com/QuantumDancer/herdr-last-tab/releases/download/v0.1.0/herdr-last-tab-x86_64-unknown-linux-musl.tar.gz
curl -fsSLO https://github.com/QuantumDancer/herdr-last-tab/releases/download/v0.1.0/herdr-last-tab-x86_64-unknown-linux-musl.tar.gz.sha256
# Exactly one of these runs: macOS ships shasum and no sha256sum. Preferring sha256sum and falling
# back is the same choice `install.sh`, `tests/install.sh` and the release workflow each make.
if command -v sha256sum >/dev/null 2>&1; then
  sha256sum -c herdr-last-tab-x86_64-unknown-linux-musl.tar.gz.sha256
else
  shasum -a 256 -c herdr-last-tab-x86_64-unknown-linux-musl.tar.gz.sha256
fi

# 2. Sigstore build provenance: these bytes came out of this workflow, in this repository.
gh attestation verify herdr-last-tab-x86_64-unknown-linux-musl.tar.gz \
  --repo QuantumDancer/herdr-last-tab \
  --signer-workflow QuantumDancer/herdr-last-tab/.github/workflows/release.yml
```

`--signer-workflow` is not optional decoration. Without it, `gh attestation verify` accepts an
attestation produced by **any** workflow in the repository, so a workflow that could be added or
changed elsewhere would satisfy the check just as well as the release workflow. Constraining the
signer is what makes the attestation say something specific.

(The `-c` form above is safe for a reviewer working in a scratch directory, where the only file
named in the sidecar is the one just downloaded. `install.sh` deliberately does not use it: `-c`
resolves the filename recorded *inside* the sidecar against the working directory, so a same-named
file already sitting there would be verified instead of the download.)

## What the per-target smoke check is, and is not

The build job's re-download-and-run step is a **liveness check, not an integrity gate**. It proves
the published archive extracts and that the binary inside it starts on that architecture — the thing
a cross-compiled release gets wrong. It does not independently attest the bytes: the same job that
produced them re-downloads them, so it cannot detect a compromise of that job. Independent integrity
comes from the sidecar and the provenance attestation above, verified by someone else.

Two related limits worth knowing before an incident rather than during one:

- An installed binary cannot be asked which release it came from — it reports no version of its own.
  The record below is the only mapping from an installed file back to a release asset, which is why
  the digest is recorded rather than described.
- Nothing here claims reproducible builds. Rebuilding a tag is not expected to produce a
  byte-identical archive, so a digest that differs from the recorded one is not by itself evidence
  of tampering — check the attestation.

## Release record

FR-020 asks a release to declare only what it exercised, and an unrecorded run is indistinguishable
from one that never happened. For each release, record every platform the release was actually
installed on, how (physical host, VM, or container of that architecture), and the sha256 of the asset
that was installed — taken from the sidecar of the asset that install actually used.

### 0.1.0

| Target | Exercised on | Asset sha256 | Date |
| --- | --- | --- | --- |
| `x86_64-unknown-linux-musl` | VM (Proxmox guest, Linux x86_64), via `herdr plugin install QuantumDancer/herdr-last-tab` | `44ced3d88eefc1b20bd3a504f8af0e124f79f96bc8e5d0347489560865ed2250` | 2026-08-07 |
| `aarch64-unknown-linux-musl` | _not yet recorded_ | | |
| `x86_64-apple-darwin` | _not yet recorded_ | | |
| `aarch64-apple-darwin` | _not yet recorded_ | | |

These rows are filled in **after** the release is published, at step 5 of
[the procedure](#the-bump-and-the-release-are-one-operation), because a draft release exposes no
downloadable assets and so offers nothing to install. That ordering is why the `publish` job cannot
gate on this table, and why it does not claim to.

The cost of that ordering is worth stating plainly rather than leaving implied: between publication
and the last row being filled, the release already *declares* a platform whose install path nobody
has run — the build job's smoke check starts the binary but never runs `install.sh`. The declaration
is live and unconfirmed for exactly that window, which is meant to be as short as the version-bump
window above.

**Withdrawing a target.** One still reading _not yet recorded_ once step 5 is done is a target this
release declared and did not earn, and the correction is a new release rather than an edit to the
old one. The published release is immutable and its tag cannot be reused, so:

1. Drop the platform from `platforms` in `herdr-plugin.toml` and from the README. Dropping a
   declared platform is a **major** version change by the rule in
   [Declaring platforms](#declaring-platforms-fr-020).
2. Bump the version accordingly, add the `CHANGELOG.md` section, and run the whole procedure again
   from step 1 against the new tag.

The withdrawn release stays published — its assets are immutable and anyone who already installed
from it keeps working — but the default branch stops naming it, so no fresh install resolves there.
The faster remedy, when the failure is caught within minutes and the manifest can simply go back, is
step 6's revert; withdrawal by new release is what is left once the default branch has been sitting
on that version.
