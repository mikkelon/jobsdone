use super::*;

/// An in-memory `Store`: a `Model` and `Model::apply`. It is `pub(crate)`
/// because the tests of every module that holds a `Box<dyn Store>` drive
/// it through this.
pub(crate) struct MemStore {
    model: Model,
    version: u64,
}

impl MemStore {
    pub(crate) fn new() -> Self {
        MemStore {
            model: Model::empty(),
            version: 0,
        }
    }
}

impl Store for MemStore {
    fn load(&self) -> Result<Model, StoreError> {
        Ok(self.model.clone())
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.model.apply(change);
        self.version += 1;
        Ok(())
    }

    fn version(&self) -> Result<u64, StoreError> {
        Ok(self.version)
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
