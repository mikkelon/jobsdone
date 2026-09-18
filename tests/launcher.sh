#!/usr/bin/env bash
set -euo pipefail

unset HYPRLAND_INSTANCE_SIGNATURE WAYLAND_DISPLAY DISPLAY

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

fake_bin="$scratch/bin"
home_dir="$scratch/home with \"quote"
config_home="$home_dir/config with space"
data_home="$home_dir/data"
mkdir -p "$fake_bin" "$config_home/foot" "$config_home/kitty" "$config_home/alacritty" "$config_home/ghostty" "$data_home"
cat > "$fake_bin/hyprctl" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' 'launcher tests must not contact a compositor' >&2
exit 99
EOF
chmod +x "$fake_bin/hyprctl"
printf '%s\n' '[main]' 'term=xterm-256color' > "$config_home/foot/foot.ini"
printf '%s\n' 'font_size 12' > "$config_home/kitty/kitty.conf"
printf '%s\n' '[window]' 'opacity = 0.9' > "$config_home/alacritty/alacritty.toml"
printf '%s\n' 'font-size = 12' > "$config_home/ghostty/config"

cat > "$fake_bin/prebuilt" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
chmod +x "$fake_bin/prebuilt"

PATH="$fake_bin:/usr/bin:/bin" HOME="$home_dir" XDG_CONFIG_HOME="$config_home" XDG_DATA_HOME="$data_home" OMARCHY_PATH="$scratch/no-omarchy" "$root/scripts/install" --binary "$fake_bin/prebuilt" --no-keybind > "$scratch/install-output"
grep -Fq 'Run now: "$HOME/.local/bin/jobsdone"' "$scratch/install-output"
grep -Fq 'export PATH="$HOME/.local/bin:$PATH"' "$scratch/install-output"

# Check availability in isolation from terminals installed on the test host.
check_bin="$scratch/check-bin"
mkdir -p "$check_bin"
ln -s "$(command -v bash)" "$check_bin/bash"
ln -s "$(command -v dirname)" "$check_bin/dirname"
check_launcher() {
    PATH="$check_bin" HOME="$home_dir" TERMINAL="" JOBSDONE_TERMINAL="${1:-}" "$home_dir/.local/bin/jobsdone-terminal" --check
}
if check_launcher; then
    echo 'launcher check must fail when no terminal is available' >&2
    exit 1
fi
cat > "$check_bin/kitty" <<'EOF'
#!/usr/bin/env bash
echo 'availability checks must not launch a terminal' >&2
exit 99
EOF
chmod +x "$check_bin/kitty"
check_launcher
check_launcher kitty.desktop
if check_launcher foot; then
    echo 'launcher check must respect an unavailable explicit terminal' >&2
    exit 1
fi
cat > "$check_bin/xdg-terminal-exec" <<'EOF'
#!/usr/bin/env bash
[ "$1" = --print-id ] || exit 99
printf '%s' "${JOBSDONE_TEST_TERMINAL_ID:-}"
EOF
chmod +x "$check_bin/xdg-terminal-exec"
JOBSDONE_TEST_TERMINAL_ID=custom.desktop check_launcher
if check_launcher custom.desktop; then
    echo 'launcher check must fail when the terminal resolver finds nothing' >&2
    exit 1
fi

foot_profile="$config_home/jobsdone/foot.ini"
grep -Fqx "include=$config_home/foot/foot.ini" "$foot_profile"
grep -Fqx 'clipboard-copy=none' "$foot_profile"
grep -Fqx 'clipboard-paste=Shift+Insert Control+Shift+v' "$foot_profile"
grep -Fqx '\x1b[99;6u=Control+Insert Control+Shift+c' "$foot_profile"
grep -Fqx '\x1b[120;6u=Control+x Control+Shift+x' "$foot_profile"
grep -Fqx "include $config_home/kitty/kitty.conf" "$config_home/jobsdone/kitty.conf"
grep -Fqx 'map ctrl+shift+c send_text all \x1b[99;6u' "$config_home/jobsdone/kitty.conf"
grep -Fqx "general.import = [ \"$scratch/home with \\\"quote/config with space/alacritty/alacritty.toml\" ]" "$config_home/jobsdone/alacritty.toml"
grep -Fqx '{ key = "C", mods = "Control|Shift", chars = "\u001B[99;6u" },' "$config_home/jobsdone/alacritty.toml"
grep -Fqx '[keyboard]' "$config_home/jobsdone/alacritty-0.13.toml"
if grep -qE '^[[:space:]]*config-file[[:space:]]*=' "$config_home/jobsdone/ghostty.conf"; then
    echo 'Ghostty profile must not reload the automatically loaded user config' >&2
    exit 1
