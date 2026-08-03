//! The persisted seam (Principle II): one JSON file under an exclusive lock, holding the
//! per-workspace focus history the toggle reads and writes. See data-model.md for the shape
//! and the concurrency argument this module implements.

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The whole of what the plugin remembers, keyed by herdr's workspace identifier.
///
/// `BTreeMap` rather than `HashMap`: a `HashMap`'s iteration order is randomised per process, so
/// two runs that make the identical change would serialize to different byte sequences and a
/// concurrency test could only compare parsed values, never raw files. `BTreeMap` keys sort, so
/// the same logical state always serializes identically — which is what lets the SC-009 test
/// diff `state.json` directly instead of parsing and re-comparing it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedState {
    pub workspaces: BTreeMap<String, WorkspaceHistory>,
}

/// One workspace's two-deep focus history. `last_tab_id` is the toggle target; `None` means the
/// workspace's current tab is known but it has never been switched (invariant 4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceHistory {
    pub current_tab_id: String,
    pub last_tab_id: Option<String>,
}

const STATE_FILE: &str = "state.json";
const LOCK_FILE: &str = "state.lock";

/// Loads `state.json` from `dir`, treating anything that isn't a clean read of the expected
/// shape as empty state. A missing file, an empty file, a file truncated mid-object, and JSON
/// that doesn't match `PersistedState` are all the same case here: unreadable content is empty
/// content (FR-014, invariant 5). There is no error path — a corrupt file must never stop the
/// plugin from running, and the next successful `persist` repairs it.
pub fn load(dir: &Path) -> PersistedState {
    fs::read(dir.join(STATE_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// An exclusive lock on `state.lock`, held for the caller's entire read-modify-write.
///
/// The lock is acquired well before any state is read and released only when this guard is
/// dropped — deliberately spanning the `tab focus` call in between, not just the file I/O. A
/// narrower lock (around the read, then again around the write) would let two overlapping
/// toggles both read `{ current: A, last: B }`, both resolve `B` as the target, both call
/// `tab focus B`, and both write `{ current: B, last: A }` — a well-formed file that is wrong,
/// because the user pressed the key twice and never came back to `A`. Serializing the herdr
/// round-trip itself is the cost that buys alternation its correctness under concurrency
/// (data-model.md#concurrency).
pub struct StateLock {
    file: File,
    dir: PathBuf,
}

impl StateLock {
    /// Creates the state directory and lock file if absent, then blocks until the exclusive
    /// lock is acquired.
    pub fn acquire(dir: &Path) -> std::io::Result<Self> {
        fs::create_dir_all(dir)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(dir.join(LOCK_FILE))?;
        file.lock_exclusive()?;
        Ok(Self {
            file,
            dir: dir.to_path_buf(),
        })
    }
}

impl Drop for StateLock {
    fn drop(&mut self) {
        // Best-effort: the OS also releases the lock when the file descriptor closes at
        // process exit, so a failure here (e.g. the fd already gone) has nothing left to do.
        let _ = FileExt::unlock(&self.file);
    }
}

/// Writes `state` durably and atomically, then keeps the lock held until `guard` is dropped by
/// the caller — this function does not release it, so the caller controls exactly how much of
/// the read-modify-write the lock spans.
///
/// Writing to a uniquely named temporary file in the same directory, `sync_all`-ing it, and then
/// `rename`-ing it over `state.json` is what makes the replacement atomic: a same-directory
/// `rename` is a single filesystem operation, so a concurrent reader (`load`, above) sees either
/// the old complete file or the new complete file and never a partial write.
pub fn persist(guard: &StateLock, state: &PersistedState) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(state)?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp_path = guard
        .dir
        .join(format!("state.{}.{nanos}.tmp", std::process::id()));

    let mut tmp = File::create(&tmp_path)?;
    tmp.write_all(&json)?;
    tmp.sync_all()?;
    drop(tmp);
    fs::rename(&tmp_path, guard.dir.join(STATE_FILE))?;

    // The rename is atomic for a concurrent *reader* the moment it returns, but it is a change to
    // the directory rather than to the file, so `tmp.sync_all()` above does not make it durable:
    // after power loss the entry can be missing while the file's contents are safely on disk.
    // Syncing the directory is what finishes the job the file sync starts — without it that first
    // sync buys nothing, since the data it flushed may have no name pointing at it.
    //
    // Both declared targets are POSIX, where opening a directory read-only and syncing it is the
    // supported way to do this. A crash here is not catastrophic — FR-014 makes a missing or
    // partial `state.json` a clean no-op that the next focus re-seeds — but the write protocol
    // claims durability, and a claim that holds only for the file half is worse than no claim.
    File::open(&guard.dir)?.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// A fresh directory under the system temp dir, unique per test so parallel `cargo test`
    /// runs never collide.
    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "herdr-last-tab-state-test-{label}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_state() -> PersistedState {
        let mut workspaces = BTreeMap::new();
        workspaces.insert(
            "wA".to_string(),
            WorkspaceHistory {
                current_tab_id: "wA:t8".to_string(),
                last_tab_id: Some("wA:t1".to_string()),
            },
        );
        PersistedState { workspaces }
    }

    #[test]
    fn missing_file_loads_as_empty_map() {
        let dir = temp_dir("missing");
        assert_eq!(load(&dir), PersistedState::default());
    }

    #[test]
    fn empty_file_loads_as_empty_map() {
        let dir = temp_dir("empty");
        fs::write(dir.join(STATE_FILE), b"").unwrap();
        assert_eq!(load(&dir), PersistedState::default());
    }

    #[test]
    fn non_json_file_loads_as_empty_map() {
        let dir = temp_dir("not-json");
        fs::write(dir.join(STATE_FILE), b"not json").unwrap();
        assert_eq!(load(&dir), PersistedState::default());
    }

    #[test]
    fn truncated_mid_object_loads_as_empty_map() {
        let dir = temp_dir("truncated");
        let full = serde_json::to_vec(&sample_state()).unwrap();
        fs::write(dir.join(STATE_FILE), &full[..full.len() / 2]).unwrap();
        assert_eq!(load(&dir), PersistedState::default());
    }

    #[test]
    fn corrupt_state_is_repaired_by_the_next_write() {
        let dir = temp_dir("repair");
        fs::write(dir.join(STATE_FILE), b"not json").unwrap();
        let guard = StateLock::acquire(&dir).unwrap();
        persist(&guard, &sample_state()).unwrap();
        drop(guard);
        assert_eq!(load(&dir), sample_state());
    }

    #[test]
    fn persist_produces_valid_json_with_stable_key_order() {
        let dir = temp_dir("stable-order");
        let mut workspaces = BTreeMap::new();
        workspaces.insert(
            "wZ".to_string(),
            WorkspaceHistory {
                current_tab_id: "wZ:t1".to_string(),
                last_tab_id: None,
            },
        );
        workspaces.insert(
            "wA".to_string(),
            WorkspaceHistory {
                current_tab_id: "wA:t1".to_string(),
                last_tab_id: None,
            },
        );
        let state = PersistedState { workspaces };

        let guard = StateLock::acquire(&dir).unwrap();
        persist(&guard, &state).unwrap();
        drop(guard);

        let raw = fs::read_to_string(dir.join(STATE_FILE)).unwrap();
        let wa = raw.find("\"wA\"").expect("wA present");
        let wz = raw.find("\"wZ\"").expect("wZ present");
        assert!(wa < wz, "keys must serialize in sorted order");

        // A second run producing the same logical state must produce byte-identical output.
        let dir2 = temp_dir("stable-order-2");
        let guard2 = StateLock::acquire(&dir2).unwrap();
        persist(&guard2, &state).unwrap();
        drop(guard2);
        let raw2 = fs::read_to_string(dir2.join(STATE_FILE)).unwrap();
        assert_eq!(raw, raw2);
    }

    #[test]
    fn persist_creates_no_file_outside_the_state_directory() {
        let dir = temp_dir("no-stray-files");
        let guard = StateLock::acquire(&dir).unwrap();
        persist(&guard, &sample_state()).unwrap();
        drop(guard);

        let entries: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        // Only the lock file and the final state file remain — the temporary file was renamed
        // away, not left behind.
        assert_eq!(entries.len(), 2, "unexpected entries: {entries:?}");
        assert!(entries.contains(&STATE_FILE.to_string()));
        assert!(entries.contains(&LOCK_FILE.to_string()));
    }
}
