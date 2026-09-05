#!/usr/bin/env python3
"""Generate the TUI wireframes as character grids.

Each screen is composed on a fixed-size grid of cells with one style
attribute per cell, then written as .txt (plain) and .html (styled, with
the theme selector). Sizes: 120x36 is an Omarchy floating terminal,
160x48 a full-width tile, 80x44 a half-width tile.
"""
import html, pathlib

OUT = pathlib.Path(__file__).parent

# ---------------------------------------------------------------- grid ---
class Grid:
    """A w×h terminal. The top and bottom rows are always left blank, so
    content coordinates run from 0 to h-3 and are offset by one row."""

    PAD = 1

    def __init__(self, w, h):
        self.w, self.h = w, h - 2 * self.PAD
        self.rows = h
        self.c = [[' '] * w for _ in range(h)]
        self.a = [[''] * w for _ in range(h)]
        self.callouts = []  # (x, y, n): rendered in HTML only

    def put(self, x, y, t, attr=''):
        if not 0 <= y < self.h:
            return x + len(t)
        y += self.PAD
        for i, ch in enumerate(t):
            if 0 <= x + i < self.w:
                self.c[y][x + i] = ch
                self.a[y][x + i] = attr
        return x + len(t)

    def rput(self, x2, y, t, attr=''):
        return self.put(x2 - len(t), y, t, attr)

    def seg(self, x, y, parts, gap=0):
        for t, a in parts:
            x = self.put(x, y, t, a) + gap
        return x

    def rseg(self, x2, y, parts, gap=0):
        total = sum(len(t) for t, _ in parts) + gap * (len(parts) - 1)
        return self.seg(x2 - total, y, parts, gap)

    def hl(self, x, y, n, attr='d', ch='─'):
        self.put(x, y, ch * n, attr)

    def vl(self, x, y, n, attr='d', ch='│'):
        for i in range(n):
            self.put(x, y + i, ch, attr)

    def add_attr(self, x, y, n, attr):
        y += self.PAD
        for i in range(n):
            if 0 <= x + i < self.w:
                self.a[y][x + i] = (self.a[y][x + i] + ' ' + attr).strip()

    def clear(self, x, y, w, h):
        for yy in range(y, y + h):
            self.put(x, yy, ' ' * w, '')

    def box(self, x, y, w, h, attr='A'):
        self.clear(x, y, w, h)
        self.put(x, y, '┌' + '─' * (w - 2) + '┐', attr)
        for yy in range(y + 1, y + h - 1):
            self.put(x, yy, '│', attr)
            self.put(x + w - 1, yy, '│', attr)
        self.put(x, y + h - 1, '└' + '─' * (w - 2) + '┘', attr)

    def callout(self, x, y, n):
        self.callouts.append((x, y, n))

    def txt(self):
        return '\n'.join(''.join(r).rstrip() for r in self.c) + '\n'

    def html_(self):
        c = [row[:] for row in self.c]
        a = [row[:] for row in self.a]
        for x, y, n in self.callouts:
            c[y + self.PAD][x], a[y + self.PAD][x] = str(n), 'n'
        out = []
        for y in range(self.rows):
            line, run, ra = [], [], None
            for x in range(self.w):
                if a[y][x] != ra and run:
                    line.append(_span(''.join(run), ra))
                    run = []
                run.append(c[y][x])
                ra = a[y][x]
            if run:
                line.append(_span(''.join(run), ra))
            out.append(''.join(line))
        return '\n'.join(out)


def _span(t, a):
    t = html.escape(t)
    return f'<span class="{a}">{t}</span>' if a else t


# ------------------------------------------------------------ elements ---
def strip(g, left, right):
    """Row 0: status line. left/right are segment lists."""
    g.seg(1, 0, left, gap=1)
    g.rseg(g.w - 1, 0, right, gap=3)
    g.hl(0, 1, g.w)


def header(g, x, w, y, title, sub='', right='', focus=False):
    g.put(x + 1, y, title, 'A b' if focus else 'b')
    if sub:
        g.put(x + 2 + len(title), y, sub, 'd')
    if right:
        g.rput(x + w - 1, y, right, 'd')


def group(g, x, w, y, label, n=None, attr='d'):
    t = label.upper() + (f' {n}' if n is not None else '')
    g.put(x + 1, y, t, attr)
    g.hl(x + 2 + len(t), y, w - 3 - len(t), 'd')


CHIP = {'due': 'Y', 'over': 'R', 'rem': 'C', 'wait': 'M', 'rep': 'd', 'ok': 'G', 'pile': 'R'}


def task(g, x, w, y, title, state='open', focus=False, cursor=False, chips=(), meta='', done_at=''):
    box = {'open': '[ ]', 'done': '[x]', 'waiting': '[ ]', 'handled': '[x]', 'moved': '[→]'}[state]
    ba = {'open': 'b' if focus else '', 'done': 'G', 'waiting': 'd', 'handled': 'd', 'moved': 'd'}[state]
    ta = 'b' if focus and state == 'open' else ('d' if state not in ('open', 'moved') else '')
    g.put(x + 1, y, box, ba)
    g.put(x + 5, y, title, ta)
    cur = x + w - 1
    if done_at:
        cur = g.rput(cur, y, done_at, 'd') - len(done_at) - 2
    for kind, text in reversed(list(chips)):
        cur = g.rput(cur, y, f'[{text}]', CHIP[kind]) - len(text) - 2 - 2
    if meta:
        g.rput(cur, y, meta, 'd')
    if cursor:
        g.add_attr(x, y, w, 'c')


def add_row(g, x, w, y, label='add a task', key='a'):
    g.put(x + 1, y, ' +  ' + label, 'd')
    g.rput(x + w - 1, y, key, 'k')


def hints(g, y, ctx, items, right=()):
    g.hl(0, y - 1, g.w)
    x = g.put(1, y, ctx.upper(), 'b') + 2
    for k, lbl in items:
        x = g.put(x, y, k, 'k')
        x = g.put(x + 1, y, lbl, 'd') + 2
    left_end = x
    rx = g.w - 1
    for k, lbl in reversed(right):
        rx = g.rput(rx, y, lbl, 'd') - len(lbl) - 1
        rx = g.rput(rx, y, k, 'k') - len(k) - 2
    return left_end


def frame2(g, left, right, focus='left', div=None):
    """Two-pane frame: headers on row 2, separators, divider. Returns (lx, lw, rx, rw, y0, y1)."""
    div = div if div is not None else g.w // 2 - 1
    lx, lw, rx, rw = 0, div, div + 1, g.w - div - 1
    header(g, lx, lw, 2, *left, focus=(focus == 'left'))
    header(g, rx, rw, 2, *right, focus=(focus == 'right'))
    g.hl(0, 3, g.w)
    g.put(div, 3, '┬', 'd')
    g.vl(div, 4, g.h - 6)
    g.put(div, g.h - 2, '┴', 'd')
    return lx, lw, rx, rw, 4, g.h - 3


def card(g, x, y, w, h, title, sub=''):
    g.box(x, y, w, h)
    g.put(x + 2, y, ' ' + title + ' ', 'A b')
    if sub:
        g.put(x + 4 + len(title), y, sub + ' ', 'd')


def item(g, x, w, y, key, label, right='', sel=False):
    g.put(x + 2, y, key, 'k')
    g.put(x + 2 + max(len(key), 5) + 1, y, label)
    if right:
        g.rput(x + w - 2, y, right, 'd')
    if sel:
        g.add_attr(x + 1, y, w - 2, 'c')


