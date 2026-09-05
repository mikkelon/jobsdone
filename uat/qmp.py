#!/usr/bin/env python3
"""Talk to a running QEMU over its QMP socket.

    qmp.py SOCKET COMMAND [JSON-ARGUMENTS]     one command, result printed as JSON
    qmp.py SOCKET type TEXT                    type a string on the virtual keyboard
    qmp.py SOCKET keys KEY...                  press key combinations, qemu key names

`type` and `keys` assume the guest uses a US keyboard layout. A key is a
qemu key name (ret, esc, spc, tab, backspace, up, down, left, right, f1,
a, 1, ...) or a combination joined with dashes: shift-j, ctrl-alt-t,
super-shift-j. Key names are listed in qemu's QKeyCode.
"""

import json
import socket
import sys
import time

SHIFTED = {
    "!": "1", "@": "2", "#": "3", "$": "4", "%": "5", "^": "6", "&": "7",
    "*": "8", "(": "9", ")": "0", "_": "minus", "+": "equal", "{": "bracket_left",
    "}": "bracket_right", "|": "backslash", ":": "semicolon", '"': "apostrophe",
    "<": "comma", ">": "dot", "?": "slash", "~": "grave_accent",
}
PLAIN = {
    " ": "spc", "\n": "ret", "\t": "tab", "-": "minus", "=": "equal",
    "[": "bracket_left", "]": "bracket_right", "\\": "backslash", ";": "semicolon",
    "'": "apostrophe", ",": "comma", ".": "dot", "/": "slash", "`": "grave_accent",
}
MODIFIERS = {"shift": "shift", "ctrl": "ctrl", "alt": "alt", "super": "meta_l", "meta": "meta_l"}


class Qmp:
    def __init__(self, path):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.connect(path)
        self.buf = b""
        self._read()  # greeting
        self.execute("qmp_capabilities")

    def _read(self):
        while b"\n" not in self.buf:
            chunk = self.sock.recv(65536)
            if not chunk:
                raise ConnectionError("qemu closed the QMP socket")
            self.buf += chunk
        line, self.buf = self.buf.split(b"\n", 1)
        return json.loads(line)

    def execute(self, command, arguments=None):
        message = {"execute": command}
        if arguments:
            message["arguments"] = arguments
        self.sock.sendall(json.dumps(message).encode() + b"\n")
        while True:
            reply = self._read()
            if "event" in reply:
                continue
            if "error" in reply:
                raise RuntimeError(f"{command}: {reply['error']['desc']}")
            return reply.get("return")

    def press(self, qcodes, hold_ms=40):
        keys = [{"type": "qcode", "data": code} for code in qcodes]
        self.execute("send-key", {"keys": keys, "hold-time": hold_ms})
        time.sleep(hold_ms / 1000 + 0.03)


def qcodes_for_char(ch):
    if ch.isascii() and ch.isalpha():
        return (["shift", ch.lower()] if ch.isupper() else [ch])
    if ch.isdigit():
        return [ch]
    if ch in PLAIN:
        return [PLAIN[ch]]
    if ch in SHIFTED:
        return ["shift", SHIFTED[ch]]
    raise ValueError(f"no key for {ch!r} on a US layout")


def qcodes_for_combo(combo):
    parts = combo.split("-") if combo != "-" else ["minus"]
    codes = []
    for part in parts:
        codes.append(MODIFIERS.get(part.lower(), part))
    return codes


def main(argv):
    if len(argv) < 3:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    path, command, rest = argv[1], argv[2], argv[3:]
    qmp = Qmp(path)
    if command == "type":
        for ch in " ".join(rest):
            qmp.press(qcodes_for_char(ch))
        return 0
    if command == "keys":
        for combo in rest:
            qmp.press(qcodes_for_combo(combo))
        return 0
    arguments = json.loads(rest[0]) if rest else None
    result = qmp.execute(command, arguments)
    if result not in (None, {}):
        print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv))
    except (RuntimeError, ValueError, ConnectionError, FileNotFoundError, ConnectionRefusedError) as error:
        print(f"qmp: {error}", file=sys.stderr)
        sys.exit(1)
