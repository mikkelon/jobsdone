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

PATH="$fake_bin:/usr/bin:/bin" HOME="$home_dir" XDG_CONFIG_HOME="$config_home" XDG_DATA_HOME="$data_home" OMARCHY_PATH="$scratch/no-omarchy" "$root/scripts/install" --binary "$fake_bin/prebuilt" --no-keybind

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
grep -Fqx "config-file = $config_home/ghostty/config" "$config_home/jobsdone/ghostty.conf"
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
env -u HYPRLAND_INSTANCE_SIGNATURE PATH="$fake_bin:/usr/bin:/bin" HOME="$home_dir" XDG_CONFIG_HOME="$config_home" XDG_DATA_HOME="$data_home" OMARCHY_PATH="$scratch/omarchy" "$root/scripts/install" --binary "$home_dir/.local/bin/jobsdone" --keybind
grep -Fqx "o.bind(\"SUPER + SHIFT + J\", \"Jobsdone\", o.shell_quote(\"$scratch/home with \\\"quote/.local/bin/jobsdone-terminal\"))" "$config_home/hypr/bindings.lua"

env -u HYPRLAND_INSTANCE_SIGNATURE PATH="$fake_bin:/usr/bin:/bin" HOME="$home_dir" XDG_CONFIG_HOME="$config_home" XDG_DATA_HOME="$data_home" XDG_STATE_HOME="$home_dir/state" "$root/scripts/uninstall"
test -f "$config_home/foot/foot.ini"
test ! -e "$config_home/jobsdone/foot.ini"
test ! -e "$home_dir/.local/bin/jobsdone-terminal"
