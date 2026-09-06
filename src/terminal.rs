//! Raw mode, alternate screen, mouse capture while the `mouse` setting
//! asks for it, the panic hook, and the event loop with its 250 ms tick.
//!
//! Events go in and frames come out. This module never sees time: a tick
//! is an action like any other.

use std::io::{self, Stdout};
use std::panic;
use std::time::Duration;

use crossterm::event::{
    self, DisableFocusChange, DisableMouseCapture, EnableFocusChange, EnableMouseCapture,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tracing::error;

use crate::app::{App, Flow};
use crate::input::{self, Action};

/// Every timeout is a tick (STACK.md section 2).
const TICK: Duration = Duration::from_millis(250);

/// Enters raw mode, installs the panic hook, loops until `Flow::Quit`,
/// restores the terminal.
pub fn run(mut app: App) -> io::Result<()> {
    // The `mouse` setting decides whether the program is handed the
    // mouse at all, from the first frame on.
    let mut mouse = app.settings().mouse();
    enter(mouse)?;
    install_panic_hook();

    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let outcome = go_round(&mut terminal, &mut app, &mut mouse);

    // The terminal is restored whether or not the loop ended well.
    if let Err(error) = leave() {
        error!(%error, "the terminal could not be restored");
    }
    outcome
}

fn go_round(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    mouse: &mut bool,
) -> io::Result<()> {
    loop {
        let mut layout = None;
        {
            let view = &*app;
            terminal.draw(|frame| layout = Some(crate::ui::draw(view, frame)))?;
        }
        if let Some(layout) = layout {
            app.set_layout(layout);
        }

        let action = if event::poll(TICK)? {
            input::action_for(&event::read()?, app.key_context())
        } else {
            Some(Action::Tick)
        };

        if let Some(action) = action {
            if app.update(action) == Flow::Quit {
                return Ok(());
            }
            // The setting takes effect at once, whether it was changed on
            // the settings page or by another window a tick picked up.
            if app.settings().mouse() != *mouse {
                *mouse = !*mouse;
                take_the_mouse(*mouse)?;
            }
        }
    }
}

fn enter(mouse: bool) -> io::Result<()> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, EnableFocusChange)?;
    take_the_mouse(mouse)
}

/// Whether the terminal hands its mouse events to the program. With them
/// off the terminal's own selection and scrollback work again, which is
/// the whole of what the `mouse` setting buys (DOMAIN.md section 19).
fn take_the_mouse(taking: bool) -> io::Result<()> {
    if taking {
        execute!(io::stdout(), EnableMouseCapture)
    } else {
        execute!(io::stdout(), DisableMouseCapture)
    }
}

fn leave() -> io::Result<()> {
    execute!(
        io::stdout(),
        DisableFocusChange,
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    disable_raw_mode()
}

/// A crash must never leave the terminal in raw mode, or the shell it
/// returns to is unusable.
fn install_panic_hook() {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = leave();
        previous(info);
    }));
}
