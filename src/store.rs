//! StateStore — the durable-state boundary behind the contract.
//!
//! core's mutable state is reached only through this trait, never by addressing
//! the backend directly. [`FileStore`] is the only implementation today (it is
//! the current on-disk layout, behaviorally identical to the pre-trait code);
//! the trait is shaped from *operations* (read / atomic-write / exclusive-create
//! / remove / list), not paths, so an embedded-DB backend could implement it
//! without changing a single caller.
//!
//! Scope (staged): this trait currently covers the nouns whose raw access lived
//! entirely inside one module — the mailbox (`Ask`/`Answer`) and the lease
//! (`Claim`). Those nouns are now genuinely backend-swappable. Goals / PLAYBOOK /
//! journal / sensors / goal-activity / action-wal still go through `Paths` and
//! will migrate noun-by-noun (each moving its reads AND writes together).
//!
//! NOT in scope: SensorRuntime + scratch (snapshots, runs, prompts, lock, cost
//! ledger, reports) — a separate concern with a different lifecycle (regenerated
//! each beat / append-only / coordination), which stays on [`Paths`].

use crate::paths::Paths;
use std::fs;
use std::io;

/// A logical, backend-agnostic address for one piece of durable state. A backend
/// maps each variant to its own storage (FileStore -> a path; a DB -> a row).
#[derive(Debug, Clone)]
pub enum Key {
    /// A worker's pending question (`looop _ ask`).
    Ask(String),
    /// The human's answer to an ask (`looop _ answer`).
    Answer(String),
    /// A worker's resource lease (`looop _ claim`).
    Claim(String),
}

/// A collection of keys to enumerate.
#[derive(Debug, Clone, Copy)]
pub enum Collection {
    Asks,
    Answers,
    Claims,
}

/// The durable-state operations the contract verbs are built on. Every method is
/// expressible by both a filesystem and a DB; nothing returns a path.
pub trait StateStore {
    /// The stored contents of `key`, or `None` if absent.
    fn read(&self, key: &Key) -> Option<String>;

    /// Whether `key` currently exists.
    fn exists(&self, key: &Key) -> bool;

    /// Durably replace `key` with `contents`, atomically — a concurrent reader
    /// never observes a half-written value (FileStore: temp -> fsync -> rename).
    fn write_atomic(&self, key: &Key, contents: &str) -> io::Result<()>;

    /// Atomic create-if-absent — the mutual-exclusion primitive. Returns
    /// `Ok(true)` if this call created `key`, `Ok(false)` if it already existed.
    /// FileStore uses `O_EXCL`; a DB would use a unique insert. This is what lets
    /// two racers never both "win" a lease.
    fn create_exclusive(&self, key: &Key, contents: &str) -> io::Result<bool>;

    /// Remove `key`. Absent key is not an error (idempotent).
    fn remove(&self, key: &Key) -> io::Result<()>;

    /// The names present in `collection` (the `<name>` part of each key), in
    /// sorted order. For `Asks`/`Answers` that is the ask id; for `Claims` the
    /// claim name.
    fn list(&self, collection: &Collection) -> Vec<String>;
}

/// The filesystem-backed [`StateStore`] — the current on-disk layout. Borrows
/// the resolved [`Paths`] so it stays a thin mapping from logical key to file.
pub struct FileStore<'a> {
    paths: &'a Paths,
}

impl<'a> FileStore<'a> {
    pub fn new(paths: &'a Paths) -> Self {
        FileStore { paths }
    }

    /// Map a logical key to its backing file.
    fn path(&self, key: &Key) -> std::path::PathBuf {
        match key {
            Key::Ask(id) => self.paths.asks_dir().join(format!("{id}.json")),
            Key::Answer(id) => self.paths.answers_dir().join(format!("{id}.json")),
            Key::Claim(name) => self.paths.claims_dir().join(format!("{name}.json")),
        }
    }

    /// Map a collection to its backing directory.
    fn dir(&self, c: &Collection) -> std::path::PathBuf {
        match c {
            Collection::Asks => self.paths.asks_dir(),
            Collection::Answers => self.paths.answers_dir(),
            Collection::Claims => self.paths.claims_dir(),
        }
    }
}

impl StateStore for FileStore<'_> {
    fn read(&self, key: &Key) -> Option<String> {
        fs::read_to_string(self.path(key)).ok()
    }

    fn exists(&self, key: &Key) -> bool {
        self.path(key).is_file()
    }

    fn write_atomic(&self, key: &Key, contents: &str) -> io::Result<()> {
        crate::util::write_atomic(&self.path(key), contents.as_bytes())
    }

    fn create_exclusive(&self, key: &Key, contents: &str) -> io::Result<bool> {
        let path = self.path(key);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut f) => {
                use io::Write;
                f.write_all(contents.as_bytes())?;
                Ok(true)
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(false),
            Err(e) => Err(e),
        }
    }

    fn remove(&self, key: &Key) -> io::Result<()> {
        match fs::remove_file(self.path(key)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    fn list(&self, collection: &Collection) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.dir(collection))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
            .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
            .collect();
        names.sort();
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_read_remove_round_trip() {
        let p = Paths::temp();
        let s = FileStore::new(&p);
        let k = Key::Ask("w-1".into());
        assert!(!s.exists(&k));
        s.write_atomic(&k, "hello").unwrap();
        assert!(s.exists(&k));
        assert_eq!(s.read(&k).as_deref(), Some("hello"));
        s.remove(&k).unwrap();
        assert!(!s.exists(&k));
        // Removing an absent key is a no-op success.
        s.remove(&k).unwrap();
    }

    #[test]
    fn create_exclusive_is_a_test_and_set() {
        let p = Paths::temp();
        let s = FileStore::new(&p);
        let k = Key::Claim("repo".into());
        assert!(s.create_exclusive(&k, "first").unwrap(), "first wins");
        assert!(
            !s.create_exclusive(&k, "second").unwrap(),
            "second sees it already exists"
        );
        assert_eq!(s.read(&k).as_deref(), Some("first"), "loser never clobbers");
    }

    #[test]
    fn list_returns_sorted_stems() {
        let p = Paths::temp();
        let s = FileStore::new(&p);
        s.write_atomic(&Key::Claim("b".into()), "{}").unwrap();
        s.write_atomic(&Key::Claim("a".into()), "{}").unwrap();
        assert_eq!(s.list(&Collection::Claims), vec!["a", "b"]);
    }
}
