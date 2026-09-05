//! Application state, the launch sequence, reloading, turning actions
//! into commands, and the screen layout.

use jiff::Zoned;
use jiff::civil::Date;
use tracing::warn;

use crate::domain::{self, Model, Store, StoreError};
use crate::input::{Action, KeyContext, Pane};

#[cfg(test)]
mod tests;

/// Whether the event loop goes round again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Quit,
}

/// Which pane and which row, with its task or note id, occupies which cell
/// rectangle. Phase 6 fills this in; phase 4 draws no rows to record.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Layout;

pub struct App {
    store: Box<dyn Store>,
    model: Model,
    /// The database version the model was loaded at, so a change made by
    /// another window can be noticed.
    version: u64,
    today: Date,
    pane: Pane,
    layout: Layout,
}

impl App {
    /// Loads the model and runs the launch sequence. Phase 8 adds the
    /// generation of recurring copies to it and phase 9 the review gate.
    pub fn new(store: Box<dyn Store>, now: &Zoned) -> Result<App, StoreError> {
        let model = store.load()?;
        let version = store.version()?;

        Ok(App {
            store,
            model,
            version,
            today: domain::working_day(now),
            pane: Pane::Day,
            layout: Layout,
        })
    }

    /// The single entry point for every event, ticks included.
    pub fn update(&mut self, action: Action) -> Flow {
        match action {
            Action::Quit => return Flow::Quit,
            Action::Tick | Action::FocusGained => {
                // The clock is read here and nowhere else, so the date
                // rolling over while the window is open is just a tick.
                self.today = domain::working_day(&Zoned::now());
                self.reload_if_stale();
            }
            Action::Resize => {}
        }
        Flow::Continue
    }

    /// Picks up what another window has done. `version` moves only for a
    /// write made on another connection, so this is free when nothing has
    /// happened.
    fn reload_if_stale(&mut self) {
        let version = match self.store.version() {
            Ok(version) => version,
            Err(error) => {
                warn!(%error, "the database version could not be read");
                return;
            }
        };
        if version == self.version {
            return;
        }
        match self.store.load() {
            Ok(model) => {
                self.model = model;
                self.version = version;
            }
            Err(error) => warn!(%error, "the model could not be reloaded"),
        }
    }

    pub fn key_context(&self) -> KeyContext {
        KeyContext::Home {
            pane: self.pane,
            text_field: false,
        }
    }

    /// The working day, which is what the status line calls today.
    pub fn today(&self) -> Date {
        self.today
    }

    pub fn model(&self) -> &Model {
        &self.model
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn set_layout(&mut self, layout: Layout) {
        self.layout = layout;
    }
}