def inp(g, x, y, w, text, ph=''):
    g.put(x, y, ' ' * w, 'i')
    if text:
        g.put(x + 1, y, text, 'i')
        g.put(x + 1 + len(text), y, '▏', 'i b')
    else:
        g.put(x + 1, y, ph, 'i d')


def progress(g, x, y, w, frac):
    n = round(w * frac)
    g.put(x, y, '█' * n, 'A')
    g.put(x + n, y, '░' * (w - n), 'd')


# -------------------------------------------------------------- pages ----
PAGES = []


def page(name, title, grids, notes):
    """grids: list of (label, Grid)."""
    PAGES.append((name, title, grids, notes))


def sample_today(g, focus='left', cursor=True, div=None, wide=False):
    lx, lw, rx, rw, y0, y1 = frame2(g, ('Today', 'Fri 5 Sep', '6 open · 2 done · 1 moved'),
                                   ('Backlog', '', '12 · 3 waiting'), focus, div)
    y = y0
    group(g, lx, lw, y, 'Focus'); y += 1
    task(g, lx, lw, y, 'Ship invoice export', focus=True, chips=[('rep', '↻ every Fri')]); y += 1
    task(g, lx, lw, y, 'Reply to the tender questions', focus=True); y += 2
    group(g, lx, lw, y, 'Plan'); y += 1
    task(g, lx, lw, y, 'Fix the flaky migration test', cursor=cursor and focus == 'left', meta='←backlog'); y += 1
    task(g, lx, lw, y, 'Book dentist', chips=[('rem', '◷ today')]); y += 1
    task(g, lx, lw, y, 'Write standup notes', chips=[('rep', '↻ work days')]); y += 1
    task(g, lx, lw, y, "Review Anna's PR"); y += 1
    add_row(g, lx, lw, y); y += 2
    group(g, lx, lw, y, 'Done', 2); y += 1
    task(g, lx, lw, y, 'Morning review', state='done', done_at='08:12'); y += 1
    task(g, lx, lw, y, 'Pay electricity bill', state='done', done_at='08:30'); y += 2
    group(g, lx, lw, y, 'Moved', 1); y += 1
    task(g, lx, lw, y, 'Chase the hosting invoice', state='moved', meta='to Mon 8 Sep'); y += 1
    y = y0
    bl = [('Migrate CI to the new runners', [('due', 'due 12 Sep')]),
          ('Write the Q4 planning doc', [('due', 'due 30 Sep')]),
          ('Clean out the garage', []),
          ('Renew passport', [('rem', '◷ 1 Oct')]),
          ('Try the new keyboard layout', []),
          ('Read the Hyprland plugin docs', []),
          ('Cancel unused subscriptions', []),
          ('Sort photo backups', []),
          ('Update the household budget', [])]
    for i, (t, ch) in enumerate(bl):
        task(g, rx, rw, y, t, chips=ch, cursor=cursor and focus == 'right' and i == 1); y += 1
    add_row(g, rx, rw, y, 'add to backlog'); y += 2
    group(g, rx, rw, y, 'Waiting', 3); y += 1
    task(g, rx, rw, y, 'Quote from the electrician', state='waiting', chips=[('wait', 'waiting')]); y += 1
    task(g, rx, rw, y, 'Feedback on the proposal', state='waiting', chips=[('wait', 'waiting'), ('rem', '◷ 15 Sep')]); y += 1
    task(g, rx, rw, y, 'Parcel from the supplier', state='waiting', chips=[('wait', 'waiting')]); y += 2
    group(g, rx, rw, y, 'Repeating', 2); y += 1
    for t, r in [('Ship invoice export', 'every Friday'), ('Write standup notes', 'every work day')]:
        g.put(rx + 1, y, ' ↻ ', 'd'); g.put(rx + 5, y, t); g.rput(rx + rw - 1, y, r, 'd'); y += 1
    return lx, lw, rx, rw, y0, y1


def today_strip(g, review=2):
    strip(g, [('‹', 'd'), ('Today · Fri 5 Sep', 'b'), ('›', 'd'), ('[ ] day', 'd'), ('g go to date', 'd')],
          [(f'● {review} in review', 'R b' if review else 'd'), ('4 notes n', 'd'), ('/ search', 'd'), (': commands', 'd'), ('?', 'd')])


TODAY_HINTS = [('J/K', 'reorder'), ('space', 'done'), ('f', 'focus'), ('a', 'add'), ('e', 'edit'),
               ('b', 'to backlog'), ('m', 'move to day…'), ('R', 'repeat'), ('x', 'delete')]
BACKLOG_HINTS = [('t', 'to today'), ('m', 'move…'), ('d', 'due by'), ('r', 'remind on'), ('w', 'waiting'),
                 ('R', 'repeat'), ('a', 'add'), ('e', 'edit'), ('x', 'delete')]
PANE_KEYS = [('tab h/l', 'pane')]


# 03 --------------------------------------------------------------------
def p03():
    g = Grid(120, 36)
    today_strip(g)
    lx, lw, rx, rw, y0, y1 = sample_today(g)
    g.callout(44, 0, 1)
    g.callout(lx + 8, y0, 2)
    g.callout(lx + 8, y0 + 4, 3)
    g.callout(lx + 10, y0 + 11, 4)
    g.callout(lx + 11, y0 + 15, 5)
    g.callout(rx + 13, y0 + 11, 6)
    hx = hints(g, g.h - 1, 'Today', TODAY_HINTS, PANE_KEYS)
    g.callout(hx, g.h - 1, 7)

    w = Grid(160, 48)
    today_strip(w)
    sample_today(w)
    hints(w, w.h - 1, 'Today', TODAY_HINTS, PANE_KEYS)
    page('03-today', 'Today + backlog', [('120×36 · floating window (the default)', g), ('160×48 · full-width tile: same layout, more rows', w)], '''
<h2>Today + backlog (home)</h2>
<ol>
<li>Status line: which day, how to move between days, and the only global indicators: the review count (red when non-zero), notes, search, commands, help.</li>
<li>Focus: the few tasks that must get done today. Bold and listed first. <kbd>f</kbd> toggles.</li>
<li>Plan: the ordered working list, manual order only. The cursor row uses the terminal's selection colour so chips keep their colours.</li>
<li>Done: closed tasks drop here in the order closed, with the time. A task closed out of Focus keeps a <em>was focus</em> marker, because Focus is a property of the task, not of where the row sits.</li>
<li>Moved: a task planned for this day and then moved away leaves a pointer here, at normal weight, saying where it is now. It appears the moment the task is moved, not the next morning.</li>
<li>Backlog is one flat list; Waiting is the same list under its own heading. Chips are bracketed text in the theme's yellow, cyan and magenta, so they still read in plain grey.</li>
<li>Hint bar for the focused pane, lazygit-style. One key per action; mouse click and wheel also work.</li>
</ol>
<div class="flow"><b>Floating vs tiled</b>The floating window at 120×36 is the primary layout. The same layout runs in a full-width tile; it just shows more rows. Nothing is hidden at the small size except a few characters of row metadata.</div>
<div class="flow"><b>The same four groups on every day</b>Focus, Plan, Done and Moved, always in that order, on today and on any past or future day (screen 08). An empty group is not drawn, which is why today usually has no Moved group and a finished past day usually has no Focus group. Nothing about a day is re-arranged when it stops being today.</div>
<div class="flow"><b>Open and close at will</b>Every change is written immediately. Quitting with <kbd>q</kbd> or closing the window never asks anything, and reopening lands where you were.</div>''')


