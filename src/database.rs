// Copyright © The Daybrite Project
// SPDX-License-Identifier: GPL-2.0-or-later
//! The same SQLite schema and atomic read/merge/write transaction run on every toolkit.
//! The wire representation retains causal context across launches and disconnected servers.
use crate::model::{Document, Invalid, Kind, Library, valid_id};
use day_persistence::{DbError, ModelContainer, Schema, Sqlite, SqliteConnection, Value};

pub struct Database {
    db: ModelContainer,
    actor: String,
}

fn bad(_: Invalid) -> DbError {
    DbError::driver("planner.invalid_document")
}
fn value(s: &str) -> Value {
    Value::Text(s.to_owned())
}
fn scalar(
    c: &mut dyn SqliteConnection,
    sql: &str,
    args: &[Value],
) -> Result<Option<String>, DbError> {
    let mut found = None;
    c.query(sql, args, &mut |r| {
        found = Some(r.get(0));
    })?;
    found.map(|v| v.as_text().map(str::to_owned)).transpose()
}
fn read(c: &mut dyn SqliteConnection, id: &str) -> Result<Option<Document>, DbError> {
    scalar(
        c,
        "SELECT json FROM planner_document WHERE id=?",
        &[value(id)],
    )?
    .map(|s| Document::decode(id, s.as_bytes()).map_err(bad))
    .transpose()
}
fn put(c: &mut dyn SqliteConnection, d: &Document) -> Result<(), DbError> {
    let json = String::from_utf8(d.encode().map_err(bad)?)
        .map_err(|_| DbError::driver("planner.encoding"))?;
    c.execute("DELETE FROM planner_receipt WHERE id=?", &[value(&d.id)])?;
    c.execute(
        "INSERT INTO planner_document(id,json) VALUES(?,?)
         ON CONFLICT(id) DO UPDATE SET json=excluded.json",
        &[value(&d.id), Value::Text(json)],
    )?;
    Ok(())
}
fn transaction<T>(
    c: &mut dyn SqliteConnection,
    f: impl FnOnce(&mut dyn SqliteConnection) -> Result<T, DbError>,
) -> Result<T, DbError> {
    c.execute_batch("BEGIN IMMEDIATE")?;
    let result = f(c).and_then(|v| {
        c.commit()?;
        Ok(v)
    });
    if result.is_err() {
        let _ = c.rollback();
    }
    result
}
impl Database {
    pub fn open(driver: Sqlite) -> Result<Self, DbError> {
        let db = ModelContainer::open(driver, Schema::default())?;
        let actor = db.try_with_connection(|c| {
            transaction(c, |c| {
                c.execute_batch(
                    "CREATE TABLE IF NOT EXISTS planner_document(
                         id TEXT PRIMARY KEY NOT NULL, json TEXT NOT NULL);
                     CREATE TABLE IF NOT EXISTS planner_device(
                         slot INTEGER PRIMARY KEY CHECK(slot=1), actor TEXT NOT NULL);
                     INSERT OR IGNORE INTO planner_device VALUES(1,lower(hex(randomblob(16))));
                     CREATE TABLE IF NOT EXISTS planner_receipt(
                         id TEXT NOT NULL, server TEXT NOT NULL, validator TEXT NOT NULL, PRIMARY KEY(id,server));
                     CREATE TABLE IF NOT EXISTS planner_server(
                         url TEXT PRIMARY KEY NOT NULL, username TEXT NOT NULL);"
                )?;
                scalar(c, "SELECT actor FROM planner_device WHERE slot=1", &[])?
                    .filter(|actor| valid_id(actor))
                    .ok_or_else(|| DbError::driver("planner.identity"))
            })
        })?;
        Ok(Self { db, actor })
    }
    pub fn load(&self) -> Result<Library, DbError> {
        self.db.try_with_connection(|c| {
            let mut rows = Vec::new();
            c.query("SELECT id,json FROM planner_document", &[], &mut |r| {
                rows.push((r.get(0), r.get(1)))
            })?;
            rows.into_iter()
                .map(|(id, json)| {
                    let key = id.as_text()?;
                    Ok((
                        key.to_owned(),
                        Document::decode(key, json.as_text()?.as_bytes()).map_err(bad)?,
                    ))
                })
                .collect()
        })
    }
    pub fn create(&self, kind: Kind, fields: &[(String, String)]) -> Result<Document, DbError> {
        self.db.try_with_connection(|c| {
            transaction(c, |c| {
                let id = scalar(c, "SELECT lower(hex(randomblob(16)))", &[])?
                    .ok_or_else(|| DbError::driver("planner.identity"))?;
                if read(c, &id)?.is_some() {
                    return Err(DbError::driver("planner.collision"));
                }
                let mut d = Document::new(id, kind);
                for (k, v) in fields {
                    d.set(&self.actor, k, v.clone()).map_err(bad)?;
                }
                put(c, &d)?;
                Ok(d)
            })
        })
    }
    pub fn edit(&self, id: &str, fields: &[(String, String)]) -> Result<Document, DbError> {
        self.db.try_with_connection(|c| {
            transaction(c, |c| {
                let mut d = read(c, id)?.ok_or_else(|| DbError::driver("planner.item_missing"))?;
                for (k, v) in fields {
                    d.set(&self.actor, k, v.clone()).map_err(bad)?;
                }
                put(c, &d)?;
                Ok(d)
            })
        })
    }
    pub fn get(&self, id: &str) -> Result<Option<Document>, DbError> {
        self.db.try_with_connection(|c| read(c, id))
    }
    pub fn import(
        &self,
        remote: &Document,
        server: &str,
        validator: &str,
    ) -> Result<Document, DbError> {
        self.db.try_with_connection(|c| {
            transaction(c, |c| {
                let local = read(c, &remote.id)?;
                let joined = match &local {
                    Some(l) => l.join(remote).map_err(bad)?,
                    None => remote.clone(),
                };
                if local.as_ref() != Some(&joined) {
                    put(c, &joined)?;
                }
                if joined == *remote {
                    c.execute(
                        "INSERT OR REPLACE INTO planner_receipt VALUES(?,?,?)",
                        &[value(&joined.id), value(server), value(validator)],
                    )?;
                }
                Ok(joined)
            })
        })
    }
    pub fn servers(&self) -> Result<Vec<(String, String)>, DbError> {
        self.db.try_with_connection(|c| {
            let mut rows = vec![];
            c.query(
                "SELECT url,username FROM planner_server ORDER BY url",
                &[],
                &mut |r| rows.push((r.get(0), r.get(1))),
            )?;
            rows.into_iter()
                .map(|(a, b)| Ok((a.as_text()?.into(), b.as_text()?.into())))
                .collect()
        })
    }
    pub fn index(
        &self,
        server: &str,
    ) -> Result<std::collections::BTreeMap<String, Option<String>>, DbError> {
        self.db.try_with_connection(|c| {
            let mut rows = Vec::new();
            c.query(
                "SELECT d.id,r.validator FROM planner_document d
                 LEFT JOIN planner_receipt r ON r.id=d.id AND r.server=?",
                &[value(server)],
                &mut |r| rows.push((r.get(0), r.get(1))),
            )?;
            rows.into_iter()
                .map(|(id, validator)| {
                    let tag = match validator {
                        Value::Null => None,
                        value => Some(value.as_text()?.to_owned()),
                    };
                    Ok((id.as_text()?.to_owned(), tag))
                })
                .collect()
        })
    }
    pub fn server(&self, url: &str, username: Option<&str>) -> Result<(), DbError> {
        self.db.try_with_connection(|c| {
            transaction(c, |c| {
                c.execute("DELETE FROM planner_receipt WHERE server=?", &[value(url)])?;
                if let Some(user) = username {
                    c.execute(
                        "INSERT INTO planner_server VALUES(?,?)
                         ON CONFLICT(url) DO UPDATE SET username=excluded.username",
                        &[value(url), value(user)],
                    )?;
                } else {
                    c.execute("DELETE FROM planner_server WHERE url=?", &[value(url)])?;
                }
                Ok(())
            })
        })
    }
}
