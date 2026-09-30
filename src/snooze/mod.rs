//! Snoozed mail: a message leaves the Inbox now and comes back at a time the
//! user chose.
//!
//! Gmail's API has no snooze, so the backend keeps the promise itself.
//! Snoozing takes INBOX off the message and files it under a label of our own,
//! `Omamail/Snoozed`, which is what the Snoozed mailbox lists and what Gmail's
//! own web app shows; when it comes back is kept here, in a private file in the
//! state directory. At that time the worker puts INBOX back and marks the
//! message unread, then says so with `snooze.changed`.
//!
//! Only the process holding the scheduler lease wakes mail, so a plugin and a
//! standalone window open at once never wake one message twice. Either can
//! snooze: every write takes the file's own short lock, and the owner reads the
//! file again on every tick of at most a minute. A process without the lease
//! asks for it on each of its own ticks, so when the owner exits another one
//! carries on. Nothing is read and no worker starts until the first `snooze.*`
//! call, so a backend nobody asks about snoozes never touches the file, and a
//! test process never wakes real mail.
//!
//! What a wake does is decided by the message, not the label. One somebody
//! already moved back to the Inbox elsewhere only loses the label; one they
//! deleted or reported is left alone; one Gmail no longer has is forgotten.
//! Anything else that fails is tried again later and never given up on, because
//! the whole promise is that the mail comes back.
use futures_util::future::BoxFuture;
use serde_json::{Value, json};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Notify, broadcast};

mod storage;
#[cfg(test)]
mod tests;

type Result<T> = std::result::Result<T, &'static str>;

/// The label a snoozed Gmail message is filed under while it is away.
pub const LABEL: &str = "Omamail/Snoozed";

const MAX_ENTRIES: usize = 5000;
const MAX_IDS: usize = 1000;
/// A snooze shorter than this would wake before it had settled.
const MIN_LEAD_MS: u64 = 30_000;
const MAX_AHEAD_MS: u64 = 5 * 366 * 24 * 3600 * 1000;
/// The longest the worker sleeps before reading the clock and the file again:
/// a machine that slept through a wake time, or another process's new snooze,
/// is noticed within a minute.
const TICK_MS: u64 = 60_000;
/// A park interrupted by a crash is looked at again after this long.
const PARKING_GRACE_MS: u64 = 120_000;

/// One Gmail call, settled: the backend's own queued session in production.
pub type Gmail =
    Arc<dyn Fn(&'static str, Value) -> BoxFuture<'static, Result<Value>> + Send + Sync>;
pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

fn system_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn account_of(value: Option<&Value>) -> Result<String> {
    let text = value.and_then(Value::as_str).ok_or("invalid_params")?;
    if text.is_empty() || text.len() > 320 || text.chars().any(char::is_control) {
        return Err("invalid_params");
    }
    Ok(text.to_lowercase())
}

// Gmail message ids are short hexadecimal strings. Anything else is refused
// before a request is built from it.
fn ids_of(value: Option<&Value>) -> Result<Vec<String>> {
    let list = value.and_then(Value::as_array).ok_or("invalid_params")?;
    if list.is_empty() || list.len() > MAX_IDS {
        return Err("invalid_params");
    }
    let mut out: Vec<String> = Vec::new();
    for id in list {
        let id = id.as_str().ok_or("invalid_params")?;
        if id.is_empty() || id.len() > 128 || !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err("invalid_params");
        }
        if !out.iter().any(|seen| seen == id) {
            out.push(id.to_owned());
        }
    }
    Ok(out)
}

fn entries(store: &Value) -> Vec<Value> {
    store["entries"].as_array().cloned().unwrap_or_default()
}

fn text(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}

fn same(entry: &Value, account: &str, id: &str) -> bool {
    entry["accountId"] == account && entry["messageId"] == id
}