# 01 --------------------------------------------------------------------
def review_frame(g, step, subtitle, count_text, title):
    strip(g, [('MORNING REVIEW', 'b'), (f'step {step} of 2 · {subtitle}', 'd')], [(count_text, 'd'), ('esc skip for now', 'd')])
    div = g.w - 41
    lw, sx, sw = div, div + 1, g.w - div - 1
    g.hl(0, 3, g.w); g.put(div, 3, '┬', 'd'); g.vl(div, 4, g.h - 6); g.put(div, g.h - 2, '┴', 'd')
    g.put(sx + 1, 2, title.upper(), 'b')
    return 0, lw, sx, sw, 4


def p01():
    g = Grid(120, 36)
    lx, lw, sx, sw, y = review_frame(g, 1, 'the pile', '7 unfinished from past days', 'Fix the flaky migration test')
    g.callout(46, 0, 1)
    group(g, lx, lw, y, 'Yesterday · Thu 4 Sep', 3); g.callout(lx + 27, y, 2); y += 1
    task(g, lx, lw, y, 'Send the invoice to Nordic Ltd', state='handled', meta='✓ closed'); y += 1
    task(g, lx, lw, y, 'Prepare slides for Monday', state='handled', meta='→ today'); y += 1
    task(g, lx, lw, y, 'Fix the flaky migration test', focus=True, cursor=True, meta='was focus'); g.callout(lx + 34, y, 3); y += 2
    group(g, lx, lw, y, 'Mon 1 Sep', 2); y += 1
    task(g, lx, lw, y, 'Call the accountant about VAT'); y += 1
    task(g, lx, lw, y, 'Write standup notes', chips=[('rep', '↻ work days')]); y += 2
    group(g, lx, lw, y, 'Fri 22 Aug · 2 weeks ago', 1); y += 1
    task(g, lx, lw, y, 'Order new office chair'); y += 2
    group(g, lx, lw, y, 'Tue 12 Aug · 3 weeks ago', 1); y += 1
    task(g, lx, lw, y, 'Book the team dinner'); y += 1
    y = 4
    for k, lbl, d in [('d', 'Done', 'was finished'), ('t', 'Move to today', 'end of plan'), ('b', 'Back to backlog', ''),
                      ('m', 'Move to a day…', ''), ('x', 'Delete', 'undo: u')]:
        item(g, sx, sw, y, k, lbl, d); y += 1
    g.callout(sx + sw - 1, 2, 4)
    y += 1
    g.put(sx + 2, y, '2 of 7 handled', 'd'); g.callout(sx + sw - 1, y, 5); y += 1
    progress(g, sx + 2, y, sw - 4, 2 / 7); y += 1
    g.put(sx + 2, y, 'j ↑/↓ any order · u undo last', 'd')
    y = g.h - 5
    g.box(sx + 1, y, sw - 2, 3, 'd'); g.put(sx + 3, y + 1, 'Continue, 5 left on the pile ⏎'); g.callout(sx + sw - 1, y - 1, 6)
    hints(g, g.h - 1, 'Review', [('d', 'done'), ('t', 'today'), ('b', 'backlog'), ('m', 'move…'), ('x', 'delete'), ('u', 'undo'), ('e', 'edit')],
          [('⏎', 'next step'), ('esc', 'skip')])
    page('01-review', 'Morning review: the pile', [('120×36 · floating window', g)], '''
<h2>Morning review, step 1: the pile</h2>
<p>Shown once per day, on the first open of a work day, when the pile is non-empty. Full window: the review is a ritual, not a sidebar. Later opens that day go straight to Today. An empty pile skips the step.</p>
<ol>
<li>The status line becomes a step indicator. Escape (or closing the window) leaves the pile intact; the home screen then shows the red count until it is dealt with.</li>
<li>Grouped by the day the task was planned for, newest first, with relative age on old groups; the age is on the group, not repeated on every row. Nothing is carried over automatically.</li>
<li>Cursor row. Handled rows stay in place, checked and dimmed, showing what happened, so the list never jumps.</li>
<li>The outcomes from PRODUCT.md plus "move to a day". Same keys as everywhere else.</li>
<li>Progress and undo. <kbd>k</kbd> is "keep" in the review, so the cursor moves with <kbd>j</kbd> and the arrows (DESIGN.md section 4). Deletes never confirm.</li>
<li>Leaving with items still on the pile is allowed.</li>
</ol>
<div class="flow"><b>Recurring copies on the pile</b>"Write standup notes" is an unfinished copy from Monday. It is handled like any other task; the schedule keeps producing new copies regardless.</div>''')


# 02 --------------------------------------------------------------------
def p02():
    g = Grid(120, 36)
    lx, lw, sx, sw, y = review_frame(g, 2, 'due & reminders', '4 surfaced today', 'Migrate CI to the new runners')
    group(g, lx, lw, y, 'Due', 2); g.callout(lx + 9, y, 1); y += 1
    task(g, lx, lw, y, 'Migrate CI to the new runners', cursor=True, chips=[('over', 'due 3 Sep · 2 days over')]); y += 1
    task(g, lx, lw, y, 'Submit the expense report', chips=[('due', 'due today')]); y += 2
    group(g, lx, lw, y, 'Reminders', 2); g.callout(lx + 15, y, 2); y += 1
    task(g, lx, lw, y, 'Book dentist', chips=[('rem', '◷ today')]); y += 1
    task(g, lx, lw, y, 'Feedback on the proposal', state='waiting', chips=[('wait', 'waiting'), ('rem', '◷ today')]); g.callout(lx + 32, y, 3); y += 2
    group(g, lx, lw, y, 'Also starting today', 2); g.callout(lx + 25, y, 4); y += 1
    task(g, lx, lw, y, 'Ship invoice export', chips=[('rep', '↻ every Fri')], meta='on today\'s plan'); y += 1
    task(g, lx, lw, y, 'Write standup notes', chips=[('rep', '↻ work days')], meta='on today\'s plan'); y += 1
    y = 4
    for k, lbl, d in [('t', 'Pull onto today', 'to plan'), ('k', 'Keep in backlog', 'tomorrow'), ('d', 'Change due date…', ''),
                      ('w', 'Mark waiting', 'no nag'), ('space', 'Done', '')]:
        item(g, sx, sw, y, k, lbl, d); y += 1
    g.callout(sx + sw - 1, 2, 5)
    y += 1
    g.put(sx + 2, y, '0 of 4 decided', 'd'); y += 1
    progress(g, sx + 2, y, sw - 4, 0)
    y = g.h - 5
    g.box(sx + 1, y, sw - 2, 3, 'A'); g.put(sx + 3, y + 1, 'Start the day ⏎', 'A b'); g.callout(sx + sw - 1, y - 1, 6)
    hints(g, g.h - 1, 'Surfaced', [('t', 'today'), ('k', 'keep'), ('d', 'due…'), ('r', 'remind…'), ('w', 'waiting'), ('space', 'done')],
          [('⏎', 'start the day'), ('esc', 'skip')])
    page('02-surfaced', 'Morning review: surfaced', [('120×36 · floating window', g)], '''
<h2>Morning review, step 2: surfaced</h2>
<p>Backlog tasks whose date has arrived. Surfacing is a prompt, not a move: a task stays in the backlog until pulled with <kbd>t</kbd>. If nothing surfaced, the step is skipped.</p>
<ol>
<li>Due-by tasks: shown from the due date on, every morning, until closed or re-dated. Overdue ones say how far over.</li>
<li>Remind-on tasks: shown on that date only.</li>
<li>Waiting tasks are not nagged about: shown dimmed for a reminder, never in the due group.</li>
<li>Recurring copies created for today, for information only. They are already on the plan. The one place the app says what it did on its own.</li>
<li>Decisions are optional; nothing blocks starting the day.</li>
<li>Primary action ends the review and lands on Today.</li>
</ol>
<div class="flow"><b>When both steps are empty</b>The app opens straight to Today. The review is never an empty ceremony.</div>''')