fi
grep -Fqx 'keybind = ctrl+shift+c=text:\x1b[99;6u' "$config_home/jobsdone/ghostty.conf"
grep -Fqx "Exec=\"$scratch/home with \\\"quote/.local/bin/jobsdone-terminal\"" "$data_home/applications/jobsdone.desktop"
grep -Fqx 'Terminal=false' "$data_home/applications/jobsdone.desktop"

cat > "$fake_bin/foot" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" > "$JOBSDONE_TEST_OUTPUT"
EOF
chmod +x "$fake_bin/foot"
output="$scratch/foot-arguments"
PATH="$fake_bin:/usr/bin:/bin" HOME="$home_dir" XDG_CONFIG_HOME="$config_home" JOBSDONE_TERMINAL=foot JOBSDONE_TEST_OUTPUT="$output" "$home_dir/.local/bin/jobsdone-terminal"
grep -Fqx -- "--config $config_home/jobsdone/foot.ini --app-id=org.omarchy.jobsdone -e $home_dir/.local/bin/jobsdone" "$output"
PATH="$fake_bin:/usr/bin:/bin" HOME="$home_dir" XDG_CONFIG_HOME="$config_home" JOBSDONE_TERMINAL=foot JOBSDONE_TEST_OUTPUT="$output" "$home_dir/.local/bin/jobsdone-terminal" --notes
grep -Fqx -- "--config $config_home/jobsdone/foot.ini --app-id=org.omarchy.jobsdone -e $home_dir/.local/bin/jobsdone --notes" "$output"

cat > "$fake_bin/kitty" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" > "$JOBSDONE_TEST_OUTPUT"
EOF
chmod +x "$fake_bin/kitty"
PATH="$fake_bin:/usr/bin:/bin" HOME="$home_dir" XDG_CONFIG_HOME="$config_home" JOBSDONE_TERMINAL=kitty JOBSDONE_TEST_OUTPUT="$output" "$home_dir/.local/bin/jobsdone-terminal"
grep -Fqx -- "--config $config_home/jobsdone/kitty.conf --class org.omarchy.jobsdone $home_dir/.local/bin/jobsdone" "$output"

cat > "$fake_bin/xdg-terminal-exec" <<'EOF'
#!/usr/bin/env bash
if [ "${1:-}" = --print-id ]; then
    printf '%s\n' kitty.desktop
fi
EOF
chmod +x "$fake_bin/xdg-terminal-exec"
PATH="$fake_bin:/usr/bin:/bin" HOME="$home_dir" XDG_CONFIG_HOME="$config_home" JOBSDONE_TEST_OUTPUT="$output" "$home_dir/.local/bin/jobsdone-terminal"
grep -Fqx -- "--config $config_home/jobsdone/kitty.conf --class org.omarchy.jobsdone $home_dir/.local/bin/jobsdone" "$output"

cat > "$fake_bin/alacritty" <<'EOF'
#!/usr/bin/env bash
if [ "${1:-}" = --version ]; then
    printf '%s\n' 'alacritty 0.13.2'
else
    printf '%s\n' "$*" > "$JOBSDONE_TEST_OUTPUT"
