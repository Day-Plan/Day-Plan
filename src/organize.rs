// Copyright © The Daybrite Project
// SPDX-License-Identifier: GPL-2.0-or-later
use crate::{
    editor::{palette, parent},
    model::Kind,
    res,
    scene::Scene,
};
use day::prelude::*;
fn create_row(sc: Scene, kind: Kind) -> impl Piece {
    let draft = Signal::new(String::new());
    let add = move || {
        let v = draft.get_untracked();
        if !v.trim().is_empty() && sc.create(kind, v.trim().to_owned(), "").is_some() {
            draft.set(String::new());
        }
    };
    row((
        text_field(draft)
            .placeholder(if kind == Kind::Group {
                res::str::group_name()
            } else {
                res::str::list_name()
            })
            .id(if kind == Kind::Group {
                day::id!(group_name)
            } else {
                day::id!(list_name)
            })
            .on_submit(add)
            .grow(Dimension::Width),
        button(if kind == Kind::Group {
            res::str::new_group()
        } else {
            res::str::new_list()
        })
        .id(if kind == Kind::Group {
            day::id!(new_group)
        } else {
            day::id!(new_list)
        })
        .action(add),
    ))
    .spacing(Space::S)
}
fn collection(sc: Scene, kind: Kind) -> impl Piece {
    section((
        create_row(sc, kind),
        each(
            items(
                move || {
                    sc.library.with(|l| {
                        let mut rows: Vec<_> = l
                            .values()
                            .filter(|d| d.kind == kind)
                            .map(|d| (d.flag("trash"), d.id.clone()))
                            .collect();
                        rows.sort();
                        rows
                    })
                },
                |a: &(bool, String)| a.1.clone(),
            ),
            move |slot| {
                let id = slot.get().1;
                column((
                    text_field(sc.field(&id, "title")).a11y(move |a| {
                        a.label(if kind == Kind::Group {
                            res::str::group_name()
                        } else {
                            res::str::list_name()
                        })
                    }),
                    palette(sc, &id),
                    when(
                        move || kind == Kind::List,
                        move || parent(sc, &slot.get().1, Kind::Group),
                    ),
                    button(move || {
                        if slot.get().0 {
                            res::str::restore()
                        } else {
                            res::str::move_trash()
                        }
                        .format()
                    })
                    .action(move || {
                        let (trash, id) = slot.get();
                        sc.edit(&id, "trash", if trash { "0" } else { "1" }.into());
                    }),
                ))
                .align(HAlign::Leading)
                .spacing(Space::S)
                .padding(Space::S)
            },
        ),
    ))
    .title(if kind == Kind::Group {
        res::str::groups()
    } else {
        res::str::lists()
    })
}
pub fn page(sc: Scene) -> impl Piece {
    scroll(
        form((
            label(res::str::organize_intro()).secondary(),
            collection(sc, Kind::Group),
            collection(sc, Kind::List),
        ))
        .padding(Space::L),
    )
}
