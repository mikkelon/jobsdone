# jobsdone

A keyboard-first daily task manager for the terminal, built around a
daily work routine: plan today, work through it, review what yesterday
left behind. It lives in a floating terminal that a keybind opens for a
moment and closes again, next to btop and lazygit.

![A morning in the app: the review, planning the day, working through it, in a floating terminal on Omarchy](assets/demo.gif)

Omarchy is the first-class home: the install wires up the window rule,
the keybind and a launcher entry, and the app takes its colours from
whatever theme the terminal is in, dark or light. Any other Linux gets
the binary and a launcher entry, and does its own windowing.

## Install

You need Rust. On Arch that is `sudo pacman -S rustup && rustup default
stable`; elsewhere, https://rustup.rs. Then:

    git clone https://github.com/mikkelon/jobsdone
    cd jobsdone
    make install

This puts the binary in `~/.local/bin`, a `Jobsdone` entry in the app
launcher, and on Omarchy a Hyprland rule that floats and centres the
window at 120 by 36 cells. If SUPER+SHIFT+J is free it offers to bind it
to the app; if the key is taken it says by what and leaves it to you.
Running `make install` again updates everything in place, and `make
uninstall` removes it all except your data.

For other keys or another window, run the script itself:

    scripts/install --keybind "SUPER + ALT + J" --size 1000x700
    scripts/install --no-keybind --tiled

The window flags are settings the app keeps, so a later install leaves
them alone, the settings page changes them without an install, and
`jobsdone desktop [--floating | --tiled] [--size WxH]` writes the
Hyprland rule again from a terminal.

On a Linux desktop that is not Omarchy the launcher entry opens the app
in the default terminal. For a floating window, add a rule for the
terminal window in your compositor's configuration; the app never sizes
or positions itself.

## Use

Press `?` inside the app for every key that works where you are. The
hint bar at the bottom always names the ones that matter most.

`,` opens the settings page: the hour the day starts, which days are work
days, how far back the review pile reaches, whether the window floats and
how big it is, and a few more. Each row says what it does and what it
holds when nobody has changed it.

`docs/PRODUCT.md` says what the program does and why, `docs/DESIGN.md`
how it looks and behaves, and `wireframes/index.html` shows every screen.

## Files

The database is at `~/.local/share/jobsdone/jobsdone.db` and is the
only thing worth backing up: the settings are in it too. The log is at
`~/.local/state/jobsdone/jobsdone.log`. There is no configuration file.

## Development

`make check` runs what CI runs: format check, clippy, tests. `make run`
starts the app against a scratch database in `.dev`, never the real
one. `uat/` holds the tools for driving the built program and for trying
the install on a clean Omarchy machine in a VM; `uat/README.md` explains
them.

## License

MIT, see `LICENSE`.
