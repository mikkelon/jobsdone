#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
scratch="$(mktemp -d)"
busy_pid=""
trap 'if [ -n "$busy_pid" ]; then kill "$busy_pid" 2>/dev/null || true; wait "$busy_pid" 2>/dev/null || true; fi; rm -rf "$scratch"' EXIT
export HOME="$scratch/home with spaces"
export XDG_CONFIG_HOME="$HOME/config" XDG_DATA_HOME="$HOME/data" XDG_STATE_HOME="$HOME/state"
export JOBSDONE_DATA_DIR="$HOME/database" OMARCHY_PATH="$scratch/no-omarchy"
unset HYPRLAND_INSTANCE_SIGNATURE WAYLAND_DISPLAY DISPLAY JOBSDONE_TERMINAL
export PATH="$scratch/bin:$HOME/.local/bin:/usr/bin:/bin"
mkdir -p "$scratch/bin" "$scratch/downloads" "$HOME"
export JOBSDONE_TEST_DOWNLOADS="$scratch/downloads"
JOBSDONE_TEST_VERSION="v$(sed -n 's/^version = "\([^"]*\)"$/\1/p' "$root/Cargo.toml" | head -n 1)"
export JOBSDONE_TEST_VERSION
unset JOBSDONE_TEST_STABLE_VERSION JOBSDONE_TEST_BETA_VERSION
if [[ "$JOBSDONE_TEST_VERSION" != *-* ]]; then
    export JOBSDONE_TEST_STABLE_VERSION="${JOBSDONE_TEST_VERSION#v}"
fi
case "$(uname -m)" in
    x86_64) target=x86_64-unknown-linux-musl ;;
    aarch64 | arm64) target=aarch64-unknown-linux-musl ;;
    *) echo 'release tests require x86-64 or ARM64' >&2; exit 1 ;;
esac
name="jobsdone-$JOBSDONE_TEST_VERSION-$target"
if [ "$#" -gt 0 ]; then
    cp "$1" "$scratch/downloads/$name.tar.gz"
else
    mkdir -p "$scratch/package/$name/scripts" "$scratch/package/$name/assets"
    cp "$root/scripts/"{install,uninstall,launch-terminal,install-release} "$scratch/package/$name/scripts/"
    cp "$root/assets/jobsdone.svg" "$scratch/package/$name/assets/"
    printf '#!/usr/bin/env bash\nprintf "jobsdone %s\\n"\n' "${JOBSDONE_TEST_VERSION#v}" > "$scratch/package/$name/jobsdone"
    chmod +x "$scratch/package/$name/jobsdone"
    tar -czf "$scratch/downloads/$name.tar.gz" -C "$scratch/package" "$name"
fi
(cd "$scratch/downloads" && sha256sum "$name.tar.gz" > SHA256SUMS)
cat > "$scratch/bin/curl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
output=""
for ((i = 1; i <= $#; i++)); do
    if [ "${!i}" = --output ]; then j=$((i + 1)); output="${!j}"; fi
done
url="${!#}"
case "$url" in
    https://raw.githubusercontent.com/mikkelon/jobsdone/release-beta/Cargo.toml)
        printf 'version = "%s"\n' "${JOBSDONE_TEST_BETA_VERSION:-${JOBSDONE_TEST_VERSION#v}}" ;;
    https://raw.githubusercontent.com/mikkelon/jobsdone/release-stable/Cargo.toml)
        [ -n "${JOBSDONE_TEST_STABLE_VERSION:-}" ] || exit 22
        printf 'version = "%s"\n' "$JOBSDONE_TEST_STABLE_VERSION" ;;
    "https://github.com/mikkelon/jobsdone/releases/download/$JOBSDONE_TEST_VERSION/"*)
        [ "${JOBSDONE_TEST_DOWNLOAD_FAIL:-no}" != yes ] || exit 22
        cp "$JOBSDONE_TEST_DOWNLOADS/${url##*/}" "$output" ;;
    *) echo "unexpected URL: $url" >&2; exit 99 ;;
