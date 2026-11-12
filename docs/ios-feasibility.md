# Start the GPUI iOS feasibility check

Use this procedure for [issue 2](https://github.com/johnnylarner/threds/issues/2).
The goal is to prove that GPUI supports the text editing workflow Threds needs on
iOS before product implementation starts. This setup records a candidate upstream
revision and a simulator recipe. It does not establish GPUI feasibility.

## Current status

Initial checks on 12 November 2026 found the following state.

- `xcode-select -p` returns `/Library/Developer/CommandLineTools`.
- `xcrun simctl list devices available` fails because `simctl` is unavailable.
- No `Xcode*.app` was found directly under `/Applications`.
- Xcode download has started in the App Store. Installation and first launch remain pending.
- The active Rust toolchain is `stable-aarch64-apple-darwin`.
- `rustup target list --installed` includes `aarch64-apple-ios` and
  `aarch64-apple-ios-sim` for that active toolchain. The upstream toolchain needs
  its own target installation below.

A later PR-update check encountered the Xcode licence prompt when invoking
Git. Complete Xcode first-launch setup and review its licence in person before
continuing. For repository operations while setup is pending, use
`DEVELOPER_DIR=/Library/Developer/CommandLineTools` without changing the global
developer directory.

The simulator build, app installation, app launch, and interaction checks have
not run. Full Xcode setup and an installed iOS Simulator runtime block those checks.

## Inspect the pinned upstream source

The candidate revision is
[`6ceb6fee8738274589b59139df27efc191ec2231`](https://github.com/zed-industries/zed/tree/6ceb6fee8738274589b59139df27efc191ec2231).
Keep this revision fixed for the first smoke test. Record any later revision
change with its reason and repeat the checks.

The following files were read at this revision through the GitHub API.

- [`rust-toolchain.toml`](https://github.com/zed-industries/zed/blob/6ceb6fee8738274589b59139df27efc191ec2231/rust-toolchain.toml)
  selects Rust `1.98.1` with the minimal profile and `rustfmt`, `clippy`,
  `rust-analyzer`, and `rust-src`.
- [`Cargo.toml`](https://github.com/zed-industries/zed/blob/6ceb6fee8738274589b59139df27efc191ec2231/Cargo.toml)
  uses edition 2024 and includes `gpui_ios`, `gpui_apple`, and `gpui_platform`
  as workspace members and path dependencies.
- [`crates/gpui_ios/Cargo.toml`](https://github.com/zed-industries/zed/blob/6ceb6fee8738274589b59139df27efc191ec2231/crates/gpui_ios/Cargo.toml)
  defines the `ios` example at `examples/ios.rs`. Its iOS dependencies include
  `gpui_apple` with `font-kit`.
- [`crates/gpui_platform/Cargo.toml`](https://github.com/zed-industries/zed/blob/6ceb6fee8738274589b59139df27efc191ec2231/crates/gpui_platform/Cargo.toml)
  selects `gpui_ios` for `target_os = "ios"`.
- [`crates/gpui_apple/Cargo.toml`](https://github.com/zed-industries/zed/blob/6ceb6fee8738274589b59139df27efc191ec2231/crates/gpui_apple/Cargo.toml)
  and [`build.rs`](https://github.com/zed-industries/zed/blob/6ceb6fee8738274589b59139df27efc191ec2231/crates/gpui_apple/build.rs)
  establish the Apple and Metal build dependencies. The build script invokes
  `xcrun` with the `iphonesimulator` SDK, `metal`, and `metallib` for this target.
- [`build-simulator.sh`](https://github.com/zed-industries/zed/blob/6ceb6fee8738274589b59139df27efc191ec2231/crates/gpui_ios/examples/ios/build-simulator.sh)
  builds `gpui_ios --example ios --target aarch64-apple-ios-sim` with an iOS 15.0
  deployment target. It packages and ad hoc signs `GPUIIosExample.app`.
  It forwards extra arguments to Cargo and uses Python 3 to find the executable.
- [`app/Info.plist`](https://github.com/zed-industries/zed/blob/6ceb6fee8738274589b59139df27efc191ec2231/crates/gpui_ios/examples/ios/app/Info.plist)
  names the executable `gpui_ios_example` and the bundle identifier
  `dev.zed.gpui-ios-example`. Its minimum iOS version is 15.0.
- [`examples/ios.rs`](https://github.com/zed-industries/zed/blob/6ceb6fee8738274589b59139df27efc191ec2231/crates/gpui_ios/examples/ios.rs)
  renders "GPUI on iOS" and a tap counter. It contains no multiline editor,
  HTTPS request, or Keychain probe.

## Prepare Xcode and the upstream checkout

Use an Apple Silicon Mac for this recipe. The upstream simulator script targets
`aarch64-apple-ios-sim`.

1. Finish the Xcode installation. Open Xcode and complete first-launch setup.
   Install an iOS Simulator runtime in Xcode Settings, Components.
2. Select full Xcode and check the tools. Adjust the application path if needed.

```sh
sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer
xcodebuild -version
xcrun --sdk iphonesimulator --show-sdk-path
xcrun --sdk iphonesimulator --find metal
xcrun --sdk iphonesimulator --find metallib
xcrun simctl list devices available
python3 --version
```

If either Metal tool is missing, install the Metal toolchain through Xcode's
Components settings before building.

3. Start in the Threds checkout. Choose a new sibling directory for upstream.
   Keep logs in the Threds evidence directory.

```sh
export THREDS_CHECKOUT="$(git rev-parse --show-toplevel)"
export IOS_EVIDENCE_DIR="$THREDS_CHECKOUT/.audit/mvp-plan/evidence"
mkdir -p "$IOS_EVIDENCE_DIR"
git clone --filter=blob:none --no-checkout https://github.com/zed-industries/zed.git ../zed-ios-feasibility
cd ../zed-ios-feasibility
git fetch origin 6ceb6fee8738274589b59139df27efc191ec2231
git checkout --detach 6ceb6fee8738274589b59139df27efc191ec2231
test "$(git rev-parse HEAD)" = 6ceb6fee8738274589b59139df27efc191ec2231
git rev-parse HEAD > "$IOS_EVIDENCE_DIR/upstream-revision.txt"
rustup toolchain install 1.98.1 --profile minimal --component rustfmt,clippy,rust-analyzer,rust-src
rustup target add --toolchain 1.98.1 aarch64-apple-ios-sim aarch64-apple-ios
rustc --version
rustup target list --installed --toolchain 1.98.1
```

Run subsequent commands from this upstream checkout in the same shell.
Its `rust-toolchain.toml` selects Rust `1.98.1`. A target installed for the
default stable toolchain alone does not satisfy this checkout.

## Build, install, and launch the smoke example

1. Record the environment and run the upstream build. The explicit target
   directory makes the bundle path independent of an inherited `CARGO_TARGET_DIR`.
   Run the build block in Bash so a failed build stays failed through `tee`.

```bash
export CARGO_TARGET_DIR="$PWD/target"
export IOS_APP_BUNDLE="$CARGO_TARGET_DIR/gpui-ios-example/GPUIIosExample.app"
xcodebuild -version > "$IOS_EVIDENCE_DIR/xcode-version.txt"
xcrun simctl list devices available > "$IOS_EVIDENCE_DIR/simulator-devices.txt"
bash -o pipefail -c 'bash crates/gpui_ios/examples/ios/build-simulator.sh --locked 2>&1 | tee "$IOS_EVIDENCE_DIR/simulator-build.log"'
```

2. Select an available iPhone UDID from `xcrun simctl list devices available`.
   Replace the placeholder below. If that device is already booted, skip the
   `simctl boot` command. Every device operation uses this explicit UDID.

```sh
export IOS_SIMULATOR_UDID='<available-iPhone-UDID>'
xcrun simctl boot "$IOS_SIMULATOR_UDID"
xcrun simctl bootstatus "$IOS_SIMULATOR_UDID" -b
open -a Simulator --args -CurrentDeviceUDID "$IOS_SIMULATOR_UDID"
printf '%s\n' "$IOS_SIMULATOR_UDID" > "$IOS_EVIDENCE_DIR/simulator-udid.txt"
xcrun simctl install "$IOS_SIMULATOR_UDID" "$IOS_APP_BUNDLE"
export IOS_BUNDLE_ID="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$IOS_APP_BUNDLE/Info.plist")"
test "$IOS_BUNDLE_ID" = dev.zed.gpui-ios-example
xcrun simctl launch "$IOS_SIMULATOR_UDID" "$IOS_BUNDLE_ID"
```

3. Confirm that "GPUI on iOS" appears and tapping the counter increments it.
   Capture the visible state and a short interaction recording.

```sh
xcrun simctl io "$IOS_SIMULATOR_UDID" screenshot "$IOS_EVIDENCE_DIR/upstream-smoke.png"
xcrun simctl io "$IOS_SIMULATOR_UDID" recordVideo "$IOS_EVIDENCE_DIR/upstream-smoke.mov"
```

Stop the recording with Control-C after demonstrating the counter. Keep the
build log, screenshot, video, revision, Xcode version, and simulator UDID together.
The eventual Threds app needs its own bundle identifier.

## Complete the feasibility checks

Leave each item unchecked until its behavior has been observed and recorded.
The tap counter establishes only that this smoke example builds and responds
to taps. A separate multiline editing probe and service probes remain to be built.

- [ ] Build and launch the pinned smoke example. Save the build log and a short
  recording of the counter interaction.
- [ ] Build a multiline editing probe. Use the software keyboard to type across
  multiple lines, insert Return, and backspace across line boundaries without
  lost or misplaced characters.
- [ ] Verify long-press selection, selection adjustment, copy, paste, and undo.
- [ ] Verify autocorrect and non-Latin composition, including replacement of
  marked text and the final committed text.
- [ ] Verify scroll-to-caret, keyboard dismissal and return, and safe-area layout
  with the keyboard visible in portrait and landscape.
- [ ] Make an HTTPS request from the app. Record the expected response, a network
  failure, and recovery. Keep credentials and tokens out of logs and recordings.
- [ ] Store, read, update, and delete a test secret through Keychain. Confirm that
  the app can read the stored secret after termination and relaunch.
- [ ] Sign and install the editing probe on a physical iPhone. Record the device,
  iOS version, signing setup, and the same editing checks. The simulator's ad hoc
  signature and `simctl` commands do not supply physical-device installation.
- [ ] Record the tested upstream revision, exact failures, and the feasibility
  decision in issue 2. Close the issue only after its acceptance criteria pass.

If editing loses or misplaces characters, stop product implementation and record
the exact sequence and missing platform behavior. Decide whether a bounded GPUI
fix is practical or whether to use a native iOS editor with a Rust core before
proceeding. Do not treat a successful smoke launch as that decision.
