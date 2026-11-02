//! One-line descriptions of the script API, kept next to the registrations (#364).
//!
//! Every `register_fn`, property and operator in `api.rs` and `inst.rs` is preceded by a
//! `docs.add(name, text)` line, so a description lives beside the function it describes and
//! there is no second list. `iwbtg script api-docs` prints them into `docs/format/script-api.md`
//! and fails when a registered function has none.

use std::collections::BTreeMap;

/// Descriptions by function, operator or property name. Overloads share one description.
#[derive(Clone, Default, Debug)]
pub struct Docs(BTreeMap<String, &'static str>);

impl Docs {
    /// Adds the description of `name`. Describing a name twice is a bug in the registrations.
    ///
    /// # Panics
    ///
    /// When `name` already has a description.
    pub fn add(&mut self, name: &str, text: &'static str) {
        let old = self.0.insert(name.to_owned(), text);
        assert!(old.is_none(), "`{name}` is described twice");
    }

    /// The description of `name`, if there is one.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&'static str> {
        self.0.get(name).copied()
    }
}
