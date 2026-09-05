use super::*;

use std::cell::RefCell;
use std::rc::Rc;

/// An in-memory `Store`: a `Model` and `Model::apply`.
///
/// A clone is another handle on the same data, which is how a test plays
/// the part of a second window. It is `pub(crate)` because the tests of
/// every module that holds a `Box<dyn Store>` drive it through this.
#[derive(Clone, Default)]
pub(crate) struct MemStore {
    shared: Rc<RefCell<Shared>>,
}

#[derive(Default)]
struct Shared {
    model: Model,
    version: u64,
}

impl MemStore {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// A store that already holds something, for a test of loading.
    pub(crate) fn holding(model: Model) -> Self {
        MemStore {
            shared: Rc::new(RefCell::new(Shared { model, version: 0 })),
        }
    }
}

impl Store for MemStore {
    fn load(&self) -> Result<Model, StoreError> {
        Ok(self.shared.borrow().model.clone())
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        let mut shared = self.shared.borrow_mut();
        shared.model.apply(change);
        // SQLite moves data_version only for a write from another
        // connection; every handle here shares one counter, so a commit
        // always moves it. The application re-reads the version after its
        // own commit either way.
        shared.version += 1;
        Ok(())
    }

    fn version(&self) -> Result<u64, StoreError> {
        Ok(self.shared.borrow().version)
    }
}

fn at(text: &str) -> Zoned {
    text.parse().expect("a zoned timestamp")
}

#[test]
fn a_day_begins_at_five() {
    let late = at("2026-09-05T01:30:00+02:00[Europe/Copenhagen]");
    assert_eq!(working_day(&late).to_string(), "2026-09-04");

    let early = at("2026-09-05T05:00:00+02:00[Europe/Copenhagen]");
    assert_eq!(working_day(&early).to_string(), "2026-09-05");
}

#[test]
fn the_model_takes_a_committed_change() {
    let mut store = MemStore::new();
    let change = Change {
        writes: vec![Write::SetMeta {
            key: "review_on".into(),
            value: "2026-09-05".into(),
        }],
    };

    store.commit(&change).expect("commit");

    let model = store.load().expect("load");
    assert_eq!(
        model.meta.get("review_on").map(String::as_str),
        Some("2026-09-05")
    );
    assert_eq!(store.version().expect("version"), 1);
}
