# Build and test the offline app

Run these commands from the repository root. Use the app scripts in this checkout, rather than the upstream GPUI feasibility example.

## Prepare the tools

Use an Apple Silicon Mac, full Xcode, Python 3, and Rust through rustup. Open Xcode and complete its first-launch setup. The user must review and accept the Xcode license directly. Do not automate license acceptance. Install an iOS Simulator runtime and the Metal toolchain through Xcode Settings, Components if either is missing.

Check the selected Xcode and its build tools.

```sh
xcode-select -p
xcodebuild -version
xcrun --sdk iphonesimulator --show-sdk-path
xcrun --sdk iphonesimulator --find metal
xcrun --sdk iphonesimulator --find metallib
xcrun simctl list devices available
python3 --version
```

If `xcode-select -p` points at Command Line Tools, select your full Xcode installation before continuing.

```sh
sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer
```

Install the pinned Rust toolchain, the formatting component, and the Apple Silicon simulator target. `rust-toolchain.toml` selects the same toolchain and target inside the checkout.

```sh
rustup toolchain install 1.98.1 --profile minimal --component rustfmt
rustup target add --toolchain 1.98.1 aarch64-apple-ios-sim
rustc --version
rustup target list --installed --toolchain 1.98.1
```

## Choose and boot a simulator

Choose an available iPhone UDID from `simctl`. Replace the placeholder below and keep this shell open for subsequent commands. Skip `simctl boot` if the chosen device is already booted.

```sh
export THREDS_SIMULATOR_UDID='<available-iPhone-UDID>'
export THREDS_EVIDENCE_DIR="$PWD/.audit/offline-app"
mkdir -p "$THREDS_EVIDENCE_DIR"
xcrun simctl boot "$THREDS_SIMULATOR_UDID"
xcrun simctl bootstatus "$THREDS_SIMULATOR_UDID" -b
```

Xcode 27.2 on this development machine presents simulator windows in Device Hub. Open it by bundle identifier, then select the chosen device in its UI.

```sh
open -b com.apple.dt.Devices
```

Older Xcode versions use the Simulator application. On those installations, open the selected device with this command.

```sh
open -a Simulator --args -CurrentDeviceUDID "$THREDS_SIMULATOR_UDID"
```

Do not assume that an app named Simulator exists on Xcode 27. Confirm the visible device before interacting. Use an explicit UDID for `simctl` operations, especially when several devices are booted.

## Test, build, and install

Run the document and filesystem tests on the Mac, then build the iOS app. The build script fixes the target directory, packages the executable and Info.plist, and applies an ad hoc simulator signature. It forwards additional arguments to Cargo, so `scripts/build-simulator.sh --release` builds a release executable into the same app bundle.

```sh
cargo fmt --all -- --check
cargo test --locked -p threds-core
bash -o pipefail -c 'scripts/build-simulator.sh 2>&1 | tee "$THREDS_EVIDENCE_DIR/build.log"'
plutil -lint scripts/Info.plist target/simulator/Threds.app/Info.plist
codesign --verify --deep --strict target/simulator/Threds.app
/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' target/simulator/Threds.app/Info.plist
scripts/install-simulator.sh "$THREDS_SIMULATOR_UDID"
```

The bundle identifier must be `dev.threds.app`. The install script terminates a running instance, installs `target/simulator/Threds.app`, and launches it. It defaults to `booted` when no UDID is supplied. Pass the UDID to avoid ambiguity.

The core tests cover text replacement, UTF-16 ranges, composition state, undo, filesystem saves, and recovery. They do not exercise UIKit input, GPUI layout, the software keyboard, or simulator lifecycle. Use the core test command independently of the iOS app build. This repository has no frontend npm checks.

## Inspect logs and restart the app

Record the source revision, Xcode version, and available devices alongside the evidence.

```sh
git rev-parse HEAD > "$THREDS_EVIDENCE_DIR/source-revision.txt"
xcodebuild -version > "$THREDS_EVIDENCE_DIR/xcode-version.txt"
xcrun simctl list devices available > "$THREDS_EVIDENCE_DIR/devices.txt"
```