fi
EOF
chmod +x "$fake_bin/alacritty"
PATH="$fake_bin:/usr/bin:/bin" HOME="$home_dir" XDG_CONFIG_HOME="$config_home" JOBSDONE_TERMINAL=Alacritty.desktop JOBSDONE_TEST_OUTPUT="$output" "$home_dir/.local/bin/jobsdone-terminal"
grep -Fqx -- "--config-file $config_home/jobsdone/alacritty-0.13.toml --class org.omarchy.jobsdone -e $home_dir/.local/bin/jobsdone" "$output"

cat > "$fake_bin/ghostty" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" > "$JOBSDONE_TEST_OUTPUT"
EOF
chmod +x "$fake_bin/ghostty"
PATH="$fake_bin:/usr/bin:/bin" HOME="$home_dir" XDG_CONFIG_HOME="$config_home" JOBSDONE_TERMINAL=com.mitchellh.ghostty.desktop JOBSDONE_TEST_OUTPUT="$output" "$home_dir/.local/bin/jobsdone-terminal"
grep -Fqx -- "--config-file=$config_home/jobsdone/ghostty.conf --class=org.omarchy.jobsdone -e $home_dir/.local/bin/jobsdone" "$output"

PATH="$fake_bin:/usr/bin:/bin" HOME="$home_dir" XDG_CONFIG_HOME="$config_home" XDG_DATA_HOME="$data_home" OMARCHY_PATH="$scratch/no-omarchy" "$root/scripts/install" --binary "$home_dir/.local/bin/jobsdone" --no-keybind

mkdir -p "$scratch/omarchy"
cat > "$fake_bin/omarchy-launch-tui" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
chmod +x "$fake_bin/omarchy-launch-tui"
bindings="$config_home/hypr/bindings.lua"
launcher_lua="$scratch/home with \\\"quote/.local/bin/jobsdone-terminal"
home_bind="o.bind(\"SUPER + SHIFT + J\", \"Jobsdone\", o.shell_quote(\"$launcher_lua\"))"
notes_bind="o.bind(\"SUPER + SHIFT + N\", \"Jobsdone notes\", o.shell_quote(\"$launcher_lua\") .. \" --notes\")"
# Without a terminal on either end, so that nothing here can stop to ask.
install_on_omarchy() {
    env -u HYPRLAND_INSTANCE_SIGNATURE PATH="$fake_bin:/usr/bin:/bin" HOME="$home_dir" XDG_CONFIG_HOME="$config_home" XDG_DATA_HOME="$data_home" OMARCHY_PATH="$scratch/omarchy" \
        "$root/scripts/install" --binary "$home_dir/.local/bin/jobsdone" "$@" < /dev/null > "$scratch/install-output"
}

# Free keys are bound as they are, with nothing to unbind.
install_on_omarchy --keybind --notes-keybind
grep -Fqx "$home_bind" "$bindings"
grep -Fqx "$notes_bind" "$bindings"
if grep -Fq 'hl.unbind(' "$bindings"; then
    echo 'free keys must be bound without an unbind' >&2
    exit 1
fi

# One set of keys opens one thing.
install_on_omarchy --keybind "SUPER + ALT + K" --notes-keybind "SUPER + ALT + K"
grep -Fq 'o.bind("SUPER + ALT + K", "Jobsdone",' "$bindings"
grep -Fq 'cannot also open' "$scratch/install-output"
if grep -Fq 'Jobsdone notes' "$bindings"; then
    echo 'the notes keybind must not share the keys that open the app' >&2
    exit 1
fi
install_on_omarchy --keybind "SUPER + SHIFT + J" --no-notes-keybind

# Keys that Omarchy binds are found with Hyprland not running, and are not
# taken over without somebody to ask.
mkdir -p "$scratch/omarchy/default/hypr/bindings"
printf '%s\n' 'o.bind("SUPER + SHIFT + N", "Editor", { omarchy = "editor" })' > "$scratch/omarchy/default/hypr/bindings/applications.lua"
install_on_omarchy --notes-keybind
grep -Fq "SUPER + SHIFT + N is already bound to 'Editor'" "$scratch/install-output"
grep -Fqx "$home_bind" "$bindings"
if grep -Fq 'Jobsdone notes' "$bindings"; then
    echo 'taken keys must not be bound without asking' >&2
    exit 1
