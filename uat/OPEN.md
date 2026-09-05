# Open after the acceptance test

What four rounds of acceptance testing left standing after three fix
rounds, for phase 13 to weigh. Everything else the rounds found is fixed
or settled in DESIGN.md and DOMAIN.md. The reports themselves are not in
the repository; the last run's are under `uat/out/reports/`.

- **The hint bar under 80 columns drops the one thing to press.** At 60
  columns a finished surfaced review step says `t today k keep space
  done esc skip` and never names Enter. The designed sizes are 120, 160
  and 80 columns; below 80 the bar should keep its primary action before
  anything else.
- **Up and Down in a wrapped note move by a line of the body**, not by
  a drawn line, so one press can jump a whole paragraph. Wrapping lives
  in `ui` and the caret in `app`.
- **Two windows editing the same note are last-writer-wins.** Deleting
  is handled; concurrent editing is not.
- **Crowded rows shorten their chips** to `[w] [↻] [due] [◷]` when the
  full text does not fit, so the date behind a chip is then only in the
  card.
- **A process killed with SIGKILL cannot restore the terminal.** Every
  exit the program makes itself does.
- **Grapheme clusters** are edited whole; nothing more is planned for
  text.
- **The day being browsed is not kept across a quit**; reopening lands on
  today.
