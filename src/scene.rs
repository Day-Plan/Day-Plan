// Copyright © The Daybrite Project
// SPDX-License-Identifier: GPL-2.0-or-later
use crate::{
    database::Database,
    model::{Document, Kind, Library, in_scope},
    res,
};
use day::prelude::*;
use std::{collections::BTreeMap, rc::Rc};

#[derive(Clone, Copy)]
pub struct Scene {
    pub owner: day::reactive::Scope,
    pub db: Signal<Rc<Database>>,
    pub library: Signal<Library>,
    pub scope: Signal<Option<String>>,
    pub selected: Signal<Option<String>>,
    pub query: Signal<String>,
    pub message: Signal<String>,
    pub today: Signal<String>,
    pub servers: Signal<Vec<(String, String)>>,
    pub busy: Signal<bool>,
    pub passwords: Signal<BTreeMap<(String, String), String>>,
    pub dirty: Signal<bool>,
    pub sync_message: Signal<String>,
    pub sort: Signal<u8>,
}

pub fn today() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        chrono::Local::now().date_naive().to_string()
    }
    #[cfg(target_arch = "wasm32")]
    {
        #[link(wasm_import_module = "env")]
        unsafe extern "C" {
            fn day_plan_local_date() -> u32;
        }
        // SAFETY: the app's calendar module supplies this zero-argument numeric import.
        let date = unsafe { day_plan_local_date() };
        format!(
            "{:04}-{:02}-{:02}",
            date / 10000,
            (date / 100) % 100,
            date % 100
        )
    }
}

pub fn name(d: &Document) -> String {
    if !d.get("title").trim().is_empty() {
        return d.get("title").into();
    }
    match d.kind {
        Kind::Note => res::str::untitled_note(),
        Kind::Task => res::str::untitled_task(),
        Kind::Group => res::str::untitled_group(),
        Kind::List => res::str::untitled_list(),
        Kind::Step => res::str::untitled_step(),
    }
    .format()
}

pub fn color(code: &str) -> Color {
    Color::hex(match code {
        "mint" => 0x248772,
        "amber" => 0xBD701B,
        "blue" => 0x3979B8,
        "rose" => 0xC45972,
        "slate" => 0x738396,
        _ => 0x7965BE,
    })
}

