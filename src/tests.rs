// Copyright © The Daybrite Project
// SPDX-License-Identifier: GPL-2.0-or-later
//! All titles, bodies, actor IDs and accounts in this module are synthetic fixtures.
use crate::{
    database::Database,
    model::{Document, Kind, Library, in_scope},
    sync::{Failure, exchange},
};
use day_persistence::Sqlite;
use day_sync::{Store, documents::DocumentStore};
use day_sync_webdav::{
    WebDavStore,
    webdav::{Client, Condition, Endpoint, ResourcePath},
};
use futures_lite::future::block_on;

const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const C: &str = "cccccccccccccccccccccccccccccccc";
const ID: &str = "0123456789abcdef0123456789abcdef";
fn task() -> Document {
    let mut d = Document::new(ID.into(), Kind::Task);
    d.set(A, "title", "Fixture".into())
        .expect("valid synthetic title");
    d
}
fn field(k: &str, v: &str) -> (String, String) {
    (k.into(), v.into())
}
fn database() -> Database {
    Database::open(Sqlite::memory()).expect("open a synthetic in-memory planner")
}

#[test]
fn concurrent_fields_reopen_and_late_replays() {
    let base = task();
    let mut a = base.clone();
    let mut b = base.clone();
    a.set(A, "done", "1".into()).unwrap();
    b.set(B, "body", "Edited offline".into()).unwrap();
    let mut merged = a.join(&b).unwrap();
    assert_eq!(merged, b.join(&a).unwrap());
    assert!(merged.flag("done"));
    assert_eq!(merged.get("body"), "Edited offline");
    merged.set(B, "done", "0".into()).unwrap();
    for stale in [&base, &a, &b, &merged.clone()] {
        assert_eq!(merged.join(stale).unwrap(), merged);
    }
    assert!(!merged.flag("done"));
}
#[test]
fn simultaneous_completion_and_reopen_have_a_consistent_winner() {
    let mut a = task();
    a.set(A, "done", "1".into()).unwrap();
    let mut b = a.clone();
    a.set(A, "done", "0".into()).unwrap();
    b.set(B, "done", "0".into()).unwrap();
    b.set(B, "done", "1".into()).unwrap();
    let merged = a.join(&b).unwrap();
    assert_eq!(merged, b.join(&a).unwrap());
    assert!(merged.flag("done"));
    let mut reopen = merged.clone();
    reopen.set(A, "done", "0".into()).unwrap();
    assert!(!reopen.join(&b).unwrap().flag("done"));
}
#[test]
fn concurrent_text_is_recoverable_and_choice_survives_a_stale_replica() {
    let mut a = task();
    let mut b = a.clone();
    a.set(A, "body", "First draft".into()).unwrap();
    b.set(B, "body", "Second draft".into()).unwrap();
    let mut joined = a.join(&b).unwrap();
    assert_eq!(joined.alternatives("body"), ["First draft", "Second draft"]);
    joined.set(C, "body", "First draft".into()).unwrap();
    assert_eq!(
        joined.join(&b).unwrap().alternatives("body"),
        ["First draft"]
    );
}
#[test]
fn removal_and_restore_are_independent_of_stale_text_and_list_membership() {
    let mut a = task();
    a.set(A, "parent", B.into()).unwrap();
    let mut b = a.clone();
    a.set(A, "trash", "1".into()).unwrap();
    b.set(B, "title", "Revised".into()).unwrap();
    let mut joined = a.join(&b).unwrap();
    assert!(joined.flag("trash"));
    assert_eq!(joined.get("title"), "Revised");
    assert!(in_scope(&joined, "trash", &Library::new(), "2026-05-03"));
    joined.set(C, "trash", "0".into()).unwrap();
    assert!(!joined.join(&a).unwrap().flag("trash"));
    assert!(in_scope(&joined, "inbox", &Library::new(), "2026-05-03")); // parent unavailable
    let mut list = Document::new(B.into(), Kind::List);
    list.set(B, "title", "Field notes".into()).unwrap();
    let library = Library::from([(B.into(), list)]);
    assert!(!in_scope(&joined, "inbox", &library, "2026-05-03"));
}
#[test]
fn merge_laws_hold_under_interleaved_edits_and_duplicate_delivery() {
    let actors = [A, B, C];
    let mut replicas = [task(), task(), task()];
    let mut rng = 127u32;
    for i in 0..1200 {
        rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
        let n = (rng % 3) as usize;
        let other = (n + 1 + (rng as usize / 17) % 2) % 3;
        match rng % 8 {
            0 | 1 => replicas[n] = replicas[n].join(&replicas[other]).unwrap(),
            2 => replicas[n]
                .set(actors[n], "body", format!("Fixture {i}"))
                .unwrap(),
            3 => replicas[n]
                .set(actors[n], "tag:field", ((rng >> 8) % 2).to_string())
                .unwrap(),
            4 => replicas[n]
                .set(actors[n], "trash", ((rng >> 9) % 2).to_string())
                .unwrap(),
            _ => replicas[n]
                .set(actors[n], "done", ((rng >> 10) % 2).to_string())
                .unwrap(),
        }
        let [a, b, c] = &replicas;
        let merged = a.join(b).unwrap().join(c).unwrap();
        assert_eq!(merged, c.join(&b.join(a).unwrap()).unwrap());
        assert_eq!(merged, merged.join(b).unwrap());
        assert_eq!(
            merged,
            Document::decode(ID, &merged.encode().unwrap()).unwrap()
        );
    }
}
#[test]
fn rejects_invalid_dates_sizes_ids_types_revisions_and_actor_forks() {
    let mut d = task();
    for (k, v) in [
        ("due", "2025-02-29"),
        ("due", "2024-2-29"),
        ("done", "true"),
        ("tag:", "1"),
        ("parent", ID),
        ("tag:Bad", "1"),
        ("unknown", "x"),
    ] {
        assert!(d.set(A, k, v.into()).is_err());
    }
    assert!(d.set(A, "due", "2024-02-29".into()).is_ok());
    assert!(d.set(A, "body", "x".repeat(256 * 1024 + 1)).is_err());
    assert!(Document::decode(B, &d.encode().unwrap()).is_err());
    let mut json: serde_json::Value = serde_json::from_slice(&d.encode().unwrap()).unwrap();
    json["version"] = 2.into();
    assert!(Document::decode(ID, &serde_json::to_vec(&json).unwrap()).is_err());
    let mut other = d.clone();
    d.set(A, "body", "one".into()).unwrap();
    other.set(A, "body", "two".into()).unwrap();
    assert!(d.join(&other).is_err());
    assert!(
        Document::new(ID.into(), Kind::Note)
            .set(A, "done", "1".into())
            .is_err()
    );
}
#[test]
fn smart_views_use_civil_dates_and_completed_items_leave_focus() {
    let mut d = task();
    d.set(A, "due", "2026-05-01".into()).unwrap();
    assert!(in_scope(&d, "today", &Library::new(), "2026-05-02"));
    assert!(!in_scope(&d, "today", &Library::new(), "2026-04-30"));
    d.set(A, "day", "2026-04-30".into()).unwrap();
    assert!(in_scope(&d, "today", &Library::new(), "2026-04-30"));
    d.set(A, "done", "1".into()).unwrap();
    assert!(!in_scope(&d, "today", &Library::new(), "2026-05-02"));
    assert!(in_scope(&d, "completed", &Library::new(), "2026-05-02"));
}
#[test]
fn sqlite_reopens_and_partial_edits_roll_back() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("fixture.sqlite");
    let d = {
        let db = Database::open(Sqlite::at(&path)).unwrap();
        let d = db
            .create(Kind::Task, &[field("title", "A"), field("tag:école", "1")])
            .unwrap();
        assert!(
            db.edit(
                &d.id,
                &[field("title", "Must roll back"), field("done", "invalid")]
            )
            .is_err()
        );
        d
    };
    let db = Database::open(Sqlite::at(&path)).unwrap();
    assert_eq!(db.get(&d.id).unwrap().unwrap(), d);
    let before = db.load().unwrap();
    assert!(
        db.create(Kind::Note, &[field("body", "No title field")])
            .is_err()
    );
    assert_eq!(before, db.load().unwrap());
}
fn dav() -> (day_webdav::testing::Server, WebDavStore) {
    let s = day_webdav::testing::Server::start().expect("bind synthetic loopback DAV fixture");
    let client =
        Client::builder(Endpoint::loopback(&s.url()).expect("numeric loopback fixture URL"))
            .build();
    (s, WebDavStore::new(client))
}
#[test]
fn actual_webdav_two_servers_offline_bridge_and_missing_remote_repair() {
    let (_s1, one) = dav();
    let (_s2, two) = dav();
    block_on(async {
        one.client().check_document_collection().await.unwrap();
        let a = database();
        let b = database();
        let bridge = database();
        let task = a
            .create(Kind::Task, &[field("title", "Bring pencils")])
            .unwrap();
        let step = a
            .create(
                Kind::Step,
                &[field("title", "Sharpen"), field("parent", &task.id)],
            )
            .unwrap();
        exchange(&a, &one, |_| {}).await.unwrap();
        exchange(&bridge, &one, |_| {}).await.unwrap();
        exchange(&bridge, &two, |_| {}).await.unwrap();
        exchange(&b, &two, |_| {}).await.unwrap();
        a.edit(&task.id, &[field("done", "1"), field("tag:coast", "1")])
            .unwrap();
        b.edit(
            &task.id,
            &[field("body", "Offline on B"), field("trash", "1")],
        )
        .unwrap();
        b.edit(&step.id, &[field("done", "1")]).unwrap();
        for (db, server) in [
            (&a, &one),
            (&b, &two),
            (&bridge, &one),
            (&bridge, &two),
            (&bridge, &one),
            (&a, &one),
            (&b, &two),
        ] {
            exchange(db, server, |_| {}).await.unwrap();
        }
        let expected = a.load().unwrap();
        assert_eq!(expected, b.load().unwrap());
        assert_eq!(expected, bridge.load().unwrap());
        assert!(expected[&task.id].flag("done"));
        assert!(expected[&task.id].flag("trash"));
        assert_eq!(expected[&task.id].tags(), ["coast"]);
        let dir = ["day-plan-v1".into(), task.id[..1].into()];
        let filename = format!("{}.json", task.id);
        one.delete(&dir, &filename).await.unwrap();
        exchange(&a, &one, |_| {}).await.unwrap();
        assert!(one.get(&dir, &filename).await.unwrap().is_some());
        let mut received = 0;
        exchange(&a, &one, |_| received += 1).await.unwrap();
        assert_eq!(
            received, 0,
            "verified unchanged files should require no download"
        );
    });
}
#[test]
fn damaged_document_blocks_overwrite_and_keeps_local_state() {
    let (_s, remote) = dav();
    block_on(async {
        let db = database();
        let d = db
            .create(Kind::Note, &[field("title", "Personal note")])
            .unwrap();
        exchange(&db, &remote, |_| {}).await.unwrap();
        let filename = format!("{}.json", d.id);
        let path =
            ResourcePath::from_segments(["day-plan-v1", &d.id[..1], &filename], false).unwrap();
        remote
            .client()
            .put(&path, b"truncated".to_vec(), Condition::Unconditional)
            .await
            .unwrap();
        let healthy = db
            .create(
                Kind::Task,
                &[field("title", "Unaffected task"), field("done", "1")],
            )
            .unwrap();
        assert_eq!(exchange(&db, &remote, |_| {}).await, Err(Failure::Data));
        assert_eq!(db.get(&d.id).unwrap().unwrap(), d);
        let receiver = database();
        assert_eq!(
            exchange(&receiver, &remote, |_| {}).await,
            Err(Failure::Data)
        );
        assert_eq!(receiver.get(&healthy.id).unwrap(), Some(healthy));
        assert!(receiver.get(&d.id).unwrap().is_none());
        assert_eq!(
            remote.client().get(&path, None).await.unwrap().body,
            b"truncated"
        );
    });
}
#[test]
fn disconnect_and_account_change_invalidate_only_their_own_receipts() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("account-fixture.sqlite");
    let first = "https://first.invalid/planner/";
    let second = "https://second.invalid/planner/";
    let document = {
        let db = Database::open(Sqlite::at(&path)).unwrap();
        db.server(first, Some("owner")).unwrap();
        db.server(second, Some("team")).unwrap();
        let d = db
            .create(Kind::Note, &[field("title", "Offline note")])
            .unwrap();
        db.import(&d, first, "old-account-revision").unwrap();
        db.import(&d, second, "team-revision").unwrap();
        db.server(first, Some("new-owner")).unwrap();
        assert_eq!(db.index(first).unwrap()[&d.id], None);
        assert_eq!(
            db.index(second).unwrap()[&d.id].as_deref(),
            Some("team-revision")
        );
        db.server(second, None).unwrap();
        d
    };
    let reopened = Database::open(Sqlite::at(&path)).unwrap();
    assert_eq!(
        reopened.servers().unwrap(),
        [(first.to_owned(), "new-owner".to_owned())]
    );
    assert_eq!(reopened.index(second).unwrap()[&document.id], None);
    assert_eq!(reopened.get(&document.id).unwrap(), Some(document));
}
#[test]
fn edit_during_transfer_never_earns_a_stale_receipt() {
    let db = database();
    let a = db
        .create(Kind::Task, &[field("title", "Original")])
        .unwrap();
    db.edit(&a.id, &[field("done", "1")]).unwrap();
    let merged = db.import(&a, "fixture-endpoint", "old-tag").unwrap();
    assert!(merged.flag("done"));
    assert_eq!(db.index("fixture-endpoint").unwrap()[&a.id], None);
}
#[test]
fn competing_conditional_put_never_clobbers_the_newer_version() {
    let (_s, remote) = dav();
    block_on(async {
        let dir = ["fixture".to_owned()];
        remote.ensure_directory(&dir).await.unwrap();
        assert!(remote.replace(&dir, "state", None, vec![1]).await.unwrap());
        let first = remote.version(&dir, "state").await.unwrap().unwrap();
        assert!(
            remote
                .replace(&dir, "state", Some(&first.validator), vec![2])
                .await
                .unwrap()
        );
        assert!(
            !remote
                .replace(&dir, "state", Some(&first.validator), vec![3])
                .await
                .unwrap()
        );
        assert!(!remote.replace(&dir, "state", None, vec![4]).await.unwrap());
        assert_eq!(remote.get(&dir, "state").await.unwrap(), Some(vec![2]));
    });
}