# 04 --------------------------------------------------------------------
def p04():
    g = Grid(80, 44)
    strip(g, [('‹', 'd'), ('Today · Fri 5 Sep', 'b'), ('›', 'd')], [('● 2', 'R b'), ('/', 'd'), (':', 'd'), ('?', 'd')])
    x = g.put(1, 2, ' TODAY 6 ', 'A b r')
    x = g.put(x + 1, 2, ' BACKLOG 12 ', 'd')
    x = g.put(x + 1, 2, ' NOTES 4 ', 'd')
    g.callout(g.w - 2, 2, 1)
    g.hl(0, 3, g.w)
    y = 4; lx, lw = 0, g.w
    group(g, lx, lw, y, 'Focus'); y += 1
    task(g, lx, lw, y, 'Ship invoice export', focus=True, chips=[('rep', '↻')]); y += 1
    task(g, lx, lw, y, 'Reply to the tender questions', focus=True); y += 2
    group(g, lx, lw, y, 'Plan'); y += 1
    task(g, lx, lw, y, 'Fix the flaky migration test'); y += 1
    task(g, lx, lw, y, 'Book dentist', cursor=True, chips=[('rem', '◷')]); g.callout(g.w - 8, y, 2); y += 1
    task(g, lx, lw, y, 'Write standup notes', chips=[('rep', '↻')]); y += 1
    task(g, lx, lw, y, "Review Anna's PR"); y += 1
    add_row(g, lx, lw, y); y += 2
    group(g, lx, lw, y, 'Done', 2); y += 1
    task(g, lx, lw, y, 'Morning review', state='done'); y += 1
    task(g, lx, lw, y, 'Pay electricity bill', state='done'); y += 2
    group(g, lx, lw, y, 'Moved', 1); y += 1
    task(g, lx, lw, y, 'Chase the hosting invoice', state='moved', meta='to Mon 8 Sep'); y += 1
    hints(g, g.h - 1, 'Today', [('space', 'done'), ('f', 'focus'), ('a', 'add'), ('b', 'backlog'), ('x', 'del')], [('?', 'more')])
    g.callout(g.w - 10, g.h - 1, 3)
    page('04-narrow', 'Half-width tile', [('80×44 · half-width tile, beside an editor', g)], '''
<h2>Narrow: under 100 columns</h2>
<p>When the window is tiled into a half column the two panes collapse to one, switched by a tab row. The floating window never hits this; a tiled one often will.</p>
<ol>
<li>Tabs replace panes. <kbd>h</kbd>/<kbd>l</kbd> or <kbd>tab</kbd> switch tabs. Counts keep the other tabs informative. Notes becomes a tab.</li>
<li>Rows drop text metadata first and keep glyph-only chips. Titles get the width. Close times go; where a moved task went stays, because that is the whole content of the row.</li>
<li>The hint bar shows the top five keys and defers to <kbd>?</kbd>.</li>
</ol>
<div class="flow"><b>Pull from backlog when narrow</b><kbd>l</kbd> to the Backlog tab, <kbd>t</kbd> on a task: it moves to Today, the Today count increments. No confirmation.</div>
<p>The review takes the whole window in both layouts; when narrow, the action column becomes a popup on <kbd>⏎</kbd>.</p>''')


# 05 --------------------------------------------------------------------
def mini(h=9):
    return Grid(80, h + 2)


def p05():
    grids = []
    g = mini(); y = 0
    group(g, 0, 80, y, 'Plan'); y += 1
    task(g, 0, 80, y, "Review Anna's PR"); y += 1
    g.put(1, y, ' + ', 'd'); inp(g, 5, y, 60, 'Call the landlord about the leak'); y += 1
    g.put(5, y, '⏎ add & keep typing · esc stop · the text is the whole task', 'd')
    grids.append(('A · Quick add (a)', g))

    g = mini(); y = 0
    group(g, 0, 80, y, 'Plan'); y += 1
    g.put(1, y, '[ ]'); inp(g, 5, y, 60, 'Fix the flaky migration test (CI only)'); y += 1
    task(g, 0, 80, y, 'Book dentist'); y += 2
    g.put(5, y, 'On a recurring copy, ⏎ asks:  1 this copy   2 this and future copies', 'd')
    grids.append(('B · Edit title in place (e)', g))

    g = mini(); y = 0
    group(g, 0, 80, y, 'Plan'); y += 1
    task(g, 0, 80, y, 'Fix the flaky migration test'); y += 1
    task(g, 0, 80, y, "Review Anna's PR", cursor=True, meta='moving ▲▼'); y += 1
    task(g, 0, 80, y, 'Book dentist'); y += 1
    task(g, 0, 80, y, 'Write standup notes'); y += 2
    g.put(5, y, 'Order is remembered per day. Focus items reorder within Focus.', 'd')
    grids.append(('C · Reorder (J/K, or drag)', g))

    g = mini(12); y = 0
    group(g, 0, 80, y, 'Focus'); y += 1
    task(g, 0, 80, y, 'Ship invoice export', focus=True); y += 1
    task(g, 0, 80, y, 'Book dentist', focus=True, cursor=True, meta='just marked'); y += 2
    group(g, 0, 80, y, 'Plan'); y += 1
    task(g, 0, 80, y, 'Fix the flaky migration test'); y += 2
    group(g, 0, 80, y, 'Done', 2); y += 1
    task(g, 0, 80, y, "Review Anna's PR", state='done', done_at='10:41'); y += 1
    task(g, 0, 80, y, 'Reply to the tender questions', state='done', meta='was focus', done_at='11:02'); y += 1
    g.put(5, y, 'space on a done task reopens it at the end of the plan.', 'd'); y += 1
    g.put(5, y, 'Closing does not erase Focus; the row keeps "was focus".', 'd')
    grids.append(('D · Focus (f) and close (space)', g))

    g = mini(13); y = 0
    task(g, 0, 80, y, 'Clean out the garage', cursor=True)
    card(g, 6, 1, 50, 11, 'Move', 'Clean out the garage')
    y = 2
    for i, (k, l, r) in enumerate([('t', 'Today', 'Fri 5 Sep'), ('1', 'Tomorrow', 'Sat 6 Sep'), ('2', 'Next work day', 'Mon 8 Sep'),
                                   ('3', 'Next Monday', 'Mon 8 Sep'), ('g', 'Pick a date…', 'calendar'), ('b', 'Backlog', 'no day')]):
        item(g, 6, 50, y, k, l, r, sel=(i == 0)); y += 1
    y += 1
    g.put(8, y, 'no text field here: g opens the date card for typing', 'd')
    grids.append(('E · Move to a day… (m), the only popup here', g))

    g = mini(7); y = 0
    group(g, 0, 80, y, 'Plan'); y += 1
    task(g, 0, 80, y, 'Fix the flaky migration test'); y += 1
    task(g, 0, 80, y, 'Write standup notes', cursor=True); y += 1
    task(g, 0, 80, y, "Review Anna's PR"); y += 2
    g.hl(0, y, 80); y += 1
    g.seg(1, y, [('TODAY', 'b'), ('Deleted "Book dentist"', ''), ('u', 'k'), ('undo', 'd')], gap=2)
    grids.append(('F · Delete (x): no confirm, undo in the hint bar', g))
    page('05-task-states', 'Task states', grids, '''
<h2>Task states</h2>
<p>A task is a title, a done flag and a place. Every edit is in place; the only popup is the day picker.</p>
<p><b>A</b> Adding appends to the current list. Enter adds and keeps the input open so several can be typed in a row.</p>
<p><b>B</b> Editing never opens a form. For a recurring copy the app asks whether the new title applies forward, because title edits change future copies only.</p>
<p><b>C</b> Reordering is explicit. No sort, no smart ordering, no drag between Focus and Plan (that is <kbd>f</kbd>).</p>
<p><b>D</b> Closing moves a task to Done at the bottom with the time. Focus and Done are shown by position, not by a badge.</p>
<p><b>E</b> The day picker covers "move onto today", "move to a day" and "back to backlog" in one card. It has no text field, so single keys work; <kbd>g</kbd> opens the date card when a typed date is wanted.</p>
<p><b>F</b> Destructive actions are undoable rather than confirmed. The undo offer lives in the hint bar until the next keypress.</p>''')