fi

# A yes at the prompt takes the keys over, and the unbind survives the
# installs that come after it.
cat > "$scratch/install-at-a-terminal" <<EOF
#!/usr/bin/env bash
exec "$root/scripts/install" --binary "\$HOME/.local/bin/jobsdone"
EOF
chmod +x "$scratch/install-at-a-terminal"
printf 'y\n' | env -u HYPRLAND_INSTANCE_SIGNATURE PATH="$fake_bin:/usr/bin:/bin" HOME="$home_dir" XDG_CONFIG_HOME="$config_home" XDG_DATA_HOME="$data_home" OMARCHY_PATH="$scratch/omarchy" \
    script -qec "$scratch/install-at-a-terminal" /dev/null > "$scratch/install-output"
grep -Fq "SUPER + SHIFT + N is already bound to 'Editor'. Overwrite 'Editor'" "$scratch/install-output"
grep -Fx -A1 'hl.unbind("SUPER + SHIFT + N")' "$bindings" | grep -Fqx "$notes_bind"
install_on_omarchy
grep -Fx -A1 'hl.unbind("SUPER + SHIFT + N")' "$bindings" | grep -Fqx "$notes_bind"
grep -Fqx 'SUPER + SHIFT + J is already bound to open Jobsdone' "$scratch/install-output"
grep -Fqx 'SUPER + SHIFT + N is already bound to open Jobsdone on the notes page' "$scratch/install-output"
if grep -Fq 'wrote the' "$scratch/install-output"; then
    echo 'an install that changes no keybind must not say it wrote one' >&2
    exit 1
fi
grep -Fqx "$home_bind" "$bindings"

# A running Hyprland is the one asked, by modmask and key.
install_on_omarchy --no-notes-keybind
live_bin="$scratch/live-bin"
mkdir -p "$live_bin"
cat > "$live_bin/hyprctl" <<'EOF'
#!/usr/bin/env bash
[ "$*" = "binds -j" ] || exit 0
printf '%s\n' '[{"modmask": 65, "key": "J", "description": "Jobsdone", "dispatcher": "__lua", "arg": "1"},
{"modmask": 65, "key": "N", "description": "Editor", "dispatcher": "__lua", "arg": "2"}]'
EOF
chmod +x "$live_bin/hyprctl"
rm "$scratch/omarchy/default/hypr/bindings/applications.lua"
HYPRLAND_INSTANCE_SIGNATURE=test PATH="$live_bin:$fake_bin:/usr/bin:/bin" HOME="$home_dir" XDG_CONFIG_HOME="$config_home" XDG_DATA_HOME="$data_home" OMARCHY_PATH="$scratch/omarchy" \
    "$root/scripts/install" --binary "$home_dir/.local/bin/jobsdone" --notes-keybind < /dev/null > "$scratch/install-output"
grep -Fq "SUPER + SHIFT + N is already bound to 'Editor'" "$scratch/install-output"
if grep -Fq 'Jobsdone notes' "$bindings"; then
    echo 'keys Hyprland reports as taken must not be bound without asking' >&2
    exit 1
fi
install_on_omarchy --notes-keybind
grep -Fqx "$notes_bind" "$bindings"

env -u HYPRLAND_INSTANCE_SIGNATURE PATH="$fake_bin:/usr/bin:/bin" HOME="$home_dir" XDG_CONFIG_HOME="$config_home" XDG_DATA_HOME="$data_home" XDG_STATE_HOME="$home_dir/state" "$root/scripts/uninstall"
test -f "$config_home/foot/foot.ini"
test ! -e "$config_home/jobsdone/foot.ini"
test ! -e "$home_dir/.local/bin/jobsdone-terminal"
if grep -Fq 'jobsdone' "$bindings"; then
    echo 'uninstall must take every Jobsdone block out of the bindings' >&2
    exit 1
fi
