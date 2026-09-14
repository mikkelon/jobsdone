# Releases

`.github/workflows/release.yml` builds Linux x86-64 and ARM64 archives using
Rust 1.98.0, Cargo.lock, musl and bundled SQLite. Native runners test each target.
`scripts/package-release` checks the executable's version, architecture and
static linkage before packaging it with the installer, launcher, icon and license.
The runtime uses the system `tzdata` database for time zones. Minimal container
images need that package installed even though the executable is statically linked.

## Validate a release candidate

Development checks require Python 3.11 or later and Git in addition to Rust.
Run `env -u NO_COLOR make check`. Push the candidate branch and run the **release**
workflow from GitHub Actions with that branch selected. A manual run uploads
archives and their checksums as workflow artifacts; it does not publish a release.
Packaging-related pull requests run the same build matrix.

The packaged installation tests exercise the download installer with a local
transport fixture: clean installation, upgrades, checksum and download failures,
an executable already running, version/help/skill output, SQLite task operations,
and uninstall preserving data. They use an isolated home directory and never
contact the desktop session. `tests/launcher.sh` covers Omarchy binding setup and
the terminal profiles. A clean desktop VM is the manual acceptance check for
opening the launcher and using clipboard shortcuts.

To test an archive downloaded from workflow artifacts:

```sh
bash tests/release.sh /path/to/jobsdone-v1.0.0-x86_64-unknown-linux-musl.tar.gz
```

Run this from the matching source checkout on the archive's architecture.

## Publish

1. Update the package version in `Cargo.toml` and its entry in `Cargo.lock`.
2. Merge the validated candidate into `main`.
3. Tag that commit with `v` followed by the exact package version, and push the tag.

Versions are chosen manually. Public betas use `0.1.0-beta.1`,
`0.1.0-beta.2`, and subsequent numbered betas. A substantial feature release
starts another series, such as `0.2.0-beta.1`. Tags include the leading `v`: a
Cargo version of `0.1.0-beta.1` uses `v0.1.0-beta.1` and is marked as a GitHub
prerelease.

`1.0.0` signals readiness for general use: installation and upgrades have been
tested on other people's machines, data migrations are reliable, and the core
daily workflow has held up in regular use.

The workflow reruns checks, builds both targets, verifies the downloaded build
artifacts, and assembles a draft release. It attaches both archives, `SHA256SUMS`
and `install.sh` before publishing. Generated release notes link the changes.
If publishing fails, rerun the workflow: an existing draft can be completed,
but an already published release is never overwritten. A rerun can finish
updating the installation channels after publication.

GitHub's built-in `GITHUB_TOKEN` needs write access to repository contents only
in the publishing job. No personal access token is needed. Enable release
immutability in the repository's settings before publishing so tags and assets
are protected after publication. Public installation URLs require a public
repository and a published release; private workflow artifacts can be tested
with authenticated GitHub access.

## Installation contract

After publication, `scripts/publish-channel` moves the managed `release-beta`
branch to the release's tagged commit. Stable releases also update
`release-stable`. Each branch advances only when the version is newer; a
compare-and-swap push prevents concurrent updates from overwriting one another.
These branches are installation references, not development branches. Published
tags and assets stay unchanged.

The README downloads the installer from the `release-stable` branch and passes
`--channel stable`. The installer reads that channel's Cargo version once and
downloads all assets from the matching immutable version URL. The branch is
updated only after the release's assets are published. Explicit `--version`
accepts a stable release or prerelease without consulting a channel.

Without `--channel`, a prerelease installation follows beta and a final release
follows stable. A fresh installation defaults to stable. Stable never resolves
to a prerelease, and an unavailable channel fails without falling back to the
other channel. A beta installation can advance to its corresponding stable
release and then follows stable updates. Newer betas are never downgraded to an
older stable version. `--channel beta` or `--channel stable` selects a channel
for the current invocation.
SHA-256 verification detects mismatched downloads; trust in their origin comes
from HTTPS and the GitHub release. A checksum downloaded with an archive is not
an independent signature.

The binary and executable helpers are staged alongside their destinations and
renamed into place, so a running executable does not block installation. This
is atomic replacement of each executable, not a transaction covering the whole
desktop setup. A setup failure can be retried by running the download installer.

`jobsdone-update` skips a matching installed version unless desktop options are
supplied. It refuses to replace a package-managed installation or automatically
downgrade a newer version. `--check` reports versions without writing files.
Source installations use `make install` to update and share the uninstaller.

Uninstall removes the user-local executable, helpers, desktop launcher, icon,
terminal profiles and Jobsdone's Hyprland blocks. It leaves the database and log.
Database downgrades are not guaranteed: retain a database backup before explicitly
installing an older release after running a version with schema changes.
