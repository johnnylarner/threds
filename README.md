# Threds

An offline Markdown notebook for iOS, built with Rust and GPUI. The first app slice has a local note library, search, new notes, and a Markdown source editor.

Notes stay in the app's `Documents/notes` folder. Existing `.md` filenames stay intact. New notes receive UUID filenames. Editing preserves UTF-8 source outside the replaced range, including BOM and CRLF line endings.

The editor saves after a 400 ms pause, when leaving a note, and when the app becomes inactive or enters the background. "Saved on device" means the save completed. A failed save keeps the note open with a Retry action. Sudden termination can lose changes made during the unsaved interval. Recovery copies hold the previous good version.

Long lines scroll horizontally. Line wrapping, rendered Markdown, GitHub sync, and physical-device signing and validation remain deferred.

## Run the app

Use an Apple Silicon Mac with full Xcode, an installed iOS Simulator runtime, Python 3, and Rust 1.98.1. Complete Xcode first-launch setup and review its license in person. See the [development runbook](docs/development.md) for tool setup, simulator selection, logs, and interaction checks.

From the repository root, build and install on a booted simulator. Replace the placeholder with its UDID.

```sh
cargo test --locked -p threds-core
scripts/build-simulator.sh
scripts/install-simulator.sh '<booted-simulator-UDID>'
```

The build creates `target/simulator/Threds.app` with bundle identifier `dev.threds.app`. Use the [manual checkpoint](docs/development.md#check-the-writing-workflow) to verify the assembled app. Host tests and a simulator build do not establish that iOS input works.

## Project structure

`crates/core` contains the document model and filesystem store. Its host tests do not compile GPUI. `crates/app` contains the iOS app. `AppSession` owns the active document and save policy. `EditorView` holds focus, layout, and scroll state.

GPUI and GPUI Platform use Zed revision `6ceb6fee8738274589b59139df27efc191ec2231`. The root `async-task` patch pins the upstream dispatcher dependency. `Cargo.lock` is tracked. No sibling checkout or absolute dependency path is needed.
