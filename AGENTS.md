# Working in this repository

## Where session material goes

Plans, implementation records, acceptance logs, scratch notes and anything
else that matters only to the session that wrote it go in `.scratch/`,
which is gitignored. `docs/` holds only what stays true after the work is
merged: what the program does, how it looks, its model, its stack and its
architecture. When a session's work changes one of those, edit the living
doc; the plan that led there stays in `.scratch/`.

## Running checks

Run tests with `NO_COLOR` unset: `env -u NO_COLOR cargo test`, or
`env -u NO_COLOR make check` for all CI checks. The terminal rendering
tests assert that ANSI palette colors are emitted; `NO_COLOR=1`
suppresses those colors and causes the color-output test to fail.
