// Copyright © The Daybrite Project
// SPDX-License-Identifier: GPL-2.0-or-later
use crate::{
    accounts, editor,
    model::{Document, Kind, in_scope},
    organize, res,
    scene::{Scene, color, name},
};
use day::prelude::*;
use std::collections::BTreeSet;

day::routes! { enum Smart { Inbox => "inbox", Today => "today", Planned => "planned", Starred => "starred", All => "all", Notes => "notes", Completed => "completed", Trash => "trash" } }

fn smart(key: &str) -> (LocalizedText, VectorName, Color) {
    match key {
        "today" => (res::str::today(), res::vectors::sun, color("amber")),
        "planned" => (res::str::planned(), res::vectors::calendar, color("rose")),
        "starred" => (res::str::starred(), res::vectors::star, color("amber")),
        "all" => (res::str::all(), res::vectors::stack, color("blue")),
        "notes" => (res::str::notes(), res::vectors::kind_note, color("mint")),
        "completed" => (
            res::str::completed(),
            res::vectors::kind_todo,
            color("mint"),
        ),
        "trash" => (res::str::trash(), res::vectors::trash_bin, color("slate")),
        _ => (res::str::inbox(), res::vectors::inbox, color("iris")),
    }
}
fn scope_title(sc: Scene) -> String {
    let key = sc.scope.get().unwrap_or_default();
    if let Some(tag) = key.strip_prefix("tag:") {
        return tag.into();
    }
    sc.library
        .with(|l| l.get(&key).map(name))
        .unwrap_or_else(|| smart(&key).0.format())
}
fn summary(d: &Document, sc: Scene) -> String {
    let date = d.get("due");
    if !date.is_empty() {
        return if date < sc.today.get().as_str() && !d.flag("done") {
            res::str::overdue(date.to_owned())
        } else {
            res::str::due_on(date.to_owned())
        }
        .format();
    }
    if !d.tags().is_empty() {
        return d.tags().join(", ");
    }
    d.get("body")
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(120)
        .collect()
}
fn change_done(sc: Scene, d: Document) {
    sc.edit(&d.id, "done", if d.flag("done") { "0" } else { "1" }.into());
}
fn middle(sc: Scene) -> impl Piece {
    let draft = Signal::new(String::new());
    let capture = move || {
        let v = draft.get_untracked();
        if !v.trim().is_empty() {
            sc.new_item(Kind::Task, v.trim().into());
            if sc.message.get_untracked().is_empty() {
                draft.set(String::new());
            }
        }
    };
    let can_capture = move || !matches!(sc.scope.get().as_deref(), Some("completed" | "trash"));
    let rows = list(
        items(move || sc.visible(), |d: &Document| d.id.clone()),
        move |slot| {
            row((
                switch(
                    move || slot.field(|d| d.kind == Kind::Task),
                    move |is_task| {
                        if *is_task {
                            button(move || {
                                if slot.field(|d| d.flag("done")) {
                                    res::str::reopen()
                                } else {
                                    res::str::done()
                                }
                                .format()
                            })
                            .icon(move || {
                                if slot.field(|d| d.flag("done")) {
                                    Symbol::Check
                                } else {
                                    Symbol::Circle
                                }
                            })
                            .icon_only()
                            .plain()
                            .id_of(move || day::id!(row_done, slot.key()))
                            .action(move || change_done(sc, slot.get()))
                            .any()
                        } else {
                            vector(res::vectors::kind_note)
                                .template()
                                .tint(move || slot.field(|d| color(d.get("color"))))
                                .frame(24.0, 24.0)
                                .decorative()
                                .any()
                        }
                    },
                ),
                column((
                    label(move || slot.field(name))
                        .font(Font::Headline)
                        .single_line(),
                    when(
                        move || !slot.field(|d| summary(d, sc)).is_empty(),
                        move || {
                            label(move || slot.field(|d| summary(d, sc)))
                                .font(Font::Footnote)
                                .secondary()
                                .single_line()
                        },
                    ),
                ))
                .align(HAlign::Leading)
                .spacing(Space::XS)
                .grow(Dimension::Width),
                when(
                    move || slot.field(|d| d.flag("star")),
                    move || {
                        vector(res::vectors::star)
                            .template()
                            .tint(color("amber"))
                            .decorative()
                            .frame(15.0, 15.0)
                    },
                ),
                vector(res::vectors::color_dot)
                    .template()
                    .tint(move || slot.field(|d| color(d.get("color"))))
                    .decorative()
                    .frame(9.0, 9.0),
            ))
            .spacing(Space::S)
            .padding(Space::S)
            .context_menu(move || {
                let d = slot.get();
                let mut menu = vec![];
                if d.kind == Kind::Task && !d.flag("trash") {
                    let task = d.clone();
                    menu.push(
                        menu_item(if d.flag("done") {
                            res::str::reopen()
                        } else {
                            res::str::done()
                        })
                        .action(move || change_done(sc, task.clone())),
                    );
                }
                let trash = d.flag("trash");
                menu.push(
                    menu_item(if trash {
                        res::str::restore()
                    } else {
                        res::str::move_trash()
                    })
                    .action(move || sc.edit(&d.id, "trash", if trash { "0" } else { "1" }.into())),
                );
                menu
            })
        },
    )
    .on_select(move |id: String| sc.selected.set(Some(id)))
    .selected_rows(move || {
        sc.selected
            .get()
            .and_then(|id| sc.visible().iter().position(|d| d.id == id))
            .into_iter()
            .collect()
    })
    .swipe_leading(move |i| {
        let row = sc.visible().get(i).cloned();
        row.filter(|d| d.kind == Kind::Task && !d.flag("trash"))
            .map(|d| {
                swipe_action(if d.flag("done") {
                    res::str::reopen()
                } else {
                    res::str::done()
                })
                .symbol(Symbol::Check)
                .action(move || change_done(sc, d.clone()))
            })
            .into_iter()
            .collect()
    })
    .empty(move || {
        column((
            vector(move || smart(sc.scope.get().as_deref().unwrap_or_default()).1)
                .template()
                .tint(color("iris"))
                .frame(38.0, 38.0)
                .decorative(),
            label(move || {
                if !sc.query.get().is_empty() {
                    res::str::empty_search()
                } else {
                    match sc.scope.get().as_deref() {
                        Some("today") => res::str::empty_today(),
                        Some("inbox") => res::str::empty_inbox(),
                        _ => res::str::empty_list(),
                    }
                }
                .format()
            })
            .font(Font::Headline),
            when(
                move || can_capture() && sc.query.get().is_empty(),
                move || {
                    label(move || {
                        if sc.scope.get().as_deref() == Some("today") {
                            res::str::empty_day_hint()
                        } else {
                            res::str::empty_capture()
                        }
                        .format()
                    })
                    .secondary()
                    .align(TextAlign::Center)
                },
            ),
        ))
        .spacing(Space::M)
        .padding(Space::L)
    })
    .id(day::id!(item_list))
    .grow(Dimension::Both);
    column((
        column((
            row((
                column((
                    label(move || scope_title(sc))
                        .font(Font::Title)
                        .weight(FontWeight::Bold)
                        .color(Color::hex(0xFFFFFF))
                        .a11y(|a| a.role(Role::Heading(1))),
                    label(move || res::str::item_count(sc.visible().len() as i64).format())
                        .font(Font::Footnote)
                        .color(Color::hex(0xEAE2F0)),
                ))
                .align(HAlign::Leading)
                .spacing(Space::XS)
                .grow(Dimension::Width),
                vector(res::vectors::hills).frame(105.0, 64.0).decorative(),
            ))
            .padding(Space::M)
            .grow(Dimension::Width)
            .background(Color::hex(0x514468))
            .corner_radius(12.0),
            when(can_capture, move || {
                row((
                    text_field(draft)
                        .placeholder(res::str::quick_capture())
                        .id(day::id!(quick_capture))
                        .on_submit(capture)
                        .grow(Dimension::Width),
                    button(res::str::add())
                        .icon(Symbol::Add)
                        .icon_only()
                        .id(day::id!(capture))
                        .action(capture),
                ))
                .spacing(Space::S)
            }),
            labeled(
                res::str::sort(),
                picker(
                    sc.sort,
                    [
                        (0, res::str::sort_focus()),
                        (1, res::str::sort_title()),
                        (2, res::str::sort_due()),
                    ],
                )
                .id(day::id!(sort)),
            ),
        ))
        .align(HAlign::Leading)
        .spacing(Space::M)
        .padding(Space::L),
        rows,
    ))
    .grow(Dimension::Both)
    .searchable(Search::new(sc.query).prompt(res::str::search()))
    .toolbar(vec![
        toolbar_button(day::id!(new_task), res::str::new_task())
            .icon(Symbol::Add)
            .action(move || sc.new_item(Kind::Task, String::new())),
        toolbar_button(day::id!(new_note), res::str::new_note())
            .icon(Symbol::New)
            .action(move || sc.new_item(Kind::Note, String::new())),
    ])
}
#[derive(Clone, Copy)]
struct Selection(Scene);
impl Binding<Option<String>> for Selection {
    fn read(&self) -> Option<String> {
        self.0.scope.get()
    }
    fn peek(&self) -> Option<String> {
        self.0.scope.get_untracked()
    }
    fn write(&self, v: Option<String>) {
        if self.peek() != v {
            self.0.selected.set(None);
            self.0.query.set(String::new());
            self.0.scope.set(v);
        }
    }
}
pub fn shell(sc: Scene, prefs: day_piece_settings::Prefs) -> impl Piece {
    let smart_row = move |route: Smart| {
        let k = route.key();
        let (title, icon, tint) = smart(&k);
        item(k.to_string(), title)
            .icon(icon)
            .icon_tint(tint)
            .badge(move || {
                let today = sc.today.get();
                let n = sc
                    .library
                    .with(|l| l.values().filter(|d| in_scope(d, &k, l, &today)).count());
                if n > 0 {
                    day::format_decimal(n as f64, 0)
                } else {
                    String::new()
                }
            })
    };
    let nav = nav(Selection(sc))
        .style(NavStyle::Sidebar)
        .title(res::str::app_title())
        .content_list(
            ContentList::new(move || middle(sc))
                .width(370.0)
                .only_for(|k: &Option<String>| {
                    !matches!(k.as_deref(), Some("organize" | "servers" | "settings"))
                })
                .detail_for(sc.selected)
                .detail_title(move || {
                    sc.library.with(|l| {
                        sc.selected
                            .get()
                            .and_then(|id| l.get(&id).map(name))
                            .unwrap_or_else(|| res::str::details().format())
                    })
                }),
        )
        .section(
            res::str::workspace(),
            (
                smart_row(Smart::Inbox),
                smart_row(Smart::Today),
                smart_row(Smart::Planned),
                smart_row(Smart::Starred),
                smart_row(Smart::All),
                smart_row(Smart::Notes),
                smart_row(Smart::Completed),
                smart_row(Smart::Trash),
            ),
        )
        .items(nav_items(
            move || {
                sc.library.with(|l| {
                    let mut rows: Vec<_> = l
                        .values()
                        .filter(|d| d.kind == Kind::List && !d.flag("trash"))
                        .map(|d| {
                            let g = l
                                .get(d.get("parent"))
                                .filter(|g| g.kind == Kind::Group && !g.flag("trash"));
                            (
                                g.map(name).unwrap_or_else(|| res::str::lists().format()),
                                g.map(|g| g.id.clone()).unwrap_or_default(),
                                name(d),
                                d.id.clone(),
                                d.get("color").to_owned(),
                            )
                        })
                        .collect();
                    rows.sort();
                    let mut last = None;
                    rows.into_iter()
                        .map(|(group, gid, name, id, tint)| {
                            let header =
                                (last.as_ref() != Some(&gid)).then(|| (gid.clone(), group));
                            last = Some(gid);
                            (name, id, tint, header)
                        })
                        .collect::<Vec<_>>()
                })
            },
            |(name, id, tint, header): &(String, String, String, Option<(String, String)>)| {
                let row = item(id.clone(), name.clone())
                    .icon(res::vectors::stack)
                    .icon_tint(color(tint));
                if let Some((id, title)) = header {
                    row.section_id(format!("group-{id}"), title.clone())
                } else {
                    row
                }
            },
        ))
        .section(
            res::str::tags(),
            nav_items(
                move || {
                    sc.library.with(|l| {
                        l.values()
                            .filter(|d| !d.flag("trash"))
                            .flat_map(Document::tags)
                            .collect::<BTreeSet<_>>()
                            .into_iter()
                            .collect::<Vec<_>>()
                    })
                },
                |tag: &String| {
                    item(format!("tag:{tag}"), tag.clone())
                        .icon(res::vectors::tag)
                        .icon_tint(color("mint"))
                },
            ),
        )
        .section(
            res::str::tools(),
            (
                item("organize".to_string(), res::str::organize())
                    .icon(res::vectors::stack)
                    .page(move || organize::page(sc)),
                item("servers".to_string(), res::str::servers())
                    .icon(res::vectors::cloud)
                    .page(move || accounts::page(sc)),
                prefs.nav_item(Some("settings".to_owned())),
            ),
        )
        .destination(move |_: &Option<String>| editor::editor(sc))
        .id(day::id!(nav));
    column((
        nav.grow(Dimension::Both),
        when(
            move || !sc.message.get().is_empty(),
            move || label(move || sc.message.get()).padding(Space::S),
        ),
    ))
    .grow(Dimension::Both)
}