# 06 --------------------------------------------------------------------
def p06():
    g = Grid(120, 36)
    today_strip(g)
    lx, lw, rx, rw, y0, y1 = sample_today(g, focus='right')
    g.callout(rx + 32, y0 + 1, 1)
    g.callout(rx + 32, y0 + 12, 4)
    cx, cy, cw, ch = 33, 5, 54, 21
    card(g, cx, cy, cw, ch, 'Due by', 'Write the Q4 planning doc')
    g.rput(cx + cw - 2, cy, ' alt-r remind on ', 'd'); g.callout(cx + cw - 3, cy + 1, 2)
    inp(g, cx + 2, cy + 2, 28, '30 sep'); g.put(cx + 32, cy + 2, 'Tue 30 Sep', 'd')
    y = cy + 4
    for k, l, r in [('alt-1', 'Tomorrow', 'Sat 6 Sep'), ('alt-2', 'Next Monday', 'Mon 8 Sep'), ('alt-3', 'In a week', 'Fri 12 Sep'),
                    ('alt-4', 'End of month', 'Tue 30 Sep'), ('alt-0', 'Clear date', 'no due date')]:
        item(g, cx, cw, y, k, l, r); y += 1
    y += 1
    g.put(cx + 2, y, '       September 2026', 'd'); g.callout(cx + cw - 3, y, 3); y += 1
    g.put(cx + 2, y, ' Mo Tu We Th Fr Sa Su', 'd'); y += 1
    rows = ['  1  2  3  4  5  6  7', '  8  9 10 11 12 13 14', ' 15 16 17 18 19 20 21', ' 22 23 24 25 26 27 28', ' 29 30  1  2  3  4  5']
    for i, r in enumerate(rows):
        g.put(cx + 2, y + i, r)
    g.put(cx + 2 + 13, y, ' 5', 'b')          # today
    g.put(cx + 2 + 4, y + 4, '30', 'A b r')   # selected
    g.put(cx + 2 + 7, y + 4, ' 1  2  3  4  5', 'd')
    y += 6
    g.hl(cx + 1, y, cw - 2, 'd'); y += 1
    g.seg(cx + 2, y, [('⏎', 'k'), ('set', 'd'), ('esc', 'k'), ('cancel', 'd'), ('tab', 'k'), ('calendar', 'd'), ('h/l/j/k', 'k'), ('</>', 'k'), ('month', 'd')], gap=1)
    hints(g, g.h - 1, 'Backlog', BACKLOG_HINTS, PANE_KEYS)
    page('06-dates', 'Due, remind, waiting', [('120×36 · floating window', g)], '''
<h2>Dates on backlog tasks, and waiting</h2>
<ol>
<li><kbd>d</kbd> on a backlog row opens the date card for "due by"; <kbd>r</kbd> opens it for "remind on". A task can carry both, shown as separate chips.</li>
<li>One card, two modes; the title says which. Switching keeps the typed date.</li>
<li>Three ways in: typed text ("30 sep", "mon", "+3"), quick picks on Alt plus a digit, or the calendar. The text field has focus, so digits and letters type; <kbd>tab</kbd> moves focus to the calendar, where <kbd>h/l/j/k</kbd> work as keys again. Clearing is a pick, not a separate action.</li>
<li><kbd>w</kbd> toggles waiting and moves the row under Waiting. No reason text; it is a flag. Waiting tasks keep their dates but only reminders surface them.</li>
</ol>
<div class="flow"><b>Due vs remind, in the review</b>Due: surfaces every morning from the date on. Remind: surfaces once, on the date. Both keep the task in the backlog until <kbd>t</kbd> pulls it.</div>
<p>Dates are only offered in the backlog; a task on a day already has a date. Moving a dated backlog task to a day keeps the chips.</p>''')


# 07 --------------------------------------------------------------------
def p07():
    g = Grid(120, 36)
    today_strip(g)
    lx, lw, rx, rw, y0, y1 = frame2(g, ('Today', 'Fri 5 Sep', '6 open · 2 done · 1 moved'), ('Backlog', '', '12 · 3 waiting'), 'left')
    y = y0
    group(g, lx, lw, y, 'Focus'); y += 1
    task(g, lx, lw, y, 'Ship invoice export', focus=True, cursor=True, chips=[('rep', '↻ every Fri')]); g.callout(lx + 26, y, 1); y += 1
    task(g, lx, lw, y, 'Reply to the tender questions', focus=True); y += 2
    group(g, lx, lw, y, 'Plan'); y += 1
    task(g, lx, lw, y, 'Fix the flaky migration test'); y += 1
    task(g, lx, lw, y, 'Write standup notes', chips=[('rep', '↻ work days')]); y += 1
    y = y0
    task(g, rx, rw, y, 'Migrate CI to the new runners', chips=[('due', 'due 12 Sep')]); y += 1
    task(g, rx, rw, y, 'Write the Q4 planning doc', chips=[('due', 'due 30 Sep')]); y += 1
    task(g, rx, rw, y, 'Clean out the garage'); y += 1
    g.put(rx + 5, y, '…', 'd'); y += 2
    group(g, rx, rw, y, 'Repeating', 4); g.callout(rx + 14, y, 4); y += 1
    for t, r in [('Write standup notes', 'every work day'), ('Ship invoice export', 'every Friday'), ('Pay rent', '1st of the month'), ('Water the plants', 'every Mon, Thu')]:
        g.put(rx + 1, y, ' ↻ ', 'd'); g.put(rx + 5, y, t); g.rput(rx + rw - 1, y, r, 'd'); y += 1
    y += 1
    g.put(rx + 5, y, 'Schedules, not tasks. Edit here: future', 'd'); y += 1
    g.put(rx + 5, y, 'copies only. Remove: existing copies stay.', 'd'); y += 1
    g.put(rx + 5, y, 'Collapsed by default; R on the header opens.', 'd')
    cx, cy, cw, ch = 33, 9, 54, 14
    card(g, cx, cy, cw, ch, 'Repeat', 'Ship invoice export'); g.callout(cx + 32, cy, 2)
    y = cy + 2
    for i, (k, l, r) in enumerate([('1', 'Every work day', 'Mon–Fri'), ('2', 'Every day', ''), ('3', 'Every week on', 'Mo Tu We Th [Fr] Sa Su'),
                                   ('4', 'Every month on the', '[ 1st ] · or last'), ('5', 'Every', '[ 2 ] weeks from Fri 5 Sep'), ('0', 'Stop repeating', 'copies stay')]):
        item(g, cx, cw, y, k, l, r, sel=(i == 2)); y += 1
    y += 1
    g.put(cx + 2, y, 'Next: Fri 12 Sep · Fri 19 Sep · Fri 26 Sep', 'd'); g.callout(cx + cw - 3, y, 3); y += 1
    g.hl(cx + 1, y, cw - 2, 'd'); y += 1
    g.seg(cx + 2, y, [('⏎', 'k'), ('save', 'd'), ('esc', 'k'), ('cancel', 'd'), ('h/l', 'k'), ('adjust', 'd')], gap=1)
    hints(g, g.h - 1, 'Today', TODAY_HINTS, PANE_KEYS)
    page('07-repeat', 'Repeat schedule', [('120×36 · floating window', g)], '''
<h2>Recurring tasks</h2>
<ol>
<li>A copy on a day is an ordinary task with a repeat chip. Close, move, reorder, focus: none of it touches the schedule.</li>
<li><kbd>R</kbd> on any task opens the schedule card. On a copy it edits the schedule behind that copy; the current copy stays as it is.</li>
<li>The card previews the next dates so "1st of the month" and "every 2 weeks" are unambiguous before saving.</li>
<li>All schedules are listed at the bottom of the backlog pane, collapsed by default, so there is somewhere to see and edit them without a fourth "place" for tasks.</li>
</ol>
<div class="flow"><b>What a schedule does</b>On launch, for each scheduled date since the last launch, a fresh copy is placed at the end of that day's plan (not Focus). Copies for past days land on the pile like any other unfinished task.</div>
<div class="flow"><b>Editing a copy's title</b><kbd>e</kbd> on a copy asks: this copy only, or this and future copies. Past copies never change.</div>''')


