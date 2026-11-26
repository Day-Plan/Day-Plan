// Copyright © The Daybrite Project
// SPDX-License-Identifier: GPL-2.0-or-later
use crate::{
    model::{Document, Kind, tag_name},
    res,
    scene::{Field, Scene, name},
};
use day::{datetime::DayDate, prelude::*};

pub fn palette(sc: Scene, id: &str) -> impl Piece + use<> {
    let options = [
        ("iris", res::str::iris()),
        ("mint", res::str::mint()),
        ("rose", res::str::rose()),
        ("amber", res::str::amber()),
        ("blue", res::str::blue()),
        ("slate", res::str::slate()),
    ]
    .map(|(k, v)| (k.to_string(), v));
    let binding = sc.binding(
        id,
        "color",
        |v| {
            if v.is_empty() {
                "iris".into()
            } else {
                v.into()
            }
        },
        |s| s,
    );
    labeled(
        res::str::color(),
        picker(binding, options).id(day::id!(item_color)),
    )
}

pub fn parent(sc: Scene, id: &str, kind: Kind) -> impl Piece + use<> {
    let group = kind == Kind::Group;
    labeled(
        if group {
            res::str::group()
        } else {
            res::str::list()
        },
        picker(sc.field(id, "parent"), move || {
            let mut options = sc.library.with(|l| {
                l.values()
                    .filter(|d| d.kind == kind && !d.flag("trash"))
                    .map(|d| (d.id.clone(), name(d)))
                    .collect::<Vec<_>>()
            });
            options.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
            options.insert(
                0,
                (
                    String::new(),
                    if group {
                        res::str::no_group()
                    } else {
                        res::str::inbox()
                    }
                    .format(),
                ),
            );
            options
        })
        .id(day::id!(parent_picker)),
    )
}
#[derive(Clone)]
struct DateToggle {
    field: Field<String>,
    today: Signal<String>,
    exact: bool,
}
impl DateToggle {
    fn matches(&self, v: &str, date: &str) -> bool {
        if self.exact { v == date } else { !v.is_empty() }
    }
}
impl Binding<bool> for DateToggle {
    fn read(&self) -> bool {
        self.matches(&self.field.read(), &self.today.get())
    }
    fn peek(&self) -> bool {
        self.matches(&self.field.peek(), &self.today.get_untracked())
    }
    fn write(&self, v: bool) {
        self.field.write(if v {
            self.today.get_untracked()
        } else {
            String::new()
        });
    }
    fn write_teardown(&self, _: bool) {}
}
fn tags(sc: Scene, id: &str) -> impl Piece + use<> {
    let input = Signal::new(String::new());
    let id = Signal::new(id.to_owned());
    let add = move || {
        let tag = tag_name(&input.get_untracked());
        if !tag.is_empty() {
            sc.edit(&id.get_untracked(), &format!("tag:{tag}"), "1".into());
            if sc.message.get_untracked().is_empty() {
                input.set(String::new());
            }
        }
    };
    section((
        each(
            items(
                move || {
                    sc.library
                        .with(|l| l.get(&id.get()).map(Document::tags).unwrap_or_default())
                },
                Clone::clone,
            ),
            move |s| {
                button(move || res::str::remove_tag(s.get()).format()).action(move || {
                    sc.edit(&id.get_untracked(), &format!("tag:{}", s.get()), "0".into())
                })
            },
        ),
        row((
            text_field(input)
                .placeholder(res::str::tag_hint())
                .id(day::id!(tag_input))
                .on_submit(add)
                .grow(Dimension::Width),
            button(res::str::add_tag())
                .id(day::id!(add_tag))
                .action(add),
        ))
        .spacing(Space::S),
    ))
    .title(res::str::tags())
}
fn steps(sc: Scene, id: &str) -> impl Piece + use<> {
    let input = Signal::new(String::new());
    let parent = Signal::new(id.to_owned());
    let add = move || {
        let s = input.get_untracked();
        if !s.trim().is_empty()
            && sc
                .create(Kind::Step, s.trim().into(), &parent.get_untracked())
                .is_some()
        {
            input.set(String::new());
        }
    };
    section((
        each(
            items(
                move || {
                    sc.library.with(|l| {
                        let mut steps = l
                            .values()
                            .filter(|d| {
                                d.kind == Kind::Step
                                    && d.get("parent") == parent.get()
                                    && !d.flag("trash")
                            })
                            .collect::<Vec<_>>();
                        steps.sort_by_key(|d| {
                            (d.get("created").parse::<u64>().unwrap_or_default(), &d.id)
                        });
                        steps.into_iter().map(|d| d.id.clone()).collect::<Vec<_>>()
                    })
                },
                Clone::clone,
            ),
            move |s| {
                let id = s.get();
                row((
                    toggle(sc.flag(&id, "done")).a11y(|a| a.label(res::str::done())),
                    text_field(sc.field(&id, "title")).grow(Dimension::Width),
                    button(res::str::remove_step())
                        .icon(Symbol::Remove)
                        .icon_only()
                        .action(move || sc.edit(&s.get(), "trash", "1".into())),
                ))
                .spacing(Space::S)
            },
        ),
        row((
            text_field(input)
                .placeholder(res::str::step_hint())
                .id(day::id!(step_input))
                .on_submit(add)
                .grow(Dimension::Width),
            button(res::str::add_step())
                .id(day::id!(add_step))
                .action(add),
        ))
        .spacing(Space::S),
    ))
    .title(res::str::steps())
}

