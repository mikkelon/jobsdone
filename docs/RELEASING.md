# Releases

`.github/workflows/release.yml` builds Linux x86-64 and ARM64 archives using
Rust 1.98.0, Cargo.lock, musl and bundled SQLite. Native runners test each target.
`scripts/package-release` checks the executable's version, architecture and
static linkage before packaging it with the installer, launcher, icon and license.
The runtime uses the system `tzdata` database for time zones. Minimal container
images need that package installed even though the executable is statically linked.

## Validate a release candidate

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
bash tests/release.sh /path/to/jobsdone-v0.1.0-x86_64-unknown-linux-musl.tar.gz
```

Run this from the matching source checkout on the archive's architecture.

## Publish

1. Update the package version in `Cargo.toml` and its entry in `Cargo.lock`.
2. Merge the validated candidate into `main`.
3. Tag that commit with `v` followed by the exact package version, and push the tag.

For example, a Cargo version of `0.1.0` uses the tag `v0.1.0`. A prerelease such
as `0.2.0-rc.1` uses `v0.2.0-rc.1` and is published as a GitHub prerelease.

The workflow reruns checks, builds both targets, verifies the downloaded build
artifacts, and assembles a draft release. It attaches both archives, `SHA256SUMS`
and `install.sh` before publishing. Generated release notes link the changes.
If publishing fails, rerun the workflow: an existing draft can be completed,
but an already published release is never overwritten.

GitHub's built-in `GITHUB_TOKEN` needs write access to repository contents only
in the publishing job. No personal access token is needed. Enable release
immutability in the repository's settings before publishing so tags and assets
are protected after publication. Public installation URLs require a public
repository and a published release; private workflow artifacts can be tested
with authenticated GitHub access.

## Installation contract

The installer resolves the latest stable tag once and downloads all assets from
that exact release. Explicit `--version` accepts a stable release or prerelease.
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
