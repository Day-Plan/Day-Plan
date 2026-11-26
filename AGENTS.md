# Day-Plan

Read the project README for build instructions, platform dependencies, and the sync model.
Keep all app-owned UI text in the Fluent catalogs and use generated Day resource accessors.
Use stable IDs for dayscript controls. Test fixtures may use synthetic English text.
Use `GPL-2.0-or-later` in the first five lines of app-owned Rust files, including `build.rs`.
Preserve the original license and attribution when copying third-party code.

Preserve the independent field histories and the transaction that verifies local state before
saving a server receipt. Exercise both delivery orders when changing conflict rules. Run
`cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
Run the affected native walkthroughs with the in-memory library. The App Fair flavor shares
the published bundle ID by design, so its flavor-id-collision lint is expected.
