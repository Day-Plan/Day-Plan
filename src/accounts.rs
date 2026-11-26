// Copyright © The Daybrite Project
// SPDX-License-Identifier: GPL-2.0-or-later
use crate::{
    res,
    scene::Scene,
    sync::{Failure, exchange},
};
use day::prelude::*;
use day_sync_webdav::{
    WebDavStore,
    webdav::{Authentication, Client, Endpoint},
};

fn store(url: &str, user: &str, password: String) -> Result<WebDavStore, Failure> {
    let endpoint = Endpoint::new(url).map_err(|_| Failure::Server)?;
    Ok(WebDavStore::new(
        Client::builder(endpoint)
            .authentication(Authentication::Basic {
                username: user.into(),
                password,
            })
            .build(),
    ))
}
#[cfg(not(target_arch = "wasm32"))]
fn key(url: &str, user: &str) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write;
    let digest = Sha256::digest(format!("{url}\0{user}").as_bytes());
    let mut key = String::from("planner-");
    for byte in digest {
        let _ = write!(key, "{byte:02x}");
    }
    key
}

fn report(e: Failure) -> String {
    match e {
        Failure::Storage => res::str::storage_failed(),
        Failure::Data => res::str::sync_invalid(),
        Failure::Access => res::str::sync_access(),
        Failure::Network => res::str::sync_network(),
        Failure::Quota => res::str::sync_quota(),
        Failure::Server => res::str::sync_server(),
        Failure::Credentials => res::str::sync_credentials(),
    }
    .format()
}
async fn password(sc: Scene, url: &str, user: &str) -> Result<String, Failure> {
    if let Some(p) = sc
        .passwords
        .with_untracked(|m| m.get(&(url.into(), user.into())).cloned())
    {
        return Ok(p);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let reason = res::str::credential_reason().format();
        let secret = day_part_keychain::get_future(&key(url, user), &reason)
            .await
            .map_err(|_| Failure::Credentials)?;
        secret
            .as_str()
            .map(str::to_owned)
            .ok_or(Failure::Credentials)
    }
    #[cfg(target_arch = "wasm32")]
    {
        Err(Failure::Credentials)
    }
}
pub fn sync(sc: Scene) {
    if sc.busy.get_untracked() || sc.servers.with_untracked(Vec::is_empty) {
        return;
    }
    sc.busy.set(true);
    sc.dirty.set(false);
    sc.sync_message.set(res::str::syncing().format());
    sc.owner.enter(|| {
        day::task(async move {
            let servers = sc.servers.get_untracked();
            let db = sc.db.get_untracked();
            let mut error = None;
            let mut stores = vec![];
            for (url, user) in &servers {
                match password(sc, url, user)
                    .await
                    .and_then(|p| store(url, user, p))
                {
                    Ok(store) => stores.push((url.clone(), store)),
                    Err(e) => error = Some((url.clone(), e)),
                }
            }
            // Second sweep relays changes from later servers back to earlier servers.
            for _ in 0..if stores.len() > 1 { 2 } else { 1 } {
                for (url, store) in &stores {
                    if let Err(e) = exchange(&db, store, |d| sc.accept(d)).await {
                        error = Some((url.clone(), e));
                    }
                }
            }
            sc.sync_message.set(match error {
                Some((url, e)) => res::str::server_problem(res::str::ServerProblem {
                    server: url,
                    reason: report(e),
                })
                .format(),
                None => res::str::synced().format(),
            });
            sc.busy.set(false);
        })
    });
}

