# Testing across Linux distributions

Use disposable Docker containers for distro-specific dependencies and terminal
versions. A headless X server inside each container lets Kitty, Alacritty, and
Ghostty receive real keyboard events and own a real X11 clipboard. It does not
exercise Omarchy's Super-key translation or Wayland; test those separately in
Foot inside the Omarchy VM using `uat/vm`. Do not drive the developer's host
desktop, keyboard focus, or clipboard for acceptance tests.

Keep acceptance logs under `.scratch/`. Use dedicated container names and do
not reuse a developer's database, display socket, clipboard, or running service
container. No privileged container, host display mount, or host network is needed.
Changing `HOME` alone does not isolate a desktop test: inherited
`HYPRLAND_INSTANCE_SIGNATURE`, `WAYLAND_DISPLAY`, and `DISPLAY` can still point
to the host session. Do not forward those variables or sockets into test
containers. Installer unit tests must clear them and stub compositor commands;
real Omarchy installation tests belong in the VM.

## Create the containers

From the repository root:

```sh
mkdir -p .scratch

docker run -d --name jobsdone-uat-arch archlinux:latest sleep infinity
docker exec jobsdone-uat-arch pacman -Syu --noconfirm --needed \
  python xorg-server-xvfb xdotool xclip kitty alacritty ghostty ttf-dejavu

docker run -d --name jobsdone-uat-ubuntu ubuntu:24.04 sleep infinity
docker exec jobsdone-uat-ubuntu apt-get update
docker exec -e DEBIAN_FRONTEND=noninteractive jobsdone-uat-ubuntu \
  apt-get install -y python3 xvfb xdotool xclip kitty alacritty \
  fonts-dejavu-core libegl1 libgl1-mesa-dri libxkbcommon-x11-0 libxcursor1
```

The extra Ubuntu X11 libraries matter: Alacritty can exit with
`NotSupported(NotSupportedError)` before opening a window when its dynamically
loaded libraries are unavailable. Use `alacritty -vv` and `ldconfig -p` to
investigate startup separately from application failures. Package availability
and versions differ between distributions; record `kitty --version`,
`alacritty --version`, and `ghostty --version` in the acceptance log.

## Install the program for runtime testing

Build once and copy the executable, installer, profiles generator, and harness:

```sh
env -u NO_COLOR cargo build

for container in jobsdone-uat-arch jobsdone-uat-ubuntu; do
  docker exec "$container" mkdir -p /work
  docker cp target/debug/jobsdone "$container":/work/jobsdone
  docker cp scripts "$container":/work/scripts
  docker cp assets "$container":/work/assets
  docker cp uat/clipboard-x11.py "$container":/work/clipboard-x11.py
  docker exec "$container" bash /work/scripts/install \
    --binary /work/jobsdone --no-keybind
done
```

This tests the executable on the target distro and exercises the installer and
launcher. It does not prove the source builds with that distro's compiler or
packages. The executable must match the container's CPU architecture and glibc.
An error such as `GLIBC_2.xx not found` means the executable needs a build against
an older compatible glibc, or a build inside the target container. Compare the
requirements with `readelf --version-info target/debug/jobsdone` and the target
with `docker exec CONTAINER ldd --version`. Ubuntu 24.04 provides glibc 2.39.

For a source-build check, copy the source into the container, install the Rust
toolchain selected for the project plus the distro's C compiler, and run
`env -u NO_COLOR make check` there. Keep the container's `target/` separate from
the host's build artifacts. A runtime test with a copied executable and a source
build answer different questions; state which was performed.

## Run real terminal clipboard tests

Start one X server per container. The `:99` displays are isolated by Docker:

```sh
for container in jobsdone-uat-arch jobsdone-uat-ubuntu; do
  docker exec -d "$container" Xvfb :99 -screen 0 1280x900x24 -ac
  docker exec -e DISPLAY=:99 \
    -e PATH=/root/.local/bin:/usr/local/bin:/usr/bin:/bin \
    "$container" python3 /work/clipboard-x11.py kitty
  docker exec -e DISPLAY=:99 \
    -e PATH=/root/.local/bin:/usr/local/bin:/usr/bin:/bin \
    "$container" python3 /work/clipboard-x11.py alacritty
done

docker exec -e DISPLAY=:99 \
  -e PATH=/root/.local/bin:/usr/local/bin:/usr/bin:/bin \
  jobsdone-uat-arch python3 /work/clipboard-x11.py ghostty
```

Run terminals sequentially within a container: each test uses that container's
keyboard focus and clipboard. Different containers may run concurrently. Use
this harness only on a disposable display. It replaces the X11 clipboard with
synthetic text and sets window focus. Every run creates a temporary Jobsdone
database, prints terminal logs on failure, and removes its scratch data on exit.

The harness selects a Jobsdone terminal profile through `JOBSDONE_TERMINAL`,
uses software rendering, waits for a visible window, and drives XTest events
through `xdotool`. Looking for a visible window matters because some terminal
versions create hidden helper windows that cannot receive keyboard focus.

A passing run verifies:

- Ctrl+Shift+C/X/V copy, cut, and paste the application's selected text.
- Ctrl+Insert, Ctrl+X, Ctrl+V, and Shift+Insert work. These are also the chords Omarchy
  sends for its universal clipboard shortcuts, but the X11 test does not run
  Omarchy's translation itself.
- Shift+arrow selection handles combining accents and wide characters.
- Multiline notes survive clipboard round trips; title fields flatten newlines.
- Pasting while a list has focus does not execute the pasted command letters.
- The expected content reaches SQLite, and Ctrl+C exits successfully.

Run the unit tests as well: the real clipboard test uses successful clipboard
operations, while unit tests cover failed writes, empty selections, unavailable
clipboard readers, and field-level behavior. `NO_COLOR` must be unset for the
color-rendering unit test: `env -u NO_COLOR make check`.

For comparison with an ordinary terminal, run `jobsdone` directly. Terminal
copy shortcuts may still target that terminal's own selection. The installed
`jobsdone-terminal` launcher supplies the application-specific key mappings;
it is part of what the clipboard acceptance test verifies.

## Clean up

Copy any desired logs out before removing the dedicated containers:

```sh
docker rm -f jobsdone-uat-arch jobsdone-uat-ubuntu
```

Removing containers leaves downloaded images cached. Do not prune unrelated
images, volumes, or containers as part of test cleanup.