# 08 --------------------------------------------------------------------
def p08():
    g = Grid(120, 36)
    strip(g, [('‹', 'd'), ('Mon 1 Sep', 'b'), ('›', 'd'), ('4 days ago', 'd'), ('. back to today', 'd')],
          [('● 5 in review', 'R b'), ('4 notes n', 'd'), ('/', 'd'), (':', 'd'), ('?', 'd')])
    g.callout(48, 0, 1)
    lx, lw, rx, rw, y0, y1 = frame2(g, ('Mon 1 Sep', 'past day', '8 planned · 3 done · 2 open · 3 moved'), ('Days', '', 'g go to date'), 'left')
    g.callout(rx + rw - 15, 2, 4)
    y = y0
    group(g, lx, lw, y, 'Plan'); y += 1
    task(g, lx, lw, y, 'Call the accountant about VAT', cursor=True, chips=[('pile', 'on the pile')]); g.callout(lx + 36, y, 2); y += 1
    task(g, lx, lw, y, 'Write standup notes', chips=[('rep', '↻ work days'), ('pile', 'on the pile')]); y += 1
    add_row(g, lx, lw, y, 'add a task to this day'); y += 2
    group(g, lx, lw, y, 'Done', 3); g.callout(lx + 10, y, 3); y += 1
    task(g, lx, lw, y, 'Weekly planning', state='done', done_at='09:05'); y += 1
    task(g, lx, lw, y, 'Send the contract draft', state='done', meta='was focus', done_at='11:20'); y += 1
    task(g, lx, lw, y, 'Reply to Anna', state='done', done_at='15:48'); y += 2
    group(g, lx, lw, y, 'Moved', 3); g.callout(lx + 11, y, 5); y += 1
    task(g, lx, lw, y, 'Prepare slides for Monday', state='moved', meta='to today'); y += 1
    task(g, lx, lw, y, 'Book the venue', state='moved', meta='to Wed 3 Sep'); y += 1
    task(g, lx, lw, y, 'Order new office chair', state='moved', meta='to backlog'); y += 1
    y = y0
    group(g, rx, rw, y, 'This week'); y += 1
    for d, r, cur in [('Fri 5 Sep · today', '2 / 8', False), ('Thu 4 Sep', '4 / 5 · 1 open', False), ('Wed 3 Sep', '6 / 6', False),
                      ('Tue 2 Sep', '3 / 3', False), ('Mon 1 Sep', '3 / 5 · 2 open', True)]:
        g.put(rx + 5, y, d); g.rput(rx + rw - 1, y, r, 'd')
        if cur: g.add_attr(rx, y, rw, 'c')
        y += 1
    y += 1
    group(g, rx, rw, y, 'Last week'); y += 1
    for d, r in [('Fri 29 Aug', '5 / 5'), ('Thu 28 Aug', '4 / 4'), ('Wed 27 Aug', '2 / 2'), ('Tue 26 Aug', '7 / 7'), ('Mon 25 Aug', '3 / 4 · 1 open')]:
        g.put(rx + 5, y, d); g.rput(rx + rw - 1, y, r, 'd'); y += 1
    y += 1
    group(g, rx, rw, y, 'Earlier'); y += 1
    g.put(rx + 5, y, 'Fri 22 Aug'); g.rput(rx + rw - 1, y, '1 / 2 · 1 open', 'd'); y += 1
    g.put(rx + 5, y, '…'); g.rput(rx + rw - 1, y, 'days with nothing planned are skipped', 'd')
    hints(g, g.h - 1, 'Past day', [('[ ]', 'day'), ('.', 'today'), ('g', 'go to date'), ('space', 'close'), ('t', 'to today'), ('b', 'to backlog'), ('m', 'move…'), ('⏎', 'follow moved')], PANE_KEYS)
    page('08-history', 'History', [('120×36 · floating window', g)], '''
<h2>History: browsing past days</h2>
<p>History is not a separate screen: it is the same day view stepped backwards. A past day is drawn exactly as it was while it was today, and the two panes stay in place.</p>
<ol>
<li>Stepping back with <kbd>[</kbd> changes the day pane. The status line says how far back you are and offers one key home.</li>
<li>Unfinished tasks on a past day are the review pile seen from the other side, and can be dealt with here with the same keys.</li>
<li>Closed tasks sit in Done in the order they were closed, with the time, exactly where they dropped while the day was today; one closed out of Focus carries <em>was focus</em>, so the marking is kept without the row jumping back up. Focus is empty on this day and so is not drawn: a day is not re-arranged just because it is no longer today.</li>
<li>While browsing the past, the backlog pane gives way to a day list with done counts. Days with nothing planned are skipped. <kbd>l</kbd> then <kbd>.</kbd> restores the backlog.</li>
<li>Tasks moved off this day get their own group at the bottom, under Done. An arrow in the box says "not here any more", and the right side says where: today, another day, or the backlog. Normal weight, because they are not finished; only Done is dim. Each row is a pointer, not a copy: <kbd>⏎</kbd> jumps there. "On the pile" stays reserved for tasks still open on this day.</li>
</ol>
<div class="flow"><b>Future days</b><kbd>]</kbd> from today steps forward. A future day holds only what has actually been put there: tasks moved onto it, and whatever is added to it. Recurring copies are not drawn ahead of the day that creates them.</div>''')


