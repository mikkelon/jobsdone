---
name: uat
description: Drive the built jobsdone program the way a person would and record what it showed. Use when asked to try, test, or screenshot the app in a terminal, check a screen against the wireframes or an Omarchy theme, or try the install and the Hyprland keybind on a clean Omarchy machine.
---

# Acceptance testing

Two drivers under `uat/`. Each prints its command list when run with no
arguments; that output is the reference, and `uat/README.md` is the
how-to with the layout facts (which row is the hint bar, how to stand on
another day).

| Question | Driver |
|----------|--------|
| Does the program behave and draw right? | `uat/tui`: the app in a tmux pane of a chosen size, scratch database, screen captures as text and PNG |
| Does it install and open from the keybind on a machine that has nothing? | `uat/vm`: a clean Omarchy in QEMU, reset to clean in twenty seconds |

Neither is a gate. Reach for them when the question is one they answer.

## Driving the app

1. `uat/tui start --fresh --size 120x36` builds and launches on wiped data. Every later step prints what it did, so the transcript reads on its own.
2. `uat/tui screen NAME` after each step that changes the screen. The text print is the truth about layout; the PNG is for colour, bold and dim in the current theme.
3. Judge against `wireframes/*.txt` at the same size, `docs/DESIGN.md` for colour and keys, `docs/PRODUCT.md` for behaviour. A difference from a wireframe is a finding unless a doc explains it.
   For colour across themes, `uat/themes.py uat/out/NAME.ansi` draws the capture in every installed theme on one sheet, light ones first; `--themes a,b` gives a close-up of a few.
4. `uat/tui stop` when done. `uat/MATRIX.md` lists the checks per phase if a full pass is wanted.

## The clean machine

`uat/vm` boots a throwaway copy of a sealed Omarchy install; the sealed
base never changes. Setting it up is a one-time human job (`uat/vm
install`, then `uat/vm seal`); everything after is scripted.

1. `uat/vm fresh` boots headless and returns when the desktop is up. `uat/vm status` says whether a base exists.
2. `uat/vm push` puts the working tree in the guest's `~/jobsdone`. `uat/vm ssh 'cd jobsdone && make install'` runs there with passwordless sudo, `hyprctl` and `grim` working.
3. Prove the desktop side the way a person would experience it: `uat/vm keys super-shift-j` presses the real keybind, `uat/vm ssh hyprctl clients -j` reports the window's class, floating state and size, `uat/vm shot NAME` captures the whole screen to `uat/out/vm/NAME.png`.
4. `uat/vm stop`, then `uat/vm save NAME` to keep a state worth returning to (a guest with rustup already installed saves minutes per run); `uat/vm restore NAME` boots it again.

Facts about the guest that no command confesses:

- It is clean apart from what a test machine needs: passwordless sudo, autologin kept, idle lock off, ssh allowed through ufw. No Rust toolchain, so installing one is part of what a fresh run tests.
- The guest keyboard is US layout; `type` and `keys` assume it.
- `--show` opens a QEMU window instead of running headless. The guest's resolution then follows the window, so fullscreen it for a 1080p-like screen. Headless is 1920x1080.
- The disk passphrase and guest user live in the config file under `~/.local/share/jobsdone-uat/vm`, outside the repository.
- Start a program in the guest's desktop with `omarchy-launch-tui --app-id=NAME command` under `setsid` over ssh, never `hyprctl dispatch exec`: Omarchy's hyprctl parses dispatches as Lua and rejects a bare command line.
- The guest's `/tmp` is cleared by every boot, so a helper script written there is gone after a `stop`, `restore` or `fresh`.
- The QEMU window on the host counts as a `qemu` client in the host's own Hyprland; `grim -g` on that geometry captures it when a headless screenshot is not possible.