esac
EOF
cat > "$scratch/bin/cargo" <<'EOF'
#!/usr/bin/env bash
echo 'binary installation must not invoke Cargo' >&2
exit 99
EOF
cat > "$scratch/bin/pacman" <<'EOF'
#!/usr/bin/env bash
[ "${JOBSDONE_TEST_PACKAGE_INSTALLED:-no}" = yes ]
EOF
chmod +x "$scratch/bin/"*

"$root/scripts/install-release" --channel beta --no-keybind
binary="$HOME/.local/bin/jobsdone"
[ "$("$binary" --version)" = "jobsdone ${JOBSDONE_TEST_VERSION#v}" ]
test -x "$HOME/.local/bin/jobsdone-update"
test -x "$HOME/.local/bin/jobsdone-uninstall"
test -f "$XDG_DATA_HOME/applications/jobsdone.desktop"
"$HOME/.local/bin/jobsdone-update" --check | grep -F 'up to date'
"$HOME/.local/bin/jobsdone-update" | grep -F 'already up to date'
"$HOME/.local/bin/jobsdone-uninstall" --help > "$scratch/uninstall-help"
test -x "$binary"
printf '#!/usr/bin/env bash\necho "jobsdone 0.0.0"\n' > "$binary"
"$HOME/.local/bin/jobsdone-update" --channel beta
[ "$("$binary" --version)" = "jobsdone ${JOBSDONE_TEST_VERSION#v}" ]
before="$(sha256sum "$binary")"
desktop_before="$(sha256sum "$XDG_DATA_HOME/applications/jobsdone.desktop")"

cp "$binary" "$scratch/installed-binary"
if env -u JOBSDONE_TEST_STABLE_VERSION "$HOME/.local/bin/jobsdone-update" --channel stable; then
    echo 'an unavailable stable channel must fail without changing files' >&2; exit 1
fi
[ "$(sha256sum "$binary")" = "$before" ]
JOBSDONE_TEST_BETA_VERSION=0.1.0-beta.10 "$HOME/.local/bin/jobsdone-update" --check | grep -F '0.1.0-beta.10'
printf '#!/usr/bin/env bash\necho "jobsdone 0.1.0-beta.10"\n' > "$binary"
if JOBSDONE_TEST_BETA_VERSION=0.1.0-beta.2 "$HOME/.local/bin/jobsdone-update"; then
    echo 'beta updates must not downgrade beta.10 to beta.2' >&2; exit 1
fi
printf '#!/usr/bin/env bash\necho "jobsdone 0.1.0"\n' > "$binary"
if JOBSDONE_TEST_BETA_VERSION=0.1.0-beta.10 "$HOME/.local/bin/jobsdone-update" --channel beta; then
    echo 'a final release must not downgrade to its beta' >&2; exit 1
fi
JOBSDONE_TEST_STABLE_VERSION=0.1.0 "$HOME/.local/bin/jobsdone-update" | grep -F 'already up to date'
if JOBSDONE_TEST_STABLE_VERSION=0.1.0-beta.10 "$HOME/.local/bin/jobsdone-update"; then
    echo 'stable installations must reject prerelease channel contents' >&2; exit 1
fi

promotion="jobsdone-v0.1.0-$target"
mkdir -p "$scratch/promotion/$promotion/scripts" "$scratch/promotion/$promotion/assets"
cp "$root/scripts/"{install,uninstall,launch-terminal,install-release} "$scratch/promotion/$promotion/scripts/"
cp "$root/assets/jobsdone.svg" "$scratch/promotion/$promotion/assets/"
printf '#!/usr/bin/env bash\necho "jobsdone 0.1.0"\n' > "$scratch/promotion/$promotion/jobsdone"
chmod +x "$scratch/promotion/$promotion/jobsdone"
mkdir -p "$scratch/promotion-downloads"
tar -czf "$scratch/promotion-downloads/$promotion.tar.gz" -C "$scratch/promotion" "$promotion"
(cd "$scratch/promotion-downloads" && sha256sum "$promotion.tar.gz" > SHA256SUMS)
printf '#!/usr/bin/env bash\necho "jobsdone 0.1.0-beta.10"\n' > "$binary"
JOBSDONE_TEST_DOWNLOADS="$scratch/promotion-downloads" JOBSDONE_TEST_VERSION=v0.1.0 \
    "$HOME/.local/bin/jobsdone-update"
