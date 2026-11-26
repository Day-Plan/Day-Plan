// Copyright © The Daybrite Project
// SPDX-License-Identifier: GPL-2.0-or-later
use crate::{
    database::Database,
    model::{Document, valid_id},
};
use day_sync::{
    StoreError, StoreErrorKind,
    documents::{DocumentStore, reconcile},
};
use std::collections::{BTreeMap, BTreeSet};

const DIRECTORY: &str = "day-plan-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Storage,
    Data,
    Access,
    Network,
    Quota,
    Server,
    Credentials,
}

impl From<StoreError> for Failure {
    fn from(e: StoreError) -> Self {
        match e.kind {
            StoreErrorKind::Unauthorized | StoreErrorKind::Forbidden => Self::Access,
            StoreErrorKind::InsufficientStorage => Self::Quota,
            StoreErrorKind::Unavailable => Self::Network,
            StoreErrorKind::Invalid => Self::Data,
            _ => Self::Server,
        }
    }
}
fn invalid() -> StoreError {
    StoreError::new(StoreErrorKind::Invalid)
}

/// Pull and publish every known shard. Documents use a fixed hexadecimal prefix, so
/// server listing limits apply to a fraction of the library. Physical absence is
/// repaired from local state. Only explicit, replicated trash/restore edits hide items.
pub async fn exchange(
    db: &Database,
    store: &dyn DocumentStore,
    mut imported: impl FnMut(Document),
) -> Result<(), Failure> {
    let root = [DIRECTORY.to_owned()];
    store.ensure_directory(&root).await?;
    let mut shards: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let server = store.endpoint();
    let known = db.index(&server).map_err(|_| Failure::Storage)?;
    let mut existing = BTreeSet::new();
    for id in known.keys().cloned() {
        shards.entry(id[..1].into()).or_default().insert(id);
    }
    for entry in store.list(&root).await? {
        let shard = entry.trim_end_matches('/');
        if shard.len() == 1
            && shard
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            existing.insert(shard.to_owned());
            shards.entry(shard.into()).or_default();
        }
    }
    let mut damaged = false;
    for (shard, mut ids) in shards {
        let dir = [DIRECTORY.to_owned(), shard.clone()];
        if !existing.contains(&shard) {
            store.ensure_directory(&dir).await?;
        }
        for (name, tag) in store.index(&dir).await? {
            if let Some(id) = name.strip_suffix(".json") {
                if !valid_id(id) || !id.starts_with(&shard) {
                    damaged = true;
                    continue;
                }
                if tag.is_some() && known.get(id) == Some(&tag) {
                    ids.remove(id);
                } else {
                    ids.insert(id.into());
                }
            }
        }
        for id in ids {
            let mut local = db.get(&id).map_err(|_| Failure::Storage)?;
            let result = reconcile(store, &dir, &format!("{id}.json"), |bytes| {
                if let Some(bytes) = bytes {
                    let other = Document::decode(&id, bytes).map_err(|_| invalid())?;
                    local = Some(match &local {
                        Some(d) => d.join(&other).map_err(|_| invalid())?,
                        None => other,
                    });
                }
                local
                    .as_ref()
                    .ok_or_else(invalid)?
                    .encode()
                    .map_err(|_| invalid())
            })
            .await;
            let bytes = match result {
                Ok(version) => version,
                Err(e) if e.kind == StoreErrorKind::Invalid => {
                    damaged = true;
                    continue;
                }
                Err(e) => return Err(e.into()),
            };
            // Re-read inside an immediate transaction. The person can keep editing
            // while HTTP is in flight, including the document being synchronized.
            let remote = Document::decode(&id, &bytes.bytes).map_err(|_| Failure::Data)?;
            imported(
                db.import(&remote, &server, &bytes.validator)
                    .map_err(|_| Failure::Storage)?,
            );
        }
    }
    if damaged { Err(Failure::Data) } else { Ok(()) }
}
