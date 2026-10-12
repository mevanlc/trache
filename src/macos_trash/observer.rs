use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use objc2_foundation::NSString;
use trash::macos::{FileIdentity, TrashReceipt, delete_with_file_manager};

use super::ds_store::{self, PutBack};

const INTERVAL: Duration = Duration::from_millis(500);
const MINIMUM: Duration = Duration::from_secs(3);
const LIMIT: Duration = Duration::from_secs(10);

#[derive(Default)]
pub(super) struct Batch {
    receipts: Vec<TrashReceipt>,
    last_success: Option<Instant>,
}

impl Batch {
    #[cfg(test)]
    pub(super) fn receipts(&self) -> &[TrashReceipt] {
        &self.receipts
    }

    pub(super) fn delete(&mut self, path: &Path) -> Result<(), trash::Error> {
        let receipt = delete_with_file_manager(path)?;
        self.last_success = Some(Instant::now());
        self.receipts.push(receipt);
        Ok(())
    }

    pub(super) fn settle(self) {
        let Some(last) = self.last_success else {
            return;
        };
        let start = Instant::now();
        let minimum = MINIMUM.saturating_sub(start.duration_since(last));
        let (done, receiver) = mpsc::sync_channel(1);
        // The owner enforces the deadline independently of filesystem reads. A stalled
        // reader is never joined; main exits immediately after settling, ending its threads.
        let spawned = thread::Builder::new()
            .name("trash-metadata".into())
            .spawn(move || {
                wait_loop(
                    minimum,
                    LIMIT,
                    || start.elapsed(),
                    thread::sleep,
                    || objc2::rc::autoreleasepool(|_| complete(&self.receipts)),
                );
                let _ = done.send(());
            });
        if spawned.is_ok() {
            if receiver.recv_timeout(LIMIT.saturating_sub(start.elapsed()))
                == Err(mpsc::RecvTimeoutError::Disconnected)
            {
                // A failed observer is inconclusive, not evidence that the flush completed.
                thread::sleep(LIMIT.saturating_sub(start.elapsed()));
            }
        } else {
            // Even thread exhaustion must leave Apple's dispatch workers time to flush.
            thread::sleep(LIMIT.saturating_sub(start.elapsed()));
        }
    }
}

fn wait_loop(
    minimum: Duration,
    limit: Duration,
    mut elapsed: impl FnMut() -> Duration,
    mut sleep: impl FnMut(Duration),
    mut observe: impl FnMut() -> bool,
) {
    loop {
        if elapsed() >= limit {
            return;
        }
        let complete = observe();
        let now = elapsed();
        if now >= limit || (complete && now >= minimum) {
            return;
        }
        sleep(INTERVAL.min(limit - now));
    }
}

type Store = HashMap<String, Option<PutBack>>;

fn read_store(path: &Path) -> Option<Store> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    let before = file.metadata().ok()?;
    if !before.is_file() || before.len() > ds_store::MAX_BYTES as u64 {
        return None;
    }
    let mut data = Vec::new();
    (&file)
        .take(ds_store::MAX_BYTES as u64 + 1)
        .read_to_end(&mut data)
        .ok()?;
    let after = file.metadata().ok()?;
    let current = fs::symlink_metadata(path).ok()?;
    if FileIdentity::from_metadata(&before) != FileIdentity::from_metadata(&current)
        || before.len() != after.len()
        || (before.mtime(), before.mtime_nsec()) != (after.mtime(), after.mtime_nsec())
    {
        return None;
    }
    let mut index = Store::new();
    for (name, record) in ds_store::parse(&data).ok()? {
        // Normalize each key once per store, rather than scanning and normalizing the
        // whole Trash for every receipt. Ambiguous equivalent keys cannot confirm a move.
        index
            .entry(normalized(&name))
            .and_modify(|entry| *entry = None)
            .or_insert(Some(record));
    }
    Some(index)
}

#[derive(PartialEq, Eq)]
enum Presence {
    Present,
    Gone,
    Unknown,
}

fn presence(receipt: &TrashReceipt) -> Presence {
    let Some(path) = &receipt.trashed_path else {
        return Presence::Unknown;
    };
    match fs::symlink_metadata(path) {
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            Presence::Gone
        }
        Ok(metadata) => match receipt.trashed_identity {
            Some(identity) if identity == FileIdentity::from_metadata(&metadata) => {
                Presence::Present
            }
            Some(_) => Presence::Gone,
            None => Presence::Unknown,
        },
        Err(_) => Presence::Unknown,
    }
}

fn complete(receipts: &[TrashReceipt]) -> bool {
    let mut stores = HashMap::new();
    let mut complete = true;
    for receipt in receipts {
        match presence(receipt) {
            Presence::Gone => continue,
            Presence::Unknown => {
                complete = false;
                continue;
            }
            Presence::Present => {}
        }
        let destination = receipt
            .trashed_path
            .as_ref()
            .expect("present item has a destination");
        let Some((directory, name)) = destination
            .parent()
            .zip(destination.file_name().and_then(|n| n.to_str()))
        else {
            complete = false;
            continue;
        };
        let store = stores
            .entry(directory)
            .or_insert_with(|| read_store(&directory.join(".DS_Store")));
        let found = store
            .as_ref()
            .and_then(|store| store.get(&normalized(name)))
            .and_then(Option::as_ref);
        complete &= found.is_some_and(|record| matches_record(receipt, record));
    }
    complete
}

fn normalized(value: &str) -> String {
    NSString::from_str(value)
        .precomposedStringWithCanonicalMapping()
        .to_string()
}