# 09 --------------------------------------------------------------------
def p09():
    g = Grid(120, 36)
    today_strip(g)
    sample_today(g, cursor=False)
    cx, cy, cw, ch = 20, 4, 80, 20
    g.box(cx, cy, cw, ch)
    g.put(cx + 2, cy + 1, '/', 'b'); inp(g, cx + 4, cy + 1, 60, 'invoice'); g.rput(cx + cw - 2, cy + 1, '9 matches', 'd'); g.callout(cx + cw - 1, cy + 1, 1)
    g.hl(cx + 1, cy + 2, cw - 2, 'd')
    y = cy + 3
    g.put(cx + 2, y, 'OPEN', 'd'); g.callout(cx + 8, y, 2); y += 1
    for chk, t, r in [('[ ]', 'Ship invoice export', 'today · focus'), ('[ ]', 'Chase the unpaid invoices', 'backlog · waiting')]:
        g.put(cx + 2, y, chk, ''); g.put(cx + 6, y, t); g.rput(cx + cw - 2, y, r, 'd'); y += 1
    y += 1
    g.put(cx + 2, y, 'CLOSED', 'd'); g.callout(cx + 10, y, 3); y += 1
    for i, (t, r) in enumerate([('Send the invoice to Nordic Ltd', 'Thu 4 Sep'), ('Ship invoice export', 'Fri 29 Aug · ↻'), ('Ship invoice export', 'Fri 22 Aug · ↻'),
                                ('Fix invoice PDF font', 'Tue 19 Aug'), ('Set up invoice numbering', 'Mon 4 Aug'), ('Invoice template v2', 'Fri 18 Jul'), ('Ask Anna about invoice terms', 'Wed 2 Jul')]):
        g.put(cx + 2, y, '[x]', 'G'); g.put(cx + 6, y, t); g.rput(cx + cw - 2, y, r, 'd')
        if i == 0: g.add_attr(cx + 1, y, cw - 2, 'c')
        y += 1
    y += 1
    g.hl(cx + 1, y, cw - 2, 'd'); y += 1
    g.seg(cx + 2, y, [('⏎', 'k'), ('go to that day', 'd'), ('alt-t', 'k'), ('re-add to today as a new task', 'd'), ('esc', 'k'), ('close', 'd')], gap=1)
    g.callout(cx + cw - 1, y, 4)
    hints(g, g.h - 1, 'Search', [('type', 'to filter'), ('↑/↓', 'move')])
    page('09-search', 'Search', [('120×36 · floating window', g)], '''
<h2>Search across history</h2>
<p><kbd>/</kbd> opens a launcher-style box, the same shape Omarchy's menus have. Filtering is instant, by title, across everything the app has ever stored.</p>
<ol>
<li>One input, no operators. The match count keeps it honest.</li>
<li>Open tasks first, with their place, because the most common search is "did I already add this".</li>
<li>Closed tasks are the history search: every closed copy with the day it was closed on. Recurring copies appear once per closed copy.</li>
<li>Enter jumps to that day. <kbd>alt-t</kbd> is "I need to do this again": a fresh task on today with the same title. While the box has focus, plain letters type; the few extra actions use Alt.</li>
</ol>
<div class="flow"><b>Empty result</b>The box stays open with "nothing matches", and Enter adds the typed text as a task on today (screen 12).</div>''')


# 10 --------------------------------------------------------------------
def p10():
    g = Grid(120, 36)
    strip(g, [('Notes', 'b'), ('4 notes', 'd')], [('n or esc back to today', 'd'), ('/', 'd'), (':', 'd'), ('?', 'd')])
    g.callout(20, 0, 1)
    lx, lw, rx, rw, y0, y1 = frame2(g, ('Notes', '', 'a new'), ('Note', 'Thu 4 Sep 16:40', 'esc back'), 'right', div=44)
    g.callout(rx + 24, 2, 3)
    y = y0
    notes = [('Mention to Anna: CI runner budget,', 'yesterday', True), ('Draft reply to tender Q3: "We can', '2 days', False),
             ('nordic ltd PO 4471, due 30 days', '2 days', False), ('rsync -av --delete ~/work nas:/bk', 'last week', False)]
    for t, age, cur in notes:
        g.put(lx + 1, y, ' ▪ ', 'd'); g.put(lx + 4, y, t[:lw - 16]); g.rput(lx + lw - 1, y, age, 'd')
        if cur: g.add_attr(lx, y, lw, 'c')
        y += 1
    y += 1
    add_row(g, lx, lw, y, 'new note'); g.callout(lx + lw - 4, y, 2)
    y = y0
    lines = ['Mention to Anna:', '- CI runner budget', '- Friday demo slot', '- ask about the retro format', '',
             'Also: the tender deadline moved to the 12th, check with legal first.', '',
             'Draft:', 'Hi Anna, two things before Friday. The CI runner budget needs a decision', 'this week, and I would like the demo slot after lunch rather than before.']
    for i, l in enumerate(lines):
        g.put(rx + 2, y + i, l)
    g.put(rx + 2 + len(lines[-1]), y + len(lines) - 1, '▏', 'b')
    g.callout(rx + rw - 2, y0, 4)
    hints(g, g.h - 1, 'Note', [('type', 'to edit'), ('esc', 'back to the list')], [('in the list:', ''), ('⏎', 'open'), ('a', 'new'), ('x', 'delete'), ('n', 'back to today')])
    page('10-scratchpad', 'Scratchpad', [('120×36 · floating window, notes page', g)], '''
<h2>Scratchpad</h2>
<p>A post-it block, on its own page. <kbd>n</kbd> switches the whole window to Notes and back, so a note gets real width; the day and backlog are one key away, not squeezed beside it. Text only.</p>
<ol>
<li>The status line says where you are and how to get back. Search, commands and help still work here.</li>
<li>Notes are a list, newest first: first line and age. No titles, no folders, no search. Create, open, throw away; delete has no confirmation, only undo. Nothing expires on its own.</li>
<li>An open note takes the wide pane and shows when it was made, so a stale note is easy to spot and bin.</li>
<li>The note is a plain multi-line text area with a cursor. No formatting, no toolbar. Every key types; <kbd>esc</kbd> returns focus to the list, where <kbd>a</kbd>, <kbd>x</kbd> and <kbd>n</kbd> work as keys.</li>
</ol>
<div class="flow"><b>Why a page and not a drawer</b>A drawer beside the day left a note about 40 columns wide, which is a strip, not a sheet. A page gives it 75 columns in the floating window and everything in a tile. The cost is one keypress to see the day again.</div>
<div class="flow"><b>Narrow layout</b>Notes is one of the three tabs (screen 04); the list and the open note stack in that tab.</div>''')