/// Refuse a file somebody else wrote, or this build did not: every field the
/// worker reads is checked before it is trusted.
fn validate(store: &Value) -> Result<()> {
    let bad = Err("snooze_storage_invalid");
    if store["version"] != 1 || !store["labels"].is_object() {
        return bad;
    }
    for (account, label) in store["labels"].as_object().unwrap() {
        let label = label.as_str().unwrap_or("");
        if account.is_empty()
            || label.is_empty()
            || label.len() > 256
            || label.chars().any(char::is_control)
        {
            return bad;
        }
    }
    let Some(list) = store["entries"].as_array() else {
        return bad;
    };
    if list.len() > MAX_ENTRIES {
        return bad;
    }
    for entry in list {
        account_of(entry.get("accountId")).map_err(|_| "snooze_storage_invalid")?;
        ids_of(Some(&json!([entry["messageId"].clone()]))).map_err(|_| "snooze_storage_invalid")?;
        if entry["wakeAt"].as_u64().is_none()
            || entry["snoozedAt"].as_u64().is_none()
            || !matches!(text(&entry["state"]), "parking" | "pending" | "waking")
        {
            return bad;
        }
    }
    Ok(())
}

fn backoff(attempts: u64) -> u64 {
    match attempts {
        0 | 1 => 60_000,
        2 => 300_000,
        3 => 900_000,
        _ => 3_600_000,
    }
}

/// Whether an entry is waiting on the clock at `now`.
fn due(entry: &Value, now: u64) -> bool {
    let retry = entry["retryAt"].as_u64().unwrap_or(0);
    if retry > now {
        return false;
    }
    match text(&entry["state"]) {
        "parking" => entry["snoozedAt"].as_u64().unwrap_or(0) + PARKING_GRACE_MS <= now,
        _ => entry["wakeAt"].as_u64().unwrap_or(u64::MAX) <= now,
    }
}

fn next_due(store: &Value) -> Option<u64> {
    entries(store)
        .iter()
        .map(|entry| {
            let when = if entry["state"] == "parking" {
                entry["snoozedAt"].as_u64().unwrap_or(0) + PARKING_GRACE_MS
            } else {
                entry["wakeAt"].as_u64().unwrap_or(u64::MAX)
            };
            when.max(entry["retryAt"].as_u64().unwrap_or(0))
        })
        .min()
}

fn projection(entry: &Value) -> Value {
    json!({"accountId":entry["accountId"],"messageId":entry["messageId"],"wakeAt":entry["wakeAt"],"state":entry["state"]})
}

