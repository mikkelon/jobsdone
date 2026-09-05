//! Raw mode, alternate screen, mouse capture, the panic hook, and the
//! event loop with its 250 ms tick.
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
    enter()?;
    install_panic_hook();

    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let outcome = go_round(&mut terminal, &mut app);

    // The terminal is restored whether or not the loop ended well.
    if let Err(error) = leave() {
        error!(%error, "the terminal could not be restored");
    }
    outcome
}

fn go_round(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> io::Result<()> {
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

        if let Some(action) = action
            && app.update(action) == Flow::Quit
        {
            return Ok(());
        }
    }
}

fn enter() -> io::Result<()> {
    enable_raw_mode()?;
    execute!(
        io::stdout(),
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableFocusChange
    )
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
