// Copyright © The Daybrite Project
// SPDX-License-Identifier: GPL-2.0-or-later
use day::prelude::*;
use day_persistence::Sqlite;
use day_piece_settings::prelude::*;
mod accounts;
mod database;
mod editor;
mod model;
mod navigation;
mod organize;
mod scene;
mod sync;

day::resources!();
day::day_start!(options: res::window_options(), root);

fn open() -> Result<scene::Scene, day_persistence::DbError> {
    if day::env("DAY_PLAN_WALKTHROUGH").as_deref() == Some("1") {
        return scene::Scene::open(database::Database::open(Sqlite::memory())?);
    }
    #[cfg(not(target_arch = "wasm32"))]
    let driver = Sqlite::app_data("day-plan-v1.sqlite")?;
    #[cfg(target_arch = "wasm32")]
    let driver = Sqlite::at("day-plan-v1.sqlite");
    scene::Scene::open(database::Database::open(driver)?)
}
pub fn root() -> impl Piece {
    let prefs = Preferences::new()
        .about(About::from_manifest().website("https://github.com/Day-Plan/Day-Plan"))
        .group(general_group(res::locales::ALL))
        .install();
    let attempt = Signal::new(0u32);
    switch(
        move || attempt.get(),
        move |_| {
            let Ok(sc) = open() else {
                return column((
                    label(res::str::storage_failed()),
                    button(res::str::retry()).action(move || attempt.update(|v| *v += 1)),
                ))
                .padding(Space::XL)
                .any();
            };
            app_menu(vec![
                sub_menu(
                    res::str::menu_file(),
                    vec![
                        menu_item(res::str::new_task())
                            .shortcut(Shortcut::primary("n"))
                            .action(move || sc.new_item(model::Kind::Task, String::new())),
                        menu_item(res::str::new_note())
                            .shortcut(Shortcut::primary("n").shift())
                            .action(move || sc.new_item(model::Kind::Note, String::new())),
                        menu_role(MenuRole::CloseWindow),
                    ],
                ),
                sub_menu(
                    res::str::menu_edit(),
                    vec![
                        menu_item(res::str::find())
                            .shortcut(Shortcut::primary("f"))
                            .action(day::focus_search),
                        menu_role(MenuRole::Cut),
                        menu_role(MenuRole::Copy),
                        menu_role(MenuRole::Paste),
                        menu_role(MenuRole::SelectAll),
                    ],
                ),
            ]);
            day::task(async move {
                accounts::sync(sc);
                let mut ticks = 0;
                loop {
                    day::sleep(5000).await;
                    ticks += 1;
                    sc.today.set_if_changed(scene::today());
                    if sc.dirty.get_untracked() || ticks == 12 {
                        accounts::sync(sc);
                    }
                    if ticks == 12 {
                        ticks = 0;
                    }
                }
            });
            navigation::shell(sc, prefs.clone()).any()
        },
    )
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
