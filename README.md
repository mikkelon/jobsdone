# jobsdone

A keyboard-first task manager for the terminal. Plan today, work through your
list, and review what yesterday left behind.

> **Public beta.** Tested primarily on the maintainer’s devices. Feedback and
> bug reports from other setups are welcome.

![A morning in the app: the review, planning the day, working through it, in a floating terminal on Omarchy](assets/demo.gif)

- Organize tasks by day, set reminders, and schedule recurring work.
- Keep notes with optional offline spell checking.
- Work in a terminal that follows your theme, with your data stored locally.

Built for Linux, with floating windows and a keyboard shortcut on Omarchy.

## Install

For Linux on x86-64 or ARM64. No sudo is needed. Uninstalling later keeps your
tasks, notes and settings.

```sh
bash -c "$(curl -fsSL https://raw.githubusercontent.com/mikkelon/jobsdone/release-beta/scripts/install-release)" -- --channel beta
```

Open **Jobsdone** from your app launcher, or run `jobsdone` in a terminal.

The installer puts the app in `~/.local/bin`. On Omarchy, it sets up a floating
window and offers **Super+Shift+J** as a shortcut if the key is free.
If `~/.local/bin` is missing from your PATH, the installer shows how to run
Jobsdone immediately and add it to your PATH. It also checks for a terminal
for the desktop launcher and explains what to install if one is missing.

<details>
<summary>Requirements and installation options</summary>

The installer reports missing prerequisites. If the download command cannot
find `curl`, install it with your package manager and try again.

For optional clipboard support, install `wl-clipboard` on Wayland or `xclip`
on X11. The desktop launcher supports Foot, Kitty, Alacritty and Ghostty.

After installing, choose a shortcut and window size:

```sh
jobsdone-update --keybind "SUPER + ALT + J" --size 1000x700
```

Use `--no-keybind` to skip the shortcut, `--tiled` for a tiled window, or
`--version v0.1.0-beta.1` to install a particular release. Run
`jobsdone-update --help` for all options. You can also append these options to
the install command above.

On other Linux desktops, configure floating windows in your window manager.

</details>

## Get started

The hint bar shows the keys available on the current screen.

| Key | Action |
| --- | --- |
| `?` | Show help |
| `,` | Open settings |
| `n` | Open notes |
| `Ctrl+C` | Quit |

Settings let you choose your working days, when a day starts, and how far back
to review unfinished work. On Omarchy, you can also adjust the window size.

Select text with the mouse or Shift+arrow keys. Use Ctrl+Shift+C/X/V to copy,
cut and paste through the Jobsdone launcher. In an ordinary terminal, Alt+y
copies the app's selection.

## Update or uninstall

| What you want to do | Command |
| --- | --- |
| Check for an update | `jobsdone-update --check` |
| Install the latest beta | `jobsdone-update` |
| Remove the app | `jobsdone-uninstall` |

Beta installations receive beta updates. Use `jobsdone-update --channel stable`
to switch when a stable release is available.

Updates keep your tasks, notes, settings and shortcut. Restart open Jobsdone
windows after updating.

Uninstall keeps your data. Reinstall to pick up where you left off.

## Command line and agents

Manage tasks from a shell or automation:

```sh
jobsdone task add "Prepare demo" --day today --focus
jobsdone task list --day today --json
```

Run `jobsdone --help` for commands or read the [CLI guide](docs/CLI.md).
`jobsdone --skill` prints the bundled agent instructions.

## Your data

Tasks, notes and settings live in `~/.local/share/jobsdone/jobsdone.db`.
Logs are in `~/.local/state/jobsdone/jobsdone.log`. These paths follow your
XDG directory settings.

## Build from source

Install [Rust](https://rustup.rs), Git, a C compiler, make and `tzdata`, then:

```sh
git clone https://github.com/mikkelon/jobsdone
cd jobsdone
make install
```

To update a source installation, pull the desired changes and run `make install`
again. Use `make uninstall` to remove it.

## Development

| Command | Purpose |
| --- | --- |
| `make run` | Run with a separate development database |
| `env -u NO_COLOR make check` | Run formatting, lint and test checks |

[Product](docs/PRODUCT.md) · [Design](docs/DESIGN.md) ·
[Architecture](docs/ARCHITECTURE.md) · [Stack](docs/STACK.md) ·
[Desktop testing](uat/README.md) · [Releasing](docs/RELEASING.md)

## License

[MIT](LICENSE)