fn matches_record(receipt: &TrashReceipt, record: &PutBack) -> bool {
    let Some((name, location)) = record.name.as_deref().zip(record.location.as_deref()) else {
        return false;
    };
    let Some(original_name) = receipt.original_path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if normalized(name) != normalized(original_name) {
        return false;
    }
    let Some(parent) = receipt.original_path.parent() else {
        return false;
    };
    if Path::new(location)
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return false;
    }
    let relative = location.trim_start_matches('/');
    let mut candidates = vec![Path::new("/").join(relative)];
    if let Some(root) = &receipt.volume_root {
        candidates.push(root.join(relative));
    }
    candidates.into_iter().any(|candidate| {
        if equivalent_path(&candidate, parent) {
            return true;
        }
        if let Some(identity) = receipt.original_parent_identity
            && fs::metadata(&candidate)
                .ok()
                .as_ref()
                .map(FileIdentity::from_metadata)
                == Some(identity)
        {
            return true;
        }
        // Only apply the boot Data volume alias when the recorded source volume is that
        // volume. An external volume may legitimately contain System/Volumes/Data.
        if receipt.volume_root.as_deref() == Some(Path::new("/System/Volumes/Data")) {
            let visible = |p: &Path| -> PathBuf {
                p.strip_prefix("/System/Volumes/Data")
                    .map(|p| Path::new("/").join(p))
                    .unwrap_or_else(|_| p.to_owned())
            };
            return equivalent_path(&visible(&candidate), &visible(parent));
        }
        false
    })
}

fn equivalent_path(a: &Path, b: &Path) -> bool {
    a.to_str().zip(b.to_str()).is_some_and(|(a, b)| {
        // No case folding: case-sensitive volumes can contain distinct names.
        normalized(a.trim_end_matches('/')) == normalized(b.trim_end_matches('/'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn wait_for(observations: impl Fn(Duration) -> bool) -> Duration {
        let clock = Cell::new(Duration::ZERO);
        wait_loop(
            MINIMUM,
            LIMIT,
            || clock.get(),
            |d| clock.set(clock.get() + d),
            || observations(clock.get()),
        );
        clock.get()
    }

    #[test]
    fn polling_obeys_minimum_delayed_completion_and_deadline() {
        assert_eq!(wait_for(|_| true), MINIMUM);
        assert_eq!(
            wait_for(|t| t >= Duration::from_secs(4)),
            Duration::from_secs(4)
        );
        assert_eq!(wait_for(|_| false), LIMIT);
        // A match observed only before the minimum is not latched forever.
        assert_eq!(wait_for(|t| t < MINIMUM), LIMIT);
    }

    fn receipt(path: &Path) -> TrashReceipt {
        TrashReceipt {
            original_path: PathBuf::from("/Users/example/雪.txt"),
            original_parent_identity: None,
            volume_root: Some(PathBuf::from("/System/Volumes/Data")),
            trashed_path: Some(path.to_owned()),
            trashed_identity: fs::symlink_metadata(path)
                .ok()
                .as_ref()
                .map(FileIdentity::from_metadata),
        }
    }

    #[test]
    fn missing_receipts_and_replacements_are_distinct() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("item");
        fs::write(&path, "first").unwrap();
        let receipt = receipt(&path);
        assert!(presence(&receipt) == Presence::Present);
        fs::rename(&path, dir.path().join("restored")).unwrap();
        assert!(complete(std::slice::from_ref(&receipt)));
        fs::write(&path, "replacement").unwrap();
        assert!(complete(std::slice::from_ref(&receipt)));
        let mut unknown = receipt;
        unknown.trashed_path = None;
        assert!(!complete(&[unknown]));
    }

    #[test]
    fn dangling_symlinks_are_present_and_store_reads_never_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("link");
        std::os::unix::fs::symlink("missing", &path).unwrap();
        assert!(presence(&receipt(&path)) == Presence::Present);
        let store = dir.path().join(".DS_Store");
        let fixture = include_bytes!("../../tests/fixtures/put_back.ds_store");
        fs::write(&store, fixture).unwrap();
        assert!(read_store(&store).is_some());
        assert_eq!(fs::read(&store).unwrap(), fixture);
        fs::write(&store, b"truncated").unwrap();
        assert!(read_store(&store).is_none());
    }

    #[test]
    fn matches_volume_paths_unicode_and_requires_both_fields() {
        let mut receipt = receipt(Path::new("/nonexistent"));
        let mut record = PutBack {
            name: Some("雪.txt".into()),
            location: Some("System/Volumes/Data/Users/example/".into()),
        };
        assert!(matches_record(&receipt, &record));
        receipt.original_path = PathBuf::from("/Volumes/External/work/café.txt");
        receipt.volume_root = Some(PathBuf::from("/Volumes/External"));
        record.name = Some("cafe\u{301}.txt".into());
        record.location = Some("work/".into());
        assert!(matches_record(&receipt, &record));
        record.name = Some("CAFÉ.txt".into());
        assert!(!matches_record(&receipt, &record));
        record.name = None;
        assert!(!matches_record(&receipt, &record));
    }

    #[test]
    fn observes_each_actual_trash_directory_and_retries_incomplete_stores() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let fixture = include_bytes!("../../tests/fixtures/put_back.ds_store");
        let mut receipts = Vec::new();
        for directory in [a.path(), b.path()] {
            let item = directory.join("雪.txt");
            fs::write(&item, "owned observer fixture").unwrap();
            fs::write(directory.join(".DS_Store"), fixture).unwrap();
            receipts.push(receipt(&item));
        }
        assert!(complete(&receipts));
        fs::write(b.path().join(".DS_Store"), &fixture[..32]).unwrap();
        assert!(!complete(&receipts));
        fs::write(b.path().join(".DS_Store"), fixture).unwrap();
        assert!(complete(&receipts));
        receipts[1].original_path = PathBuf::from("/somewhere-else/雪.txt");
        assert!(!complete(&receipts));
    }
}
