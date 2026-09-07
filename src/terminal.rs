//! Raw mode, alternate screen, mouse capture while the `mouse` setting
//! asks for it, the panic hook, and the event loop with its 250 ms tick.
//!
//! Events go in and frames come out. This module never sees time: a tick
//! is an action like any other.

use std::io::{self, Stdout, Write};
use std::panic;
use std::process::{Command, Stdio};
use std::time::Duration;

use crossterm::event::{
    self, DisableFocusChange, DisableMouseCapture, EnableFocusChange, EnableMouseCapture, Event,
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
            let event = event::read()?;
            // A terminal the mouse has been handed back to sends no mouse
            // events. One that sends them anyway, a test driver writing
            // them straight into the pane, is answered the same way.
            if !*mouse && matches!(event, Event::Mouse(_)) {
                None
            } else {
                input::action_for(&event, app.key_context())
            }
        } else {
            Some(Action::Tick)
        };

        if let Some(action) = action {
            match app.update(action) {
                Flow::Quit => return Ok(()),
                Flow::CopyNote(text) => app.copied_note(copy_to_clipboard(&text)),
                Flow::Continue => {}
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

/// Use the desktop clipboard, including when the app runs inside tmux.
/// Text goes through stdin so prompts are never shell code or arguments.
fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let (program, args): (&str, &[&str]) = if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        ("wl-copy", &["--type", "text/plain;charset=utf-8"])
    } else if std::env::var_os("DISPLAY").is_some() {
        ("xclip", &["-selection", "clipboard", "-in"])
    } else {
        return Err("Could not copy note: no desktop clipboard is available.".to_owned());
    };
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Could not copy note: {program}: {error}"))?;
    let written = child
        .stdin
        .take()
        .expect("piped clipboard input")
        .write_all(text.as_bytes());
    let status = child.wait();
    written.map_err(|error| format!("Could not copy note: {error}"))?;
    match status {
        Ok(status) if status.success() => Ok(()),
        Ok(_) => Err(format!("Could not copy note: {program} failed.")),
        Err(error) => Err(format!("Could not copy note: {error}")),
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