fn versions(sc: Scene, id: &str, key: &'static str) -> impl Piece + use<> {
    let id = Signal::new(id.to_owned());
    let values = move || {
        sc.library.with(|l| {
            l.get(&id.get())
                .map(|d| d.alternatives(key))
                .unwrap_or_default()
        })
    };
    when(
        move || values().len() > 1,
        move || {
            section((each(items(values, Clone::clone), move |s| {
                column((
                    label(move || s.get()),
                    button(res::str::use_version())
                        .action(move || sc.edit(&id.get_untracked(), key, s.get())),
                ))
                .align(HAlign::Leading)
            }),))
            .title(res::str::alternate_versions())
        },
    )
}

fn form_for(sc: Scene, id: &str) -> impl Piece + use<> {
    let id = Signal::new(id.to_owned());
    let k = id.get_untracked();
    let is_task = sc
        .library
        .with_untracked(|l| l.get(&k).is_some_and(|d| d.kind == Kind::Task));
    let due = sc.field(&k, "due");
    let trashed = move || {
        sc.library
            .with(|l| l.get(&id.get()).is_some_and(|d| d.flag("trash")))
    };
    scroll(
        form((
            section((
                labeled(
                    res::str::title(),
                    text_field(sc.field(&k, "title")).id(day::id!(item_title)),
                ),
                text_area(sc.field(&k, "body"))
                    .placeholder(res::str::body_hint())
                    .a11y(|a| a.label(res::str::body()))
                    .min_lines(4)
                    .max_lines(14)
                    .id(day::id!(item_body)),
                versions(sc, &k, "title"),
                versions(sc, &k, "body"),
            ))
            .title(if is_task {
                res::str::task()
            } else {
                res::str::note()
            }),
            section((
                when(
                    move || is_task,
                    move || {
                        labeled(
                            res::str::done(),
                            toggle(sc.flag(&id.get(), "done")).id(day::id!(item_done)),
                        )
                    },
                ),
                labeled(
                    res::str::star(),
                    toggle(sc.flag(&k, "star")).id(day::id!(item_star)),
                ),
                labeled(
                    res::str::my_day(),
                    toggle(DateToggle {
                        field: sc.field(&k, "day"),
                        today: sc.today,
                        exact: true,
                    })
                    .id(day::id!(item_today)),
                ),
                labeled(
                    res::str::schedule(),
                    toggle(DateToggle {
                        field: due.clone(),
                        today: sc.today,
                        exact: false,
                    })
                    .id(day::id!(schedule)),
                ),
                when(
                    move || !due.read().is_empty(),
                    move || {
                        labeled(
                            res::str::due(),
                            date_picker(sc.binding(
                                &id.get(),
                                "due",
                                |s| DayDate::parse_iso(s).unwrap_or_default(),
                                |d| d.to_string(),
                            ))
                            .id(day::id!(item_due)),
                        )
                    },
                ),
                parent(sc, &k, Kind::List),
                palette(sc, &k),
            ))
            .title(res::str::details()),
            steps(sc, &k),
            tags(sc, &k),
            section((button(move || {
                if trashed() {
                    res::str::restore()
                } else {
                    res::str::move_trash()
                }
                .format()
            })
            .id(day::id!(item_trash))
            .action(move || {
                sc.edit(
                    &id.get_untracked(),
                    "trash",
                    if trashed() { "0" } else { "1" }.into(),
                );
                if sc.message.get_untracked().is_empty() {
                    sc.selected.set(None);
                }
            }),)),
        ))
        .padding(Space::M),
    )
}
pub fn editor(sc: Scene) -> impl Piece + use<> {
    column((
        when(
            move || sc.selected.get().is_none(),
            move || {
                column((
                    spacer(),
                    vector(res::vectors::planner_mark)
                        .decorative()
                        .frame(84.0, 84.0),
                    label(res::str::choose_item())
                        .font(Font::Title)
                        .a11y(|a| a.role(Role::Heading(1))),
                    label(res::str::empty_detail()).secondary(),
                    spacer(),
                ))
                .spacing(Space::M)
                .padding(Space::XL)
                .grow(Dimension::Both)
            },
        ),
        keyed(move || sc.selected.get(), move |id| form_for(sc, id)),
    ))
    .grow(Dimension::Both)
}
