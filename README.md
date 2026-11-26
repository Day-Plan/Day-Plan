# Day-Plan

Day-Plan is an offline notes and task app built with Day. Its sidebar, item list, and editor share a window on desktop and become a navigation stack on phones. The same Rust application serves macOS, iPhone and iPad, Android, GTK and Qt on Linux, Windows, HarmonyOS, and the browser.

Capture a task in Inbox or start a note from the toolbar. Give items a color, due date, list, tags, and checklist steps. My Day includes manually selected items, items due today, and overdue work. Finishing a task moves it to Completed. Trash retains the contents of an item for restoration. Removing a list returns its items to Inbox, and restoring it reconnects them. Groups organize lists.

## Build

The repository is [Day-Plan/Day-Plan](https://github.com/Day-Plan/Day-Plan). The app uses the Day CLI and Rust 1.100 or later. Development currently uses the companion changes in the day-sync and day-webdav checkouts for portable document exchange and WebDAV conditional-write verification. From this project, supply the paths to those checkouts and their matching Day and day-persistence sources.

```sh
day patch --local /path/to/day --local /path/to/day-persistence --local /path/to/day-sync --local /path/to/day-webdav
cargo test
day launch -t macos-appkit
day launch -t web-dom
```

`cargo test` runs the merge laws, SQLite reopen and rollback cases, two-server synchronization, and request-level fault tests. The WebDAV integration tests bind ephemeral loopback ports and use synthetic data. Run the portable UI walkthrough on a device with `day launch -t ios-uikit --env DAY_PLAN_WALKTHROUGH=1 --script dayscript/demo.yaml --script dayscript/status.yaml`. The walkthrough uses an in-memory library when that variable is set. Its captures supply the storefront gallery. For software-emulated HarmonyOS, use the companion Day CLI and runtime with per-screenshot timeout support.

## Synchronization

Under **Sync servers**, enter an existing writable HTTPS collection, username, and app password. Day-Plan probes the collection with a disposable file before enrolling it. Every connected collection receives the full planner under `day-plan-v1/`. Use private collections and accounts you trust with the contents. Stored notes and sync files are readable to the server operator. Native builds keep credentials in the platform keychain. Browsers retain the password for the current session and require a server that permits cross-origin WebDAV and exposes ETag. The web app also requires Day's cross-origin-isolated host for durable SQLite storage.

The local SQLite database stores each document and its causal history together. Task status, title, notes, dates, list membership, color, and each tag have separate multi-value registers. Concurrent title and note edits are available in the editor until a version is chosen. Completion and trash take precedence over concurrent reopen and restore. A later edit made after observing both versions supersedes them. A missing server file is repaired from local state.

Each pass reads the server's complete collection listings, skips unchanged strong validators, merges each changed document and conditionally writes it. A verified readback is merged with current SQLite state inside one transaction. A receipt is saved only if the local result equals the verified server document. Sync runs while the app is open, after edits, and on demand. A device connected to multiple servers relays changes between them. Disconnecting removes that server from this device. The copies on the device and server remain available.

## Project structure

- `src/model.rs` defines document validation, merge rules, and smart views.
- `src/database.rs` owns SQLite transactions and synchronization receipts.
- `src/sync.rs` exchanges planner documents through day-sync and day-webdav.
- `src/scene.rs`, `navigation.rs`, `editor.rs`, `organize.rs`, and `accounts.rs` connect the model to native controls.
- `resource/locales/en/app.ftl` holds the English catalog. UI code uses generated accessors. Future translations can use the same keys.

## App Fair distribution

The base and App Fair flavors both use `org.appfair.app.SkipNotes`. The Apple listing is 6740916318. Version 1.0.0 starts at build 60, following Skip Notes build 59. The new app opens its own database. Export any needed Skip Notes data before installing the replacement.

`day --flavor appfair store stage` produces Fastlane metadata for the Apple App Store, Google Play, and Mac App Store from `store/storefront.toml`. After capturing the gallery on a Mac, large iPhone, 13-inch iPad, Android phone and tablet, stage and validate the images with the listings.

```sh
day screenshot index --out build/day/screenshots/gallery.json
day --flavor appfair store screenshots build/day/screenshots/gallery.json
day --flavor appfair store stage --screenshots build/day/screenshots/gallery.json
```

The macOS distribution build is sandboxed with outgoing network access. The workflow builds the declared targets and runs the screenshot walkthrough. Publishing the shared dependency changes and resolving the release lock against those revisions precedes a tagged store build. Signing, App Fair submission, and store upload are release actions.

## License

Day-Plan’s application code and original assets are licensed under the GNU General Public License, version 2 or (at your option) any later version. [GPL version 2 license text](LICENSE.txt)

The Day framework and its native runner templates use the Mozilla Public License 2.0. Other dependencies retain their own licenses and notices. [Day dependency license](LICENSES/MPL-2.0.txt)

For each release, review direct and transitive dependencies for public source, DFSG-compliant licenses, and terms that permit distribution with Day-Plan through its stores. For each prebuilt dependency, record its source revision, build instructions, build environment, and evidence that rebuilding reproduces the shipped binary. [App Fair inclusion criteria](https://appfair.org/docs/inclusion-criteria/#license-and-dependencies)