#[test]
fn ambiguous_acknowledgement_and_contended_writes_converge_without_replaying_edits() {
    use day_sync::documents::reconcile;
    use day_webdav::http::{
        self,
        simulation::{Reply, Simulation},
        transport::PreparedBody,
    };
    use std::sync::{Arc, Mutex};
    for lose_ack in [false, true] {
        let base = task();
        let mut local = base.clone();
        local.set(A, "done", "1".into()).unwrap();
        let mut rival = base.clone();
        rival.set(B, "body", "Another device".into()).unwrap();
        let bytes = rival.encode().unwrap();
        let state = Arc::new(Mutex::new((base.encode().unwrap(), 0u8, 0u32)));
        let shared = state.clone();
        let sim = Simulation::default();
        sim.handle(
            |_, _| true,
            move |req| {
                let mut server = shared.lock().unwrap();
                let tag = format!("\"revision-{}\"", server.1);
                if req.method.as_str() == "GET" {
                    return Ok(Reply::new(200, server.0.clone()).header("ETag", &tag));
                }
                assert_eq!(req.method.as_str(), "PUT");
                assert!(
                    req.headers
                        .iter()
                        .any(|(k, v)| k.eq_ignore_ascii_case("If-Match") && v == &tag)
                );
                server.2 += 1;
                if server.2 == 1 {
                    server.0 = bytes.clone();
                    server.1 += 1;
                    return Ok(Reply::new(412, vec![]));
                }
                let PreparedBody::Bytes(b) = req.body else {
                    panic!("fixture expected buffered document")
                };
                server.0 = b.as_ref().clone();
                server.1 += 1;
                if lose_ack {
                    Err(http::HttpError::Connect)
                } else {
                    Ok(Reply::new(204, vec![]))
                }
            },
        );
        let session = http::Session::new();
        session.set_simulation(Some(sim));
        let remote = WebDavStore::new(
            Client::builder(Endpoint::new("https://fixture.invalid/dav/").unwrap())
                .session(session)
                .build(),
        );
        let got = block_on(reconcile(&remote, &[], "fixture.json", |other| {
            let doc = Document::decode(ID, other.unwrap()).unwrap();
            local = local.join(&doc).unwrap();
            Ok(local.encode().unwrap())
        }))
        .unwrap();
        let merged = Document::decode(ID, &got.bytes).unwrap();
        assert!(merged.flag("done"));
        assert_eq!(merged.get("body"), "Another device");
        assert_eq!(state.lock().unwrap().2, 2);
    }
}
