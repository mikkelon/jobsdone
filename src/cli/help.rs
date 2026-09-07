//! What `--help` prints, built from the same table the parser reads, so
//! that a command the program takes and a command the help lists cannot
//! drift apart.

use std::fmt::Write as _;

use super::request::{Spec, commands};

/// The lines that open the program's own help.
const OPENING: &str = "\
jobsdone, a keyboard-first daily task manager for the terminal

    jobsdone                          open the app
    jobsdone COMMAND [ARGUMENTS]      one operation, without the app
    jobsdone help [COMMAND]           what a command takes
";

/// The options every command takes, wherever they are written.
const GLOBAL: &str = "\
Options, anywhere on the line:
    --json                    the machine response instead of a reading of it
    --format text|json        the same choice, written out
    --input FILE              a JSON body for the command; - is standard input
    --data-dir DIR            a database somewhere else, ahead of JOBSDONE_DATA_DIR
    -h, --help                this, or a command's own
    -V, --version
    --skill                   the agent guide bundled with this binary
";

/// The closing lines: the little that is worth saying once rather than
/// under every command.
const CLOSING: &str = "\
Dates are 2026-09-07, today, yesterday, tomorrow or next-work-day. Today is
the working day, which begins at the hour the settings name rather than at
midnight; every response carries the one it used.

Reading changes nothing: no recurring copy is made and the morning review's
once-a-day gate is not spent. `jobsdone refresh` makes the copies that are
due and `jobsdone review start` opens the review.

Everything a change is meant to do belongs in one invocation: it commits at
once, and one `jobsdone undo apply` takes the whole of it back.

Answers go to standard output and failures to standard error. The exit code
is 0, or 2 for a command line or a request that could not be read, 3 for
something that is not there, 4 for a change the rules refused, 5 for a clash
with another window, and 1 for anything else.
";

/// Help for one command, or for the program where there is none.
pub fn for_command(spec: Option<&'static Spec>) -> String {
    let Some(spec) = spec.filter(|spec| spec.name != "help") else {
        return program();
    };
    let mut out = format!("{}\n", spec.usage);
    if !spec.detail.is_empty() {
        let _ = write!(out, "\n{}\n", spec.detail);
    }
    if spec.op.is_empty() {
        return out;
    }
    let _ = write!(
        out,
        "\nSends the {} operation. `jobsdone {} --json` gives the response\nthe fields above go into.\n",
        spec.op, spec.name
    );
    out
}

/// The whole program's help: the command list, grouped by its first word
/// in the order the table has them.
fn program() -> String {
    let mut out = String::from(OPENING);
    out.push('\n');

    let mut group = "";
    for spec in commands() {
        let noun = spec.name.split(' ').next().unwrap_or_default();
        if noun != group {
            if !group.is_empty() {
                out.push('\n');
            }
            group = noun;
        }
        let _ = writeln!(out, "    {:<20}  {}", spec.name, spec.summary);
    }

    let _ = write!(out, "\n{GLOBAL}\n{CLOSING}");
    out
}