fn label_ids(resource: &Value) -> Vec<String> {
    resource["labelIds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

pub struct Snoozer {
    inner: Arc<Inner>,
}

struct Inner {
    gmail: Gmail,
    clock: Clock,
    store: storage::Store,
    /// Whether holding the lease starts the worker; tests drive `tick` by hand.
    spawn_worker: bool,
    events: broadcast::Sender<Value>,
    wake: Notify,
    stopping: AtomicBool,
    revision: AtomicU64,
    worker: Mutex<Worker>,
}

#[derive(Default)]
struct Worker {
    started: bool,
    lease: Option<storage::Lease>,
    job: Option<tokio::task::JoinHandle<()>>,
}

impl Snoozer {
    pub fn new(gmail: Gmail) -> Self {
        Self::with(
            gmail,
            Arc::new(system_now),
            storage::Store::Disk(None),
            true,
        )
    }

    fn with(gmail: Gmail, clock: Clock, store: storage::Store, spawn_worker: bool) -> Self {
        // Built with the session, before any frame is read, so the protocol
        // can subscribe to it; everything that reads the file waits for a call.
        let (events, _) = broadcast::channel(64);
        Self {
            inner: Arc::new(Inner {
                gmail,
                clock,
                store,
                spawn_worker,
                events,
                wake: Notify::new(),
                stopping: AtomicBool::new(false),
                revision: AtomicU64::new(0),
                worker: Mutex::new(Worker::default()),
            }),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.inner.events.subscribe()
    }

    pub async fn call(&self, method: &str, params: &Value) -> Result<Value> {
        self.inner.call(method, params).await
    }

    /// Stops the worker before the Gmail queue it sends through goes away. A
    /// wake cut off here is still `waking` in the file and is sent again by
    /// whichever process wakes mail next.
    pub async fn shutdown(&self) {
        self.inner.stopping.store(true, Ordering::Release);
        self.inner.wake.notify_waiters();
        let (job, lease) = {
            let mut worker = self.inner.worker.lock().unwrap_or_else(|e| e.into_inner());
            (worker.job.take(), worker.lease.take())
        };
        if let Some(job) = job {
            job.abort();
            let _ = job.await;
        }
        drop(lease);
    }
}

impl Inner {
    async fn call(self: &Arc<Self>, method: &str, params: &Value) -> Result<Value> {
        let fields = params.as_object().ok_or("invalid_params")?;
        let allowed: &[&str] = match method {
            "snooze.snapshot" => &["accountId"],
            "snooze.set" => &["accountId", "ids", "wakeAt"],
            "snooze.cancel" | "snooze.wake" => &["accountId", "ids"],
            _ => return Err("unknown_method"),
        };
        if fields.keys().any(|key| !allowed.contains(&key.as_str())) {
            return Err("invalid_params");
        }
        if method == "snooze.snapshot" {
            let account = match params.get("accountId") {
                None => None,
                some => Some(account_of(some)?),
            };
            self.start().await;
            return self.snapshot(account.as_deref()).await;
        }
        let account = account_of(params.get("accountId"))?;
        let ids = ids_of(params.get("ids"))?;
        let wake_at = if method == "snooze.set" {
            let now = (self.clock)();
            let at = params["wakeAt"].as_u64().ok_or("invalid_params")?;
            if at < now + MIN_LEAD_MS || at > now + MAX_AHEAD_MS {
                return Err("invalid_params");
            }
            at
        } else {
            0
        };
        self.start().await;
        match method {
            "snooze.set" => self.set(&account, &ids, wake_at).await,
            "snooze.cancel" => self.cancel(&account, &ids).await,
            _ => self.wake_now(&account, &ids).await,
        }
    }

    async fn read(&self) -> Result<Value> {
        let store = self.store.read().await?;
        validate(&store)?;
        Ok(store)
    }

    /// Every change is made to a file already known to be sound, so nothing
    /// here writes into a shape it did not expect.
    async fn update<T, F>(&self, change: F) -> Result<T>
    where
        F: FnOnce(&mut Value) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        self.store
            .update(move |store| {
                validate(store)?;
                change(store)
            })
            .await
    }

    /// The first call starts the worker, which runs whether or not this
    /// process may wake mail yet.
    async fn start(self: &Arc<Self>) {
        if self
            .worker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .started
        {
            return;
        }
        self.hold().await;
        let mut worker = self.worker.lock().unwrap_or_else(|e| e.into_inner());
        if worker.started || self.stopping.load(Ordering::Acquire) {
            return;
        }
        worker.started = true;
        if self.spawn_worker {
            let inner = self.clone();
            worker.job = Some(tokio::spawn(async move { inner.run().await }));
        }
    }

    /// Whether this process wakes mail, taking the lease if it is free.
    async fn hold(&self) -> bool {
        if self
            .worker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .lease
            .is_some()
        {
            return true;
        }
        let Ok(Some(lease)) = self.store.lease().await else {
            return false;
        };
        let mut worker = self.worker.lock().unwrap_or_else(|e| e.into_inner());
        if self.stopping.load(Ordering::Acquire) {
            return false;
        }
        worker.lease = Some(lease);
        true
    }

    async fn snapshot(&self, account: Option<&str>) -> Result<Value> {
        let store = self.read().await?;
        let listed: Vec<Value> = entries(&store)
            .iter()
            .filter(|entry| account.is_none_or(|account| entry["accountId"] == account))
            .map(projection)
            .collect();
        Ok(json!({"revision":self.revision.load(Ordering::Acquire).to_string(),"entries":listed}))
    }

    fn changed(&self, account: &str, woken: Vec<Value>) {
        let revision = self.revision.fetch_add(1, Ordering::AcqRel) + 1;
        let _ = self.events.send(json!({"jsonrpc":"2.0","method":"snooze.changed","params":{"accountId":account,"revision":revision.to_string(),"woken":woken}}));
    }

    /// The Snoozed label's id for an account: remembered, else found by name,
    /// else made. A second process making it at the same moment is answered by
    /// looking again.
    async fn label(&self, account: &str, fresh: bool) -> Result<String> {
        if !fresh {
            let store = self.read().await?;
            if let Some(id) = store["labels"][account].as_str() {
                return Ok(id.to_owned());
            }
        }
        let find = |labels: &Value| {
            labels
                .as_array()
                .into_iter()
                .flatten()
                .find(|label| label["rawName"] == LABEL)
                .and_then(|label| label["id"].as_str())
                .map(str::to_owned)
        };
        let listed = (self.gmail)("gmail.labels", json!({"accountId":account})).await?;
        let id = match find(&listed) {
            Some(id) => id,
            None => match (self.gmail)(
                "gmail.createLabel",
                json!({"accountId":account,"name":LABEL}),
            )
            .await
            {
                Ok(made) => made["id"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .map(str::to_owned)
                    .ok_or("snooze_label_unavailable")?,
                Err(_) => {
                    let again = (self.gmail)("gmail.labels", json!({"accountId":account})).await?;
                    find(&again).ok_or("snooze_label_unavailable")?
                }
            },
        };
        if id.is_empty() || id.len() > 256 || id.chars().any(char::is_control) {
            return Err("snooze_label_unavailable");
        }
        let account = account.to_owned();
        let remembered = id.clone();
        self.update(move |store| {
            store["labels"][account] = json!(remembered);
            Ok(())
        })
        .await?;
        Ok(id)
    }

    /// One labelled change to several messages, tried once more with the
    /// label looked up afresh if the remembered one was deleted in Gmail.
    /// Gmail answers a label id it no longer has as a bad request.
    async fn relabel(&self, account: &str, ids: &[String], add_label: bool) -> Result<()> {
        for fresh in [false, true] {
            let label = self.label(account, fresh).await?;
            let (add, remove) = if add_label {
                (vec![label], vec!["INBOX".to_owned()])
            } else {
                (vec!["INBOX".to_owned()], vec![label])
            };
            let answer = (self.gmail)(
                "gmail.batchModify",
                json!({"accountId":account,"ids":ids,"addLabelIds":add,"removeLabelIds":remove}),
            )
            .await;
            match answer {
                Err("gmail_http_failed" | "gmail_not_found") if !fresh => continue,
                other => return other.map(|_| ()),
            }
        }
        Err("snooze_label_unavailable")
    }

    async fn set(&self, account: &str, ids: &[String], wake_at: u64) -> Result<Value> {
        let now = (self.clock)();
        // Written before Gmail is asked, so a crash between the two is found
        // and settled by the worker rather than lost.
        let (owned, wanted) = (account.to_owned(), ids.to_vec());
        let replaced = self
            .update(move |store| {
                let (replaced, mut next): (Vec<Value>, Vec<Value>) = entries(store)
                    .into_iter()
                    .partition(|entry| wanted.iter().any(|id| same(entry, &owned, id)));
                for id in &wanted {
                    next.push(json!({"accountId":owned,"messageId":id,"wakeAt":wake_at,"snoozedAt":now,"state":"parking","attempts":0,"retryAt":0}));
                }
                if next.len() > MAX_ENTRIES {
                    return Err("snooze_too_many");
                }
                store["entries"] = json!(next);
                Ok(replaced)
            })
            .await?;
        if let Err(error) = self.relabel(account, ids, true).await {
            // Nothing moved in Gmail as far as anyone can prove, so these
            // messages go back to what the file said about them before.
            // Anything another process wrote meanwhile is left as it is.
            let (owned, wanted) = (account.to_owned(), ids.to_vec());
            let _ = self
                .update(move |store| {
                    let mut list: Vec<Value> = entries(store)
                        .into_iter()
                        .filter(|entry| {
                            !(wanted.iter().any(|id| same(entry, &owned, id))
                                && entry["state"] == "parking"
                                && entry["snoozedAt"] == now
                                && entry["wakeAt"] == wake_at)
                        })
                        .collect();
                    for old in replaced {
                        let (account, id) = (text(&old["accountId"]), text(&old["messageId"]));
                        if !list.iter().any(|entry| same(entry, account, id)) {
                            list.push(old);
                        }
                    }
                    store["entries"] = json!(list);
                    Ok(())
                })
                .await;
            return Err(error);
        }
        let (owned, wanted) = (account.to_owned(), ids.to_vec());
        self.update(move |store| {
            let mut list = entries(store);
            for entry in list.iter_mut() {
                if wanted.iter().any(|id| same(entry, &owned, id)) && entry["wakeAt"] == wake_at {
                    entry["state"] = json!("pending");
                }
            }
            store["entries"] = json!(list);
            Ok(())
        })
        .await?;
        self.changed(account, vec![]);
        self.wake.notify_one();
        self.snapshot(Some(account)).await
    }

    /// Back to the Inbox now, read or unread as it was: no reminder and no
    /// unread mark, because this is the snooze being taken back rather than
    /// ending. A message snoozed from outside the Inbox comes back into it
    /// too; the file keeps no record of where each one was.
    async fn cancel(&self, account: &str, ids: &[String]) -> Result<Value> {
        self.relabel(account, ids, false).await?;
        let (owned, wanted) = (account.to_owned(), ids.to_vec());
        self.update(move |store| {
            let list: Vec<Value> = entries(store)
                .into_iter()
                .filter(|entry| !wanted.iter().any(|id| same(entry, &owned, id)))
                .collect();
            store["entries"] = json!(list);
            Ok(())
        })
        .await?;
        self.changed(account, vec![]);
        Ok(json!({"cancelled":ids}))
    }

    /// Wake these now, through the worker, the way the clock would have.
    async fn wake_now(&self, account: &str, ids: &[String]) -> Result<Value> {
        let now = (self.clock)();
        let (owned, wanted) = (account.to_owned(), ids.to_vec());
        self.update(move |store| {
            let mut list = entries(store);
            for entry in list.iter_mut() {
                if wanted.iter().any(|id| same(entry, &owned, id)) && entry["state"] != "parking" {
                    entry["wakeAt"] = json!(now);
                    entry["retryAt"] = json!(0);
                }
            }
            store["entries"] = json!(list);
            Ok(())
        })
        .await?;
        self.wake.notify_one();
        self.snapshot(Some(account)).await
    }

    async fn run(self: Arc<Self>) {
        while !self.stopping.load(Ordering::Acquire) {
            let next = self.tick().await;
            let now = (self.clock)();
            let delay = next.map_or(TICK_MS, |at| at.saturating_sub(now).min(TICK_MS));
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(delay)) => {}
                _ = self.wake.notified() => {}
            }
        }
    }

    /// Everything due now, one at a time; answers when the next thing is due.
    /// A process without the lease wakes nothing and asks again next time.
    async fn tick(&self) -> Option<u64> {
        if !self.hold().await {
            return None;
        }
        let now = (self.clock)();
        let Ok(store) = self.read().await else {
            return None;
        };
        for entry in entries(&store).into_iter().filter(|entry| due(entry, now)) {
            if self.stopping.load(Ordering::Acquire) {
                break;
            }
            if entry["state"] == "parking" {
                self.settle_parking(&entry).await;
            } else {
                self.wake_one(&entry).await;
            }
        }
        self.read().await.ok().and_then(|store| next_due(&store))
    }

    // Only the entry this was about: a newer snooze of the same message, made
    // while this one was being dealt with, is its own promise.
    fn is(found: &Value, entry: &Value) -> bool {
        same(found, text(&entry["accountId"]), text(&entry["messageId"]))
            && found["wakeAt"] == entry["wakeAt"]
    }

    async fn forget(&self, entry: &Value) {
        let entry = entry.clone();
        let _ = self
            .update(move |store| {
                let list: Vec<Value> = entries(store)
                    .into_iter()
                    .filter(|found| !Self::is(found, &entry))
                    .collect();
                store["entries"] = json!(list);
                Ok(())
            })
            .await;
    }

    async fn mark(&self, entry: &Value, state: &'static str, retry: bool) {
        let now = (self.clock)();
        let entry = entry.clone();
        let _ = self
            .update(move |store| {
                let mut list = entries(store);
                if let Some(found) = list.iter_mut().find(|found| Self::is(found, &entry)) {
                    found["state"] = json!(state);
                    if retry {
                        let attempts = found["attempts"].as_u64().unwrap_or(0) + 1;
                        found["attempts"] = json!(attempts);
                        found["retryAt"] = json!(now + backoff(attempts));
                    }
                }
                store["entries"] = json!(list);
                Ok(())
            })
            .await;
    }

    /// A park a crash interrupted: finished if Gmail shows the message out of
    /// the Inbox under the label, otherwise never started and forgotten.
    async fn settle_parking(&self, entry: &Value) {
        let (account, id) = (text(&entry["accountId"]), text(&entry["messageId"]));
        let read = (self.gmail)(
            "gmail.read",
            json!({"accountId":account,"id":id,"full":false}),
        )
        .await;
        let resource = match read {
            Err("gmail_not_found" | "gmail_account_unknown") => {
                return self.forget(entry).await;
            }
            Err(_) => return self.mark(entry, "parking", true).await,
            Ok(resource) => resource,
        };
        let labels = label_ids(&resource);
        let label = self
            .read()
            .await
            .ok()
            .and_then(|store| store["labels"][account].as_str().map(str::to_owned));
        if !labels.iter().any(|l| l == "INBOX")
            && label.is_some_and(|label| labels.contains(&label))
        {
            self.mark(entry, "pending", false).await;
        } else {
            self.forget(entry).await;
        }
        self.changed(account, vec![]);
    }

    async fn wake_one(&self, entry: &Value) {
        let (account, id) = (text(&entry["accountId"]), text(&entry["messageId"]));
        self.mark(entry, "waking", false).await;
        let read = (self.gmail)(
            "gmail.read",
            json!({"accountId":account,"id":id,"full":false}),
        )
        .await;
        let resource = match read {
            Err("gmail_not_found" | "gmail_account_unknown") => {
                self.forget(entry).await;
                return self.changed(account, vec![]);
            }
            Err(_) => return self.mark(entry, "pending", true).await,
            Ok(resource) => resource,
        };
        let labels = label_ids(&resource);
        // Deleted or reported while it was away: that decision stands.
        if labels.iter().any(|l| l == "TRASH" || l == "SPAM") {
            self.forget(entry).await;
            return self.changed(account, vec![]);
        }
        // Already brought back by hand somewhere else: only the label goes.
        let back = labels.iter().any(|l| l == "INBOX");
        let mut done = false;
        for fresh in [false, true] {
            let Ok(label) = self.label(account, fresh).await else {
                break;
            };
            let add: Vec<&str> = if back {
                vec![]
            } else {
                vec!["INBOX", "UNREAD"]
            };
            let answer = (self.gmail)(
                "gmail.modify",
                json!({"accountId":account,"id":id,"addLabelIds":add,"removeLabelIds":[label]}),
            )
            .await;
            match answer {
                Ok(_) => {
                    done = true;
                    break;
                }
                // The label may be what Gmail no longer has; the message was
                // there a moment ago.
                Err("gmail_http_failed" | "gmail_not_found") if !fresh => continue,
                Err("gmail_not_found") => {
                    self.forget(entry).await;
                    return self.changed(account, vec![]);
                }
                Err(_) => break,
            }
        }
        if !done {
            return self.mark(entry, "pending", true).await;
        }
        self.forget(entry).await;
        let woken = if back {
            vec![]
        } else {
            let summary = crate::message::content::summarize(&resource, (self.clock)() as i64)
                .unwrap_or(Value::Null);
            vec![
                json!({"messageId":id,"subject":text(&summary["subject"]),"from":text(&summary["from"]["display"])}),
            ]
        };
        self.changed(account, woken);
    }
}
