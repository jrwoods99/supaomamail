use super::*;
use std::collections::HashMap;

const NOW: u64 = 1_790_000_000_000;
const HOUR: u64 = 3_600_000;

#[derive(Default)]
struct Fake {
    calls: Vec<(String, Value)>,
    labels: Vec<Value>,
    messages: HashMap<String, Value>,
    /// Errors answered once each, by method, in order.
    failures: HashMap<String, Vec<&'static str>>,
    /// Another process's write to the file, landing the moment this method
    /// is asked.
    intrude: Option<(&'static str, Intruder)>,
}

type Intruder = Box<dyn FnOnce(&mut Value) + Send>;

impl Fake {
    fn message(&mut self, id: &str, labels: &[&str]) {
        self.messages.insert(
            id.into(),
            json!({"id":id,"threadId":format!("t-{id}"),"labelIds":labels,"payload":{"headers":[
                {"name":"From","value":"=?UTF-8?Q?Dana_P=C3=A1rk?= <dana@example.org>"},
                {"name":"Subject","value":"Contract redline"}]}}),
        );
    }
    fn labels_of(&self, id: &str) -> Vec<String> {
        label_ids(&self.messages[id])
    }
    fn called(&self, method: &str) -> Vec<Value> {
        self.calls
            .iter()
            .filter(|(m, _)| m == method)
            .map(|(_, p)| p.clone())
            .collect()
    }
}

fn apply(message: &mut Value, add: &Value, remove: &Value) {
    let mut labels = label_ids(message);
    labels.retain(|l| !remove.as_array().unwrap().iter().any(|r| r == l.as_str()));
    for added in add.as_array().unwrap() {
        let added = added.as_str().unwrap().to_owned();
        if !labels.contains(&added) {
            labels.push(added);
        }
    }
    message["labelIds"] = json!(labels);
}

fn gmail(fake: &Arc<Mutex<Fake>>, memory: &Arc<Mutex<storage::Memory>>) -> Gmail {
    let (fake, memory) = (fake.clone(), memory.clone());
    Arc::new(move |method, params| {
        let (fake, memory) = (fake.clone(), memory.clone());
        Box::pin(async move {
            let intruder = {
                let mut f = fake.lock().unwrap();
                match f.intrude.take() {
                    Some((at, write)) if at == method => Some(write),
                    other => {
                        f.intrude = other;
                        None
                    }
                }
            };
            if let Some(write) = intruder {
                storage::Store::Memory(memory)
                    .update(move |store| {
                        write(store);
                        Ok(())
                    })
                    .await
                    .unwrap();
            }
            let mut f = fake.lock().unwrap();
            f.calls.push((method.to_owned(), params.clone()));
            if let Some(queue) = f.failures.get_mut(method)
                && !queue.is_empty()
            {
                return Err(queue.remove(0));
            }
            match method {
                "gmail.labels" => Ok(json!(f.labels)),
                "gmail.createLabel" => {
                    f.labels.push(
                        json!({"id":"Label_new","rawName":params["name"],"name":params["name"]}),
                    );
                    Ok(json!({"id":"Label_new","name":params["name"]}))
                }
                "gmail.read" => f
                    .messages
                    .get(params["id"].as_str().unwrap())
                    .cloned()
                    .ok_or("gmail_not_found"),
                "gmail.batchModify" => {
                    for id in params["ids"].as_array().unwrap() {
                        let known = f.labels.iter().any(|l| {
                            params["addLabelIds"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .chain(params["removeLabelIds"].as_array().unwrap())
                                .all(|want| {
                                    ["INBOX", "UNREAD"].contains(&want.as_str().unwrap())
                                        || l["id"] == *want
                                })
                        });
                        if !known {
                            return Err("gmail_http_failed");
                        }
                        let message = f
                            .messages
                            .get_mut(id.as_str().unwrap())
                            .ok_or("gmail_not_found")?;
                        apply(message, &params["addLabelIds"], &params["removeLabelIds"]);
                    }
                    Ok(json!({}))
                }
                "gmail.modify" => {
                    let message = f
                        .messages
                        .get_mut(params["id"].as_str().unwrap())
                        .ok_or("gmail_not_found")?;
                    apply(message, &params["addLabelIds"], &params["removeLabelIds"]);
                    Ok(json!({}))
                }
                _ => Err("unknown_method"),
            }
        })
    })
}

struct Rig {
    fake: Arc<Mutex<Fake>>,
    clock: Arc<AtomicU64>,
    memory: Arc<Mutex<storage::Memory>>,
}

impl Rig {
    fn new() -> Self {
        let mut fake = Fake::default();
        fake.message("m1", &["INBOX", "UNREAD", "CATEGORY_PERSONAL"]);
        fake.message("m2", &["INBOX"]);
        Rig {
            fake: Arc::new(Mutex::new(fake)),
            clock: Arc::new(AtomicU64::new(NOW)),
            memory: Arc::default(),
        }
    }
    fn snoozer(&self) -> Snoozer {
        let clock = self.clock.clone();
        Snoozer::with(
            gmail(&self.fake, &self.memory),
            Arc::new(move || clock.load(Ordering::SeqCst)),
            storage::Store::Memory(self.memory.clone()),
            false,
        )
    }
    fn advance(&self, by: u64) {
        self.clock.fetch_add(by, Ordering::SeqCst);
    }
    fn now(&self) -> u64 {
        self.clock.load(Ordering::SeqCst)
    }
}

async fn set(s: &Snoozer, ids: &[&str], wake_at: u64) -> Result<Value> {
    s.call(
        "snooze.set",
        &json!({"accountId":"Me@Example.org","ids":ids,"wakeAt":wake_at}),
    )
    .await
}

#[tokio::test]
async fn a_snooze_files_the_message_under_the_label_and_out_of_the_inbox() {
    let rig = Rig::new();
    let s = rig.snoozer();
    let mut events = s.subscribe();
    let answer = set(&s, &["m1", "m2", "m1"], NOW + HOUR).await.unwrap();
    {
        let f = rig.fake.lock().unwrap();
        assert_eq!(
            f.called("gmail.createLabel"),
            vec![json!({"accountId":"me@example.org","name":"Omamail/Snoozed"})],
            "the label is made the first time it is needed"
        );
        assert_eq!(
            f.called("gmail.batchModify"),
            vec![
                json!({"accountId":"me@example.org","ids":["m1","m2"],"addLabelIds":["Label_new"],"removeLabelIds":["INBOX"]})
            ]
        );
        assert_eq!(
            f.labels_of("m1"),
            vec!["UNREAD", "CATEGORY_PERSONAL", "Label_new"]
        );
    }
    assert_eq!(answer["entries"].as_array().unwrap().len(), 2);
    assert_eq!(
        answer["entries"][0],
        json!({"accountId":"me@example.org","messageId":"m1","wakeAt":NOW + HOUR,"state":"pending"})
    );
    assert_eq!(events.try_recv().unwrap()["params"]["woken"], json!([]));
    // The label is remembered rather than looked up on every snooze.
    set(&s, &["m2"], NOW + 2 * HOUR).await.unwrap();
    assert_eq!(rig.fake.lock().unwrap().called("gmail.labels").len(), 1);
    assert_eq!(
        rig.fake.lock().unwrap().called("gmail.createLabel").len(),
        1
    );
}

#[tokio::test]
async fn a_bad_request_is_refused_before_gmail_is_asked() {
    let rig = Rig::new();
    let s = rig.snoozer();
    for params in [
        json!({"accountId":"me@example.org","ids":["m1"],"wakeAt":NOW + 10_000}),
        json!({"accountId":"me@example.org","ids":["m1"],"wakeAt":NOW + 6 * 366 * 24 * HOUR}),
        json!({"accountId":"me@example.org","ids":["m1"],"wakeAt":"soon"}),
        json!({"accountId":"me@example.org","ids":[],"wakeAt":NOW + HOUR}),
        json!({"accountId":"me@example.org","ids":["../m1"],"wakeAt":NOW + HOUR}),
        json!({"accountId":"me@example.org","ids":["m1\n"],"wakeAt":NOW + HOUR}),
        json!({"accountId":"","ids":["m1"],"wakeAt":NOW + HOUR}),
        json!({"accountId":"me@example.org\r","ids":["m1"],"wakeAt":NOW + HOUR}),
        json!({"accountId":"me@example.org","ids":["m1"],"wakeAt":NOW + HOUR,"extra":1}),
    ] {
        assert_eq!(
            s.call("snooze.set", &params).await,
            Err("invalid_params"),
            "{params}"
        );
    }
    assert_eq!(
        s.call("snooze.unknown", &json!({})).await,
        Err("unknown_method")
    );
    assert!(rig.fake.lock().unwrap().calls.is_empty());
}

#[tokio::test]
async fn a_park_gmail_refused_leaves_nothing_behind() {
    let rig = Rig::new();
    rig.fake
        .lock()
        .unwrap()
        .failures
        .insert("gmail.batchModify".into(), vec!["gmail_rate_limited"]);
    let s = rig.snoozer();
    assert_eq!(
        set(&s, &["m1"], NOW + HOUR).await,
        Err("gmail_rate_limited")
    );
    let snapshot = s.call("snooze.snapshot", &json!({})).await.unwrap();
    assert_eq!(snapshot["entries"], json!([]));
    assert!(
        rig.fake
            .lock()
            .unwrap()
            .labels_of("m1")
            .contains(&"INBOX".to_owned())
    );
}

#[tokio::test]
async fn a_failed_park_takes_back_only_what_it_wrote() {
    let rig = Rig::new();
    let s = rig.snoozer();
    set(&s, &["m1"], NOW + HOUR).await.unwrap();
    {
        let mut f = rig.fake.lock().unwrap();
        f.failures
            .insert("gmail.batchModify".into(), vec!["gmail_rate_limited"]);
        f.intrude = Some((
            "gmail.batchModify",
            Box::new(|store: &mut Value| {
                store["entries"].as_array_mut().unwrap().push(json!({"accountId":"me@example.org","messageId":"m2","wakeAt":NOW + 3 * HOUR,"snoozedAt":NOW,"state":"pending"}));
            }),
        ));
    }
    assert_eq!(
        set(&s, &["m1"], NOW + 2 * HOUR).await,
        Err("gmail_rate_limited")
    );
    assert_eq!(
        s.call("snooze.snapshot", &json!({})).await.unwrap()["entries"],
        json!([
            {"accountId":"me@example.org","messageId":"m2","wakeAt":NOW + 3 * HOUR,"state":"pending"},
            {"accountId":"me@example.org","messageId":"m1","wakeAt":NOW + HOUR,"state":"pending"}
        ]),
        "another process's snooze is kept, and m1 keeps the snooze it already had"
    );
}

#[tokio::test]
async fn a_wake_never_takes_a_newer_snooze_with_it() {
    let rig = Rig::new();
    let s = rig.snoozer();
    set(&s, &["m1"], NOW + HOUR).await.unwrap();
    let again = json!({"accountId":"me@example.org","messageId":"m1","wakeAt":NOW + 5 * HOUR,"snoozedAt":NOW + HOUR,"state":"pending"});
    let written = again.clone();
    rig.fake.lock().unwrap().intrude = Some((
        "gmail.modify",
        Box::new(move |store: &mut Value| store["entries"] = json!([written])),
    ));
    rig.advance(HOUR);
    assert_eq!(s.inner.tick().await, Some(NOW + 5 * HOUR));
    assert_eq!(
        s.call("snooze.snapshot", &json!({})).await.unwrap()["entries"],
        json!([projection(&again)]),
        "snoozed again while it was waking: the new time stands"
    );
}

#[tokio::test]
async fn at_its_time_the_message_comes_back_unread_and_says_so() {
    let rig = Rig::new();
    let s = rig.snoozer();
    set(&s, &["m1"], NOW + HOUR).await.unwrap();
    let mut events = s.subscribe();
    rig.advance(HOUR - 1);
    s.inner.tick().await;
    assert!(
        rig.fake.lock().unwrap().called("gmail.modify").is_empty(),
        "not a moment early"
    );
    rig.advance(1);
    let next = s.inner.tick().await;
    assert_eq!(next, None, "nothing else is waiting");
    {
        let f = rig.fake.lock().unwrap();
        assert_eq!(
            f.called("gmail.modify"),
            vec![
                json!({"accountId":"me@example.org","id":"m1","addLabelIds":["INBOX","UNREAD"],"removeLabelIds":["Label_new"]})
            ]
        );
        assert!(f.labels_of("m1").contains(&"INBOX".to_owned()));
    }
    let event = events.try_recv().unwrap();
    assert_eq!(event["method"], "snooze.changed");
    assert_eq!(event["params"]["accountId"], "me@example.org");
    assert_eq!(
        event["params"]["woken"],
        json!([{"messageId":"m1","subject":"Contract redline","from":"Dana Párk"}])
    );
    assert_eq!(
        s.call("snooze.snapshot", &json!({})).await.unwrap()["entries"],
        json!([])
    );
}

#[tokio::test]
async fn mail_brought_back_by_hand_only_loses_the_label() {
    let rig = Rig::new();
    let s = rig.snoozer();
    set(&s, &["m1"], NOW + HOUR).await.unwrap();
    rig.fake.lock().unwrap().messages.get_mut("m1").unwrap()["labelIds"] =
        json!(["INBOX", "Label_new"]);
    let mut events = s.subscribe();
    rig.advance(HOUR);
    s.inner.tick().await;
    assert_eq!(
        rig.fake.lock().unwrap().called("gmail.modify"),
        vec![
            json!({"accountId":"me@example.org","id":"m1","addLabelIds":[],"removeLabelIds":["Label_new"]})
        ],
        "no unread mark on a message already read in the Inbox"
    );
    assert_eq!(events.try_recv().unwrap()["params"]["woken"], json!([]));
}

#[tokio::test]
async fn deleted_reported_or_vanished_mail_stays_where_it_is() {
    let rig = Rig::new();
    let s = rig.snoozer();
    set(&s, &["m1", "m2"], NOW + HOUR).await.unwrap();
    {
        let mut f = rig.fake.lock().unwrap();
        f.messages.get_mut("m1").unwrap()["labelIds"] = json!(["TRASH", "Label_new"]);
        f.messages.remove("m2");
    }
    rig.advance(HOUR);
    s.inner.tick().await;
    assert!(rig.fake.lock().unwrap().called("gmail.modify").is_empty());
    assert_eq!(
        s.call("snooze.snapshot", &json!({})).await.unwrap()["entries"],
        json!([])
    );
}

#[tokio::test]
async fn a_failed_wake_is_tried_again_later_and_never_dropped() {
    let rig = Rig::new();
    let s = rig.snoozer();
    set(&s, &["m1"], NOW + HOUR).await.unwrap();
    rig.fake
        .lock()
        .unwrap()
        .failures
        .insert("gmail.read".into(), vec!["gmail_timeout", "gmail_timeout"]);
    rig.advance(HOUR);
    let next = s.inner.tick().await;
    assert_eq!(
        next,
        Some(rig.now() + 60_000),
        "a minute after the first failure"
    );
    let entry = &s.call("snooze.snapshot", &json!({})).await.unwrap()["entries"][0];
    assert_eq!(entry["state"], "pending");
    rig.advance(60_000);
    let next = s.inner.tick().await;
    assert_eq!(
        next,
        Some(rig.now() + 300_000),
        "five minutes after the second"
    );
    rig.advance(300_000);
    s.inner.tick().await;
    assert!(
        rig.fake
            .lock()
            .unwrap()
            .labels_of("m1")
            .contains(&"INBOX".to_owned())
    );
}

#[tokio::test]
async fn snoozes_survive_a_restart_and_overdue_ones_wake_first_thing() {
    let rig = Rig::new();
    let first = rig.snoozer();
    set(&first, &["m1"], NOW + HOUR).await.unwrap();
    first.shutdown().await;
    drop(first);
    rig.advance(3 * HOUR);
    let second = rig.snoozer();
    assert_eq!(
        second.call("snooze.snapshot", &json!({})).await.unwrap()["entries"][0]["messageId"],
        "m1"
    );
    second.inner.tick().await;
    assert!(
        rig.fake
            .lock()
            .unwrap()
            .labels_of("m1")
            .contains(&"INBOX".to_owned())
    );
}

#[tokio::test]
async fn only_the_lease_holder_wakes_and_another_carries_on_when_it_exits() {
    let rig = Rig::new();
    let owner = rig.snoozer();
    let other = rig.snoozer();
    owner.call("snooze.snapshot", &json!({})).await.unwrap();
    other.call("snooze.snapshot", &json!({})).await.unwrap();
    assert!(owner.inner.worker.lock().unwrap().lease.is_some());
    assert!(other.inner.worker.lock().unwrap().lease.is_none());
    set(&other, &["m2"], NOW + HOUR).await.unwrap();
    set(&other, &["m1"], NOW + 2 * HOUR).await.unwrap();
    rig.advance(HOUR);
    let inbox = |id: &str| {
        rig.fake
            .lock()
            .unwrap()
            .labels_of(id)
            .contains(&"INBOX".to_owned())
    };
    assert_eq!(other.inner.tick().await, None);
    assert!(!inbox("m2"), "a process without the lease wakes nothing");
    owner.inner.tick().await;
    assert!(inbox("m2"), "the owner reads what the other process wrote");
    owner.shutdown().await;
    rig.advance(HOUR);
    other.inner.tick().await;
    assert!(
        other.inner.worker.lock().unwrap().lease.is_some(),
        "a released lease is taken over"
    );
    assert!(inbox("m1"));
}

#[tokio::test]
async fn an_interrupted_park_is_finished_or_forgotten() {
    let rig = Rig::new();
    let s = rig.snoozer();
    {
        let mut f = rig.fake.lock().unwrap();
        f.labels
            .push(json!({"id":"Label_9","rawName":"Omamail/Snoozed"}));
        f.messages.get_mut("m1").unwrap()["labelIds"] = json!(["Label_9"]);
    }
    *rig.memory.lock().unwrap() = storage::Memory::default();
    s.inner
        .store
        .update(|store| {
            store["labels"]["me@example.org"] = json!("Label_9");
            store["entries"] = json!([
                {"accountId":"me@example.org","messageId":"m1","wakeAt":NOW + HOUR,"snoozedAt":NOW,"state":"parking"},
                {"accountId":"me@example.org","messageId":"m2","wakeAt":NOW + HOUR,"snoozedAt":NOW,"state":"parking"}
            ]);
            Ok(())
        })
        .await
        .unwrap();
    s.inner.tick().await;
    assert!(
        rig.fake.lock().unwrap().called("gmail.read").is_empty(),
        "a park is given time to land"
    );
    rig.advance(PARKING_GRACE_MS);
    s.inner.tick().await;
    let entries = s.call("snooze.snapshot", &json!({})).await.unwrap()["entries"].clone();
    assert_eq!(
        entries,
        json!([{"accountId":"me@example.org","messageId":"m1","wakeAt":NOW + HOUR,"state":"pending"}]),
        "m1 was parked; m2 never left the Inbox, so its snooze never began"
    );
}

#[tokio::test]
async fn undo_puts_it_back_where_it_was_and_wake_now_is_the_clock_early() {
    let rig = Rig::new();
    let s = rig.snoozer();
    set(&s, &["m1", "m2"], NOW + HOUR).await.unwrap();
    let answer = s
        .call(
            "snooze.cancel",
            &json!({"accountId":"me@example.org","ids":["m1"]}),
        )
        .await
        .unwrap();
    assert_eq!(answer, json!({"cancelled":["m1"]}));
    {
        let f = rig.fake.lock().unwrap();
        assert_eq!(
            f.called("gmail.batchModify")[1]["addLabelIds"],
            json!(["INBOX"])
        );
        assert_eq!(
            f.labels_of("m1"),
            vec!["UNREAD", "CATEGORY_PERSONAL", "INBOX"],
            "back, with its read state as it was"
        );
    }
    s.call(
        "snooze.wake",
        &json!({"accountId":"me@example.org","ids":["m2"]}),
    )
    .await
    .unwrap();
    s.inner.tick().await;
    assert!(
        rig.fake
            .lock()
            .unwrap()
            .labels_of("m2")
            .contains(&"UNREAD".to_owned())
    );
    assert_eq!(
        s.call("snooze.snapshot", &json!({})).await.unwrap()["entries"],
        json!([])
    );
}

#[tokio::test]
async fn a_label_deleted_in_gmail_is_found_or_made_again() {
    let rig = Rig::new();
    let s = rig.snoozer();
    s.inner
        .store
        .update(|store| {
            store["labels"]["me@example.org"] = json!("Label_gone");
            Ok(())
        })
        .await
        .unwrap();
    set(&s, &["m1"], NOW + HOUR).await.unwrap();
    let f = rig.fake.lock().unwrap();
    assert_eq!(
        f.called("gmail.batchModify").len(),
        2,
        "tried with the remembered label, then a fresh one"
    );
    assert!(f.labels_of("m1").contains(&"Label_new".to_owned()));
}

#[tokio::test]
async fn a_forged_file_is_refused_rather_than_trusted() {
    let rig = Rig::new();
    let s = rig.snoozer();
    for forged in [
        json!([]),
        json!({"version":2,"labels":{},"entries":[]}),
        json!({"version":1,"labels":{},"entries":[{"accountId":"me@example.org","messageId":"../x","wakeAt":1,"snoozedAt":1,"state":"pending"}]}),
        json!({"version":1,"labels":{},"entries":[{"accountId":"me@example.org","messageId":"m1","wakeAt":1,"snoozedAt":1,"state":"sent"}]}),
        json!({"version":1,"labels":{"me@example.org":"x\ny"},"entries":[]}),
    ] {
        let value = forged.clone();
        s.inner
            .store
            .update(move |store| {
                *store = value;
                Ok(())
            })
            .await
            .unwrap();
        assert_eq!(
            s.call("snooze.snapshot", &json!({})).await,
            Err("snooze_storage_invalid"),
            "{forged}"
        );
        assert_eq!(
            set(&s, &["m1"], NOW + HOUR).await,
            Err("snooze_storage_invalid")
        );
    }
    assert!(
        rig.fake
            .lock()
            .unwrap()
            .called("gmail.batchModify")
            .is_empty()
    );
}