fn connect(sc: Scene, url: String, user: String, pwd: Signal<String>) {
    if sc.busy.get_untracked() {
        return;
    }
    let p = pwd.get_untracked();
    if user.trim().is_empty() || p.is_empty() {
        sc.sync_message.set(res::str::sync_credentials().format());
        return;
    }
    let Ok(client) = store(url.trim(), user.trim(), p.clone()) else {
        sc.sync_message.set(res::str::url_hint().format());
        return;
    };
    let url = client.client().endpoint().as_str().to_owned();
    let user = user.trim().to_owned();
    pwd.set(String::new());
    sc.busy.set(true);
    sc.sync_message.set(res::str::checking().format());
    sc.owner.enter(|| {
        day::task(async move {
            let result = async {
                client.client().check_document_collection().await.map_err(
                    |failure| match failure.source {
                        day_sync_webdav::webdav::Error::Status {
                            status: 401 | 403, ..
                        } => Failure::Access,
                        _ => Failure::Server,
                    },
                )?;
                #[cfg(not(target_arch = "wasm32"))]
                day_part_keychain::set(
                    &key(&url, &user),
                    &day_part_keychain::Secret::from(p.as_str()),
                    day_part_keychain::Protection::WhenUnlocked,
                )
                .map_err(|_| Failure::Credentials)?;
                sc.db
                    .get_untracked()
                    .server(&url, Some(&user))
                    .map_err(|_| Failure::Storage)?;
                sc.passwords.update(|m| {
                    m.retain(|(u, _), _| u != &url);
                    m.insert((url, user), p);
                });
                sc.servers.set(
                    sc.db
                        .get_untracked()
                        .servers()
                        .map_err(|_| Failure::Storage)?,
                );
                Ok::<(), Failure>(())
            }
            .await;
            sc.busy.set(false);
            match result {
                Ok(()) => {
                    sync(sc);
                }
                Err(e) => sc.sync_message.set(report(e)),
            }
        })
    });
}
fn disconnect(sc: Scene, url: String, user: String) {
    if sc.busy.get_untracked() {
        return;
    }
    let db = sc.db.get_untracked();
    let result = db.server(&url, None).and_then(|()| db.servers());
    match result {
        Ok(servers) => {
            sc.servers.set(servers);
            #[cfg(not(target_arch = "wasm32"))]
            {
                let _ = day_part_keychain::remove(&key(&url, &user));
            }
            sc.passwords.update(|p| {
                p.remove(&(url, user));
            });
            sc.sync_message.set(res::str::disconnected().format());
        }
        Err(_) => sc.sync_message.set(report(Failure::Storage)),
    }
}
pub fn page(sc: Scene) -> impl Piece {
    let url = Signal::new(String::new());
    let user = Signal::new(String::new());
    let pass = Signal::new(String::new());
    scroll(
        form((
            section((
                label(res::str::server_intro()).secondary(),
                labeled(
                    res::str::server_url(),
                    text_field(url).id(day::id!(server_url)),
                ),
                labeled(
                    res::str::username(),
                    text_field(user).id(day::id!(server_user)),
                ),
                labeled(
                    res::str::password(),
                    secure_field(pass).id(day::id!(server_password)),
                ),
                button(res::str::connect())
                    .prominent()
                    .enabled(move || !sc.busy.get())
                    .id(day::id!(connect))
                    .action(move || connect(sc, url.get_untracked(), user.get_untracked(), pass)),
            ))
            .title(res::str::connect_server()),
            section((
                each(
                    items(move || sc.servers.get(), |a: &(String, String)| a.0.clone()),
                    move |s| {
                        column((
                            label(move || s.get().0),
                            label(move || s.get().1).secondary(),
                            button(res::str::disconnect())
                                .enabled(move || !sc.busy.get())
                                .action(move || {
                                    let (u, n) = s.get();
                                    disconnect(sc, u, n);
                                }),
                        ))
                        .align(HAlign::Leading)
                    },
                ),
                button(res::str::sync_now())
                    .id(day::id!(sync_now))
                    .enabled(move || !sc.busy.get() && !sc.servers.with(Vec::is_empty))
                    .action(move || sync(sc)),
                label(move || sc.sync_message.get()).secondary(),
            ))
            .title(res::str::servers()),
        ))
        .padding(Space::L),
    )
}
