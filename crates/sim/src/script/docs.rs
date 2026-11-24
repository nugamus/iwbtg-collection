//! One-line descriptions of the script API, kept next to the registrations (#364).
//!
//! Every `register_fn`, property and operator in `api.rs` and `inst.rs` is preceded by a
//! `docs.add(name, text)` line, so a description lives beside the function it describes and
//! there is no second list. `iwbtg script api-docs` prints them into `docs/format/script-api.md`
//! and fails when a registered function has none.
//!
//! A property can also be registered as read only: `docs.add_read_only` records it so the
//! generated table says `read only` even when the setter exists to refuse the write with a clear
//! error (the kid's `yflag` and `flipped`, decision 0004).

use std::collections::BTreeMap;

/// Whether a property may be written.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Access {
    /// A getter and a working setter.
    ReadWrite,
    /// A getter; a write is refused.
    ReadOnly,
}

/// Descriptions by function, operator or property name. Overloads share one description.
#[derive(Clone, Default, Debug)]
pub struct Docs(BTreeMap<String, (&'static str, Access)>);

impl Docs {
    /// Adds the description of `name`. Describing a name twice is a bug in the registrations.
    ///
    /// # Panics
    ///
    /// When `name` already has a description.
    pub fn add(&mut self, name: &str, text: &'static str) {
        self.insert(name, text, Access::ReadWrite);
    }

    /// Adds the description of a read-only property: a write is a script error.
    ///
    /// # Panics
    ///
    /// When `name` already has a description.
    pub fn add_read_only(&mut self, name: &str, text: &'static str) {
        self.insert(name, text, Access::ReadOnly);
    }

    fn insert(&mut self, name: &str, text: &'static str, access: Access) {
        let old = self.0.insert(name.to_owned(), (text, access));
        assert!(old.is_none(), "`{name}` is described twice");
    }

    /// The description of `name`, if there is one.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&'static str> {
        self.0.get(name).map(|(text, _)| *text)
    }

    /// Whether the property `name` is writable; `None` when it is not described.
    #[must_use]
    pub fn access(&self, name: &str) -> Option<Access> {
        self.0.get(name).map(|(_, access)| *access)
    }
}
