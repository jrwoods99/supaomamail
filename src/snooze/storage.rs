//! Where snoozes are kept. One private file in the state directory, rewritten
//! whole under a short cross-process lock, and a separate lease that says which
//! process may wake mail. Tests keep the same file in memory.
use serde_json::{Value, json};
use std::{
    io::Read,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

type Result<T> = std::result::Result<T, &'static str>;

const LIMIT: u64 = 8 * 1024 * 1024;
const FILE: &str = "snooze.json";
const LOCK: &str = ".snooze.lock";
const LEASE: &str = "snooze-scheduler.lock";

pub(super) fn empty() -> Value {
    json!({"version":1,"labels":{},"entries":[]})
}

#[derive(Default)]
pub(super) struct Memory {
    value: Option<Value>,
    leased: bool,
}

pub(super) enum Store {
    /// The state directory, or an explicit root.
    Disk(Option<PathBuf>),
    /// The same file held in memory, shared between the processes a test plays.
    #[cfg_attr(not(test), allow(dead_code))]
    Memory(Arc<Mutex<Memory>>),
}

/// Held for as long as this process may wake mail.
pub(super) enum Lease {
    Disk(#[allow(dead_code)] crate::platform::private_fs::ExclusiveLock),
    Memory(Arc<Mutex<Memory>>),
}

impl Drop for Lease {
    fn drop(&mut self) {
        if let Lease::Memory(memory) = self {
            memory.lock().unwrap_or_else(|e| e.into_inner()).leased = false;
        }
    }
}

fn directory(root: &Option<PathBuf>) -> Result<std::fs::File> {
    let root = match root {
        Some(root) => root.clone(),
        None => crate::platform::dirs::AppDirs::discover()?.state,
    };
    crate::cache::directories(&root, &["omamail"], true)
        .map_err(|error| match error {
            "cache_unsafe_path" => "snooze_storage_unsafe",
            _ => "snooze_storage_unavailable",
        })?
        .ok_or("snooze_storage_unavailable")
}

fn read_file(dir: &std::fs::File) -> Result<Value> {
    let Some(file) =
        crate::cache::regular(dir, FILE, false).map_err(|_| "snooze_storage_unavailable")?
    else {
        return Ok(empty());
    };
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "snooze_storage_unavailable")?;
    if bytes.len() as u64 > LIMIT {
        return Err("snooze_storage_too_large");
    }
    serde_json::from_slice(&bytes).map_err(|_| "snooze_storage_invalid")
}

fn write_file(dir: &std::fs::File, value: &Value) -> Result<()> {
    let bytes = serde_json::to_vec(value).map_err(|_| "snooze_storage_invalid")?;
    if bytes.len() as u64 > LIMIT {
        return Err("snooze_storage_too_large");
    }
    crate::platform::private_fs::atomic_replace(dir, FILE, &bytes)
        .map_err(|_| "snooze_storage_unavailable")
}

// The file lock is non-blocking, and whoever holds it holds it for one short
// rewrite, so a busy lock is waited out rather than reported.
fn lock(dir: &std::fs::File) -> Result<crate::platform::private_fs::ExclusiveLock> {
    for _ in 0..100 {
        match crate::platform::private_fs::lock_exclusive(dir, LOCK) {
            Err("private_fs_busy") => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => return Err("snooze_storage_unavailable"),
            Ok(lock) => return Ok(lock),
        }
    }
    Err("snooze_storage_busy")
}

impl Store {
    pub(super) async fn read(&self) -> Result<Value> {
        match self {
            Store::Memory(memory) => Ok(memory
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .value
                .clone()
                .unwrap_or_else(empty)),
            Store::Disk(root) => {
                let root = root.clone();
                tokio::task::spawn_blocking(move || {
                    let dir = directory(&root)?;
                    let _lock = lock(&dir)?;
                    read_file(&dir)
                })
                .await
                .map_err(|_| "snooze_storage_unavailable")?
            }
        }
    }

    /// Read, change and write back under the file's lock, so another process
    /// adding a snooze at the same moment is never written over. `change`
    /// returns an error to write nothing.
    pub(super) async fn update<T, F>(&self, change: F) -> Result<T>
    where
        F: FnOnce(&mut Value) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        match self {
            Store::Memory(memory) => {
                let mut memory = memory.lock().unwrap_or_else(|e| e.into_inner());
                let mut value = memory.value.clone().unwrap_or_else(empty);
                let answer = change(&mut value)?;
                memory.value = Some(value);
                Ok(answer)
            }
            Store::Disk(root) => {
                let root = root.clone();
                tokio::task::spawn_blocking(move || {
                    let dir = directory(&root)?;
                    let _lock = lock(&dir)?;
                    let mut value = read_file(&dir)?;
                    let before = value.clone();
                    let answer = change(&mut value)?;
                    if value != before {
                        write_file(&dir, &value)?;
                    }
                    Ok(answer)
                })
                .await
                .map_err(|_| "snooze_storage_unavailable")?
            }
        }
    }

    /// The scheduler lease, or none while another process holds it.
    pub(super) async fn lease(&self) -> Result<Option<Lease>> {
        match self {
            Store::Memory(memory) => {
                let mut held = memory.lock().unwrap_or_else(|e| e.into_inner());
                if held.leased {
                    return Ok(None);
                }
                held.leased = true;
                Ok(Some(Lease::Memory(memory.clone())))
            }
            Store::Disk(root) => {
                let root = root.clone();
                tokio::task::spawn_blocking(move || {
                    let dir = directory(&root)?;
                    match crate::platform::private_fs::lock_exclusive(&dir, LEASE) {
                        Ok(lease) => Ok(Some(Lease::Disk(lease))),
                        Err("private_fs_busy") => Ok(None),
                        Err(_) => Err("snooze_storage_unavailable"),
                    }
                })
                .await
                .map_err(|_| "snooze_storage_unavailable")?
            }
        }
    }
}