impl Scene {
    pub fn open(db: Database) -> Result<Self, day_persistence::DbError> {
        let library = db.load()?;
        let servers = db.servers()?;
        Ok(Self {
            owner: day::reactive::Scope::current(),
            db: Signal::new(Rc::new(db)),
            library: Signal::new(library),
            scope: Signal::new(Some("inbox".into())),
            selected: Signal::new(None),
            query: Signal::new(String::new()),
            message: Signal::new(String::new()),
            today: Signal::new(today()),
            servers: Signal::new(servers),
            busy: Signal::new(false),
            passwords: Signal::new(BTreeMap::new()),
            dirty: Signal::new(false),
            sync_message: Signal::new(String::new()),
            sort: Signal::new(0),
        })
    }
    pub fn accept(self, d: Document) {
        if self
            .library
            .with_untracked(|lib| lib.get(&d.id) != Some(&d))
        {
            self.library.update(|lib| {
                lib.insert(d.id.clone(), d);
            });
        }
    }
    pub fn edit(self, id: &str, k: &str, v: String) {
        let result = self.db.get_untracked().edit(id, &[(k.into(), v)]);
        match result {
            Ok(d) => {
                self.accept(d);
                self.dirty.set(true);
                self.message.set(String::new());
            }
            Err(_) => self.message.set(res::str::edit_failed().format()),
        }
    }
    pub fn create(self, kind: Kind, title: String, parent: &str) -> Option<String> {
        let scope = self.scope.get_untracked().unwrap_or_default();
        let mut fields = vec![("title".into(), title)];
        if kind == Kind::Step {
            fields.push((
                "created".into(),
                day::datetime::now_epoch_millis().to_string(),
            ));
        }
        if !parent.is_empty() {
            fields.push(("parent".into(), parent.into()));
        }
        if matches!(kind, Kind::Task | Kind::Note) {
            match scope.as_str() {
                "today" => fields.push(("day".into(), self.today.get_untracked())),
                "planned" => fields.push(("due".into(), self.today.get_untracked())),
                "starred" => fields.push(("star".into(), "1".into())),
                t if t.starts_with("tag:") => fields.push((t.into(), "1".into())),
                _ => {}
            }
        }
        match self.db.get_untracked().create(kind, &fields) {
            Ok(d) => {
                let id = d.id.clone();
                self.accept(d);
                self.dirty.set(true);
                self.message.set(String::new());
                Some(id)
            }
            Err(_) => {
                self.message.set(res::str::edit_failed().format());
                None
            }
        }
    }
    pub fn new_item(self, kind: Kind, title: String) {
        let scope = self.scope.get_untracked().unwrap_or_default();
        let parent = self.library.with_untracked(|lib| {
            lib.get(&scope)
                .filter(|d| d.kind == Kind::List && !d.flag("trash"))
                .map(|d| d.id.clone())
                .unwrap_or_default()
        });
        if let Some(id) = self.create(kind, title, &parent) {
            self.query.set(String::new());
            if matches!(
                scope.as_str(),
                "trash" | "completed" | "organize" | "servers"
            ) || scope.is_empty()
                || (scope == "notes" && kind == Kind::Task)
            {
                self.scope.set(Some("inbox".into()));
            }
            self.selected.set(Some(id));
        }
    }
    pub fn visible(self) -> Vec<Document> {
        let scope = self.scope.get().unwrap_or_default();
        let query = self.query.get().trim().to_lowercase();
        let today = self.today.get();
        let sort = self.sort.get();
        self.library.with(|l| {
            let mut docs: Vec<_> = l
                .values()
                .filter(|d| {
                    in_scope(d, &scope, l, &today)
                        && (query.is_empty()
                            || [d.get("title"), d.get("body")]
                                .iter()
                                .any(|s| s.to_lowercase().contains(&query))
                            || d.tags().iter().any(|s| s.contains(&query)))
                })
                .cloned()
                .collect();
            docs.sort_by_cached_key(|d| {
                (
                    sort == 0 && !d.flag("star"),
                    sort != 1 && d.get("due").is_empty(),
                    if sort == 1 {
                        String::new()
                    } else {
                        d.get("due").into()
                    },
                    name(d).to_lowercase(),
                    d.id.clone(),
                )
            });
            docs
        })
    }
    pub fn field(self, id: &str, k: &str) -> Field<String> {
        self.binding(id, k, str::to_owned, |s| s)
    }
    pub fn flag(self, id: &str, k: &str) -> Field<bool> {
        self.binding(id, k, |s| s == "1", |v| if v { "1" } else { "0" }.into())
    }
    pub fn binding<T: Clone + 'static>(
        self,
        id: &str,
        k: &str,
        decode: fn(&str) -> T,
        encode: fn(T) -> String,
    ) -> Field<T> {
        Field {
            sc: self,
            id: id.into(),
            key: k.into(),
            decode,
            encode,
        }
    }
}
#[derive(Clone)]
pub struct Field<T> {
    sc: Scene,
    id: String,
    key: String,
    decode: fn(&str) -> T,
    encode: fn(T) -> String,
}
impl<T: Clone + 'static> Binding<T> for Field<T> {
    fn read(&self) -> T {
        self.sc
            .library
            .with(|l| (self.decode)(l.get(&self.id).map_or("", |d| d.get(&self.key))))
    }
    fn peek(&self) -> T {
        self.sc
            .library
            .with_untracked(|l| (self.decode)(l.get(&self.id).map_or("", |d| d.get(&self.key))))
    }
    fn write(&self, v: T) {
        self.sc.edit(&self.id, &self.key, (self.encode)(v));
    }
    // Controls may be disposed when navigating after a sync. Every live write is already durable.
    fn write_teardown(&self, _: T) {}
}