# 11 --------------------------------------------------------------------
def p11():
    g = Grid(120, 24)
    group(g, 0, 60, 0, 'Plan')
    task(g, 0, 60, 1, 'Book dentist', cursor=True)
    task(g, 0, 60, 2, 'Write standup notes')
    cx, cy, cw, ch = 30, 2, 60, 17
    g.box(cx, cy, cw, ch)
    g.put(cx + 2, cy + 1, ':', 'b'); inp(g, cx + 4, cy + 1, 52, 'wa')
    g.hl(cx + 1, cy + 2, cw - 2, 'd')
    y = cy + 3
    g.put(cx + 2, y, 'FOR "BOOK DENTIST"', 'd'); y += 1
    for i, (l, r) in enumerate([('Mark waiting', 'w'), ('Move to a day…', 'm')]):
        g.put(cx + 2, y, l); g.rput(cx + cw - 2, y, r, 'k')
        if i == 0: g.add_attr(cx + 1, y, cw - 2, 'c')
        y += 1
    y += 1
    g.put(cx + 2, y, 'APP', 'd'); y += 1
    for l, r in [('Show waiting only', ''), ('Open morning review', '2 on the pile'), ('Repeating schedules', 'R'), ('Go to date…', 'g')]:
        g.put(cx + 2, y, l); g.rput(cx + cw - 2, y, r, 'k' if len(r) == 1 else 'd'); y += 1
    y += 1
    g.hl(cx + 1, y, cw - 2, 'd'); y += 1
    g.seg(cx + 2, y, [('⏎', 'k'), ('run', 'd'), ('esc', 'k'), ('close', 'd'), ('· right column: the direct key, for next time', 'd')], gap=1)
    hints(g, g.h - 1, 'Commands', [('type', 'to filter'), ('↑/↓', 'move')])

    h = Grid(120, 17)
    h.box(2, 1, 116, 13)
    h.put(4, 1, ' Keys ', 'A b'); h.rput(116, 1, ' ? or esc close ', 'd')
    cols = [(5, 'Everywhere', [[('j/k', 'move'), ('h/l tab', 'pane')], [('a', 'add'), ('e', 'edit'), ('x', 'delete'), ('u', 'undo')],
                               [('space', 'done / reopen')], [('/', 'search'), (':', 'commands'), ('?', 'help')], [('n', 'notes page'), ('q', 'quit')]]),
            (45, 'Day', [[('J/K', 'reorder'), ('f', 'focus')], [('b', 'to backlog'), ('m', 'move to day…')], [('[ ]', 'prev/next day'), ('.', 'today')],
                         [('g', 'go to date'), ('R', 'repeat')]]),
            (83, 'Backlog', [[('t', 'to today'), ('m', 'move to day…')], [('d', 'due by'), ('r', 'remind on'), ('w', 'waiting')], [('R', 'repeat schedule')], [],
                             'REVIEW', [('d', 'done'), ('t', 'today'), ('b', 'backlog')], [('k', 'keep'), ('⏎', 'next step')]])]
    for x, title, lines in cols:
        h.put(x, 3, title.upper(), 'd')
        for i, l in enumerate(lines):
            if l == 'REVIEW':
                h.put(x, 4 + i, l, 'd'); continue
            xx = x
            for j, (k, lbl) in enumerate(l):
                if j: xx = h.put(xx, 4 + i, ' · ', 'd')
                xx = h.put(xx, 4 + i, k, 'k')
                xx = h.put(xx + 1, 4 + i, lbl)
    page('11-palette-help', 'Command palette & help', [('A · Command palette (:) over the home screen', g), ('B · Help overlay (?), the full key map', h)], '''
<h2>Command palette and help</h2>
<p>Two escape hatches so nobody has to memorise the key map before the app is usable.</p>
<p><b>A</b> The palette is the Omarchy launcher shape: a centred box with a fuzzy filter and the selected row highlighted. Actions for the cursor task come first, app-level ones after. Inside the palette you type to filter, move with ↑/↓ and run with Enter; letters type, as in every text field. The key in the right column is not for pressing here: it is the direct key for next time, on the home screen, so the palette teaches itself out of use.</p>
<p><b>B</b> Help is a static overlay of the same keys the hint bar shows, grouped by context. It is the whole map; there are no hidden keys.</p>
<div class="flow"><b>Key conventions</b>Lowercase acts on the cursor row. Uppercase reorders or opens schedule editing. Punctuation navigates. Enter confirms, Escape backs out one level, <kbd>u</kbd> undoes.</div>
<p>There is no settings screen. Colours and font come from the terminal, which Omarchy themes; the app has nothing of its own to configure.</p>''')


# 12 --------------------------------------------------------------------
def p12():
    grids = []
    g = Grid(60, 11); header(g, 0, 60, 0, 'Today', 'Sat 6 Sep', 'nothing planned', focus=True); g.hl(0, 1, 60)
    g.put(14, 4, 'Nothing planned.', 'd'); g.put(6, 5, 'a add a task · l then t pull from the backlog', 'd')
    grids.append(('A · Empty day', g))
    g = Grid(60, 11); header(g, 0, 60, 0, 'Backlog', '', '0'); g.hl(0, 1, 60)
    g.put(14, 4, 'Backlog is empty.', 'd'); g.put(8, 5, 'a add · b on a day task sends it here', 'd')
    grids.append(('B · Empty backlog', g))
    g = Grid(60, 7); strip(g, [('Today · Fri 5 Sep', 'b')], [('0 in review', 'd')])
    g.put(1, 3, 'Opens straight to Today. Count is 0, not an alert.', 'd')
    grids.append(('C · Nothing to review: the review is skipped, not shown', g))
    g = Grid(60, 11); header(g, 0, 60, 0, 'Sun 31 Aug', 'past day', 'nothing was planned', focus=True); g.hl(0, 1, 60)
    g.put(10, 4, 'Nothing was planned on this day.', 'd'); g.put(8, 5, '[ keeps stepping back · g pick a date', 'd')
    grids.append(('D · Past day with nothing planned', g))
    g = Grid(60, 10); g.box(2, 0, 56, 7)
    g.put(4, 1, '/', 'b'); inp(g, 6, 1, 38, 'tax return'); g.rput(56, 1, '0 matches', 'd'); g.hl(3, 2, 54, 'd')
    g.put(4, 3, 'Nothing matches, open or closed.', 'd'); g.hl(3, 4, 54, 'd')
    g.seg(4, 5, [('⏎', 'k'), ('add "tax return" to today', 'd'), ('esc', 'k'), ('close', 'd')], gap=1)
    grids.append(('E · Search with no match', g))
    g = Grid(60, 8); header(g, 0, 60, 0, 'Notes', '', '0'); g.hl(0, 1, 60)
    add_row(g, 0, 60, 3, 'new note')
    grids.append(('F · No notes', g))
    page('12-empty', 'Empty states', grids, '''
<h2>Empty states</h2>
<p>Empty states say what the list is for and name the one or two keys that fill it. No illustrations, no encouragement.</p>
<p><b>A, B</b> The two lists point at each other.</p>
<p><b>C</b> The review only ever appears with content. Its absence is the good news.</p>
<p><b>D</b> Past days with nothing planned still exist in the day stepper so the calendar stays continuous; the day list on screen 08 skips them.</p>
<p><b>E</b> The most likely reason for a miss is that the task does not exist yet, so with no matches Enter adds the typed text as a task on today.</p>
<p><b>F</b> The new-note row is the whole empty state.</p>''')


# ------------------------------------------------------------- output ---
TEMPLATE = '''<!doctype html>
<html><head><meta charset="utf-8"><title>{title}</title>
<link rel="stylesheet" href="wf.css"><script src="themes.js"></script><script src="wf.js" defer></script></head>
<body>
<div class="stage">
<div class="desktop">
{grids}
</div>
<aside class="notes">{notes}
</aside>
</div>
</body></html>
'''


def main():
    for f in (p01, p02, p03, p04, p05, p06, p07, p08, p09, p10, p11, p12):
        f()
    for name, title, grids, notes in PAGES:
        blocks = []
        for lbl, g in grids:
            blocks.append(f'<div class="shot"><div class="lbl">{html.escape(lbl)}</div><pre class="tui" style="width:{g.w}ch">{g.html_()}</pre></div>')
        (OUT / f'{name}.html').write_text(TEMPLATE.format(title=html.escape(title), grids='\n'.join(blocks), notes=notes))
        (OUT / f'{name}.txt').write_text('\n'.join(f'== {lbl}\n\n{g.txt()}' for lbl, g in grids))
        print(name)


if __name__ == '__main__':
    main()