[ "$("$binary" --version)" = 'jobsdone 0.1.0' ]
JOBSDONE_TEST_STABLE_VERSION=0.1.0 "$HOME/.local/bin/jobsdone-update" | grep -F 'already up to date'
cp "$scratch/installed-binary" "$binary"

if TZDIR="$scratch/no-zoneinfo" "$HOME/.local/bin/jobsdone-update"; then
    echo 'missing time-zone data must produce an installation error' >&2; exit 1
fi
if JOBSDONE_TEST_DOWNLOAD_FAIL=yes "$HOME/.local/bin/jobsdone-update" --no-keybind; then
    echo 'failed downloads must fail installation' >&2; exit 1
fi
[ "$(sha256sum "$binary")" = "$before" ]
[ "$(sha256sum "$XDG_DATA_HOME/applications/jobsdone.desktop")" = "$desktop_before" ]
cp "$scratch/downloads/SHA256SUMS" "$scratch/checksums"
printf '%064d  %s.tar.gz\n' 0 "$name" > "$scratch/downloads/SHA256SUMS"
if "$HOME/.local/bin/jobsdone-update" --no-keybind; then
    echo 'invalid checksums must fail installation' >&2; exit 1
fi
[ "$(sha256sum "$binary")" = "$before" ]
[ "$(sha256sum "$XDG_DATA_HOME/applications/jobsdone.desktop")" = "$desktop_before" ]
cp "$scratch/checksums" "$scratch/downloads/SHA256SUMS"
if JOBSDONE_TEST_PACKAGE_INSTALLED=yes "$HOME/.local/bin/jobsdone-update"; then
    echo 'package-managed installations must not be overwritten' >&2; exit 1
fi
[ "$(sha256sum "$binary")" = "$before" ]
"$HOME/.local/bin/jobsdone-update" --version "$JOBSDONE_TEST_VERSION" --no-keybind
[ "$(sha256sum "$binary")" = "$before" ]

if [ "$#" -gt 0 ]; then
    "$binary" --help > "$scratch/help"
    "$binary" --skill > "$scratch/skill"
    test -s "$scratch/skill"
    test ! -e "$JOBSDONE_DATA_DIR/jobsdone.db"
    "$binary" task add 'Release smoke test' --day today --json > "$scratch/task"
    "$binary" task list --day today --json | grep -F 'Release smoke test'
fi

mkdir -p "$XDG_DATA_HOME/jobsdone"
printf 'preserve user data\n' > "$XDG_DATA_HOME/jobsdone/sentinel"
tar -xzf "$scratch/downloads/$name.tar.gz" -C "$scratch"
cp /usr/bin/sleep "$binary"
"$binary" 60 &
busy_pid=$!
"$scratch/$name/scripts/install" --binary "$scratch/$name/jobsdone" --no-keybind
kill -0 "$busy_pid"
[ "$("$binary" --version)" = "jobsdone ${JOBSDONE_TEST_VERSION#v}" ]
"$HOME/.local/bin/jobsdone-uninstall"
test ! -e "$binary"
test ! -e "$HOME/.local/bin/jobsdone-update"
test ! -e "$HOME/.local/bin/jobsdone-uninstall"
test ! -e "$XDG_DATA_HOME/applications/jobsdone.desktop"
test -f "$XDG_DATA_HOME/jobsdone/sentinel"
if [ "$#" -gt 0 ]; then test -f "$JOBSDONE_DATA_DIR/jobsdone.db"; fi
echo 'release installation tests passed'