For an attached app console, terminate the app and launch it with `--console`. The command stays attached until the app exits or you interrupt the console.

```sh
xcrun simctl terminate "$THREDS_SIMULATOR_UDID" dev.threds.app
xcrun simctl launch --console "$THREDS_SIMULATOR_UDID" dev.threds.app
```

For recent system logs, run this command in another shell with the same UDID.

```sh
xcrun simctl spawn "$THREDS_SIMULATOR_UDID" log show --last 5m --style compact --predicate 'process == "threds"' > "$THREDS_EVIDENCE_DIR/runtime.log"
```

After the app reports "Saved on device", check persistence by terminating and relaunching. Do not uninstall the app, because uninstalling deletes its sandbox.

```sh
xcrun simctl terminate "$THREDS_SIMULATOR_UDID" dev.threds.app
xcrun simctl launch "$THREDS_SIMULATOR_UDID" dev.threds.app
xcrun simctl get_app_container "$THREDS_SIMULATOR_UDID" dev.threds.app data
```

The last command prints the data-container path. Notes are under `Documents/notes` in that container. Recovery copies are under `Documents/notes/.recovery`. Inspect only synthetic fixtures when collecting evidence. Do not print private notes into logs or PRs.

## Check the writing workflow

Use a fresh synthetic note, such as a heading "Simulator checklist" followed by paragraphs, emoji, and a line wider than the screen. Observe each result before marking it complete.

1. Open the library and tap **+ New**. Type the fixture with the software keyboard. Press Return in the editor and confirm it inserts a new line. Check caret visibility, horizontal scrolling, vertical scrolling, and keyboard safe areas.
2. Pause for at least 400 ms and wait for **Saved on device**. Edit again and tap **Done**. Confirm the library shows the updated title and excerpt, then reopen the note and check the text.
3. Return to the library and enter a matching query in **Search notes**. Confirm the matching note appears. Press the keyboard's Search or Return key. Confirm the keyboard dismisses without adding a new line to the query. Open the result, then clear the query and confirm the full library returns.
4. Edit the note, immediately background the app, and reopen it. Confirm the change remains. Wait for **Saved on device**, terminate and relaunch with the commands above, and confirm the exact text remains.
5. Check backspace across lines, long-press selection, selection adjustment, copy, paste, undo, autocorrect, and non-Latin composition. Check light and dark appearance, portrait and landscape, and keyboard dismissal and return.

Record the device, runtime, revision, actions, and observed results. User feedback that the basic app looks good does not certify every input workflow above. Keep unobserved checks pending. Physical-device signing and interaction remain deferred. Long source lines scroll horizontally. The app does not wrap or render Markdown, and it does not sync with GitHub.

## Capture review evidence

Keep logs, screenshots, and recordings under ignored `.audit/`. Use only synthetic notes. Never capture private writing, contacts, or credentials. For functional PRs, show the starting state, deliberate actions, and resulting state in a short desktop usage video. Include a visible cursor, click highlights, and readable keyboard annotations using the project's PR video recording instructions.

For supplementary simulator captures, use these commands. Stop `recordVideo` with Control-C so it finalizes the video.

```sh
xcrun simctl io "$THREDS_SIMULATOR_UDID" screenshot "$THREDS_EVIDENCE_DIR/library.png"
xcrun simctl io "$THREDS_SIMULATOR_UDID" recordVideo "$THREDS_EVIDENCE_DIR/workflow.mp4"
```

`simctl` manages lifecycle and capture. Use computer use against the visible Device Hub or Simulator window for taps and keyboard interaction. Inspect fresh UI state before acting. Raw simulator video alone does not provide the required desktop cursor and keyboard annotations.

Upload review media as GitHub attachments with `gh pr create --attach` or `gh pr edit --attach`, then verify playback in the PR. Do not commit media. If the installed CLI cannot attach files or playback cannot be verified, report the blocker and provide the local artifact path.
