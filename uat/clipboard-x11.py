#!/usr/bin/env python3
"""Exercise clipboard shortcuts in an isolated X11 terminal and scratch database."""

import argparse
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import time


def run(*args, **kwargs):
    return subprocess.run(args, check=True, timeout=10, **kwargs)


def wait_for(probe, description):
    end = time.monotonic() + 10
    while time.monotonic() < end:
        try:
            value = probe()
            if value:
                return value
        except (sqlite3.Error, subprocess.CalledProcessError, FileNotFoundError):
            pass
        time.sleep(0.1)
    raise AssertionError(description)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("terminal", choices=["kitty", "alacritty", "ghostty"])
    parser.add_argument("--launcher", default="jobsdone-terminal")
    options = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="jobsdone-clipboard-") as directory:
        data = Path(directory)
        environment = os.environ.copy()
        environment.update(JOBSDONE_TERMINAL=options.terminal,
                           JOBSDONE_DATA_DIR=directory,
                           XDG_STATE_HOME=str(data / "state"),
                           LIBGL_ALWAYS_SOFTWARE="1", GDK_BACKEND="x11")
        environment.pop("WAYLAND_DISPLAY", None)
        environment.pop("NO_COLOR", None)
        with (data / "terminal.log").open("w+") as log:
            terminal = subprocess.Popen([options.launcher], env=environment,
                                        stdout=log, stderr=log)
            try:
                def window_id():
                    result = run("xdotool", "search", "--onlyvisible", "--class", "org.omarchy.jobsdone",
                                 capture_output=True, text=True)
                    return result.stdout.splitlines()[-1]

                window = wait_for(window_id, "Jobsdone terminal did not open")
                run("xdotool", "windowfocus", "--sync", window)
                time.sleep(0.7)

                def key(chord):
                    run("xdotool", "key", "--clearmodifiers", chord)
                    time.sleep(0.15)

                def clipboard(text):
                    run("xclip", "-selection", "clipboard", "-in", input=text.encode(),
                        stdout=subprocess.DEVNULL, stderr=log)

                def copied():
                    return run("xclip", "-selection", "clipboard", "-out",
                               capture_output=True).stdout.decode()

                def row(table, column):
                    with sqlite3.connect(data / "jobsdone.db") as connection:
                        result = connection.execute(
                            f"SELECT {column} FROM {table} ORDER BY id DESC LIMIT 1"
                        ).fetchone()
                        return result[0] if result else None

                def note(expected):
                    wait_for(lambda: row("notes", "body") == expected,
                             f"Expected saved note {expected!r}")

                key("Escape")
                key("n")
                key("a")
                body = "first line\ncafe\u0301 界"
                clipboard(body)
                key("ctrl+shift+v")
                note(body)
                key("shift+Left")
                key("shift+Left")
                key("ctrl+shift+c")
                wait_for(lambda: copied() == " 界", "Ctrl+Shift+C did not copy selection")
                key("ctrl+shift+x")
                note("first line\ncafe\u0301")
                key("ctrl+shift+v")
                note(body)
                key("ctrl+a")
                key("ctrl+Insert")
                wait_for(lambda: copied() == body, "Ctrl+Insert did not copy selection")
                key("ctrl+x")
                note("")
                key("shift+Insert")
                note(body)
                key("ctrl+a")
                key("ctrl+v")
                note(body)
                key("Escape")
                key("n")
                key("a")
                clipboard("alpha\nbeta")
                key("ctrl+shift+v")
                key("Return")
                wait_for(lambda: row("tasks", "title") == "alpha beta",
                         "Multiline paste was not normalized in the title field")
                key("Escape")
                clipboard("qnx")
                key("ctrl+shift+v")
                assert terminal.poll() is None, "Paste on a list quit the app"
                note(body)
                key("ctrl+c")
                assert terminal.wait(timeout=10) == 0, "Ctrl+C did not exit cleanly"
                print(f"PASS {options.terminal}: copy, cut, paste, Unicode, multiline notes, "
                      "single-line normalization, paste outside fields, Ctrl+C quit")
            except Exception:
                log.flush()
                log.seek(0)
                print(log.read())
                raise
            finally:
                if terminal.poll() is None:
                    terminal.terminate()
                    try:
                        terminal.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        terminal.kill()
                        terminal.wait()


if __name__ == "__main__":
    main()
