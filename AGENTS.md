# Project summary and structure

Threds is an open-source iOS writing app in early development. GitHub
issue #6 tracks the accepted MVP and issues #2 through #5 define its sequence.
Markdown is authoritative. The first offline source-editor slice is implemented;
physical-device feasibility and broader input validation remain pending. The browser recording skill applies only to
browser workflows; native iOS interaction follows the Simulator section below.

## Worktree policy

- For read-only tasks (investigating, reading, summarizing, or describing code),
  work on the current branch without creating a worktree.
- For tasks that require changing code or other files, use [`worktrunk`](https://worktrunk.dev):
  - `wt switch --create <branch-name> --base main`
- Never modify the primary checkout without explicit permission.

## Exposed coding policy

- Follow with strict detail to the existing style of the application. This includes:
  - Type driven design
  - Domain modelling and parse-not-validate
  - Consistent error handling
  - Service layer abstractions

### UI verification

- Run `npm run check`, `npm run build`, and the affected Playwright tests from
  `frontend`. Gallery tests use a development server. Production tests use the
  built application.
- Match the evidence in the PR description to the type of UI change:
  - Aesthetic-only changes, such as sizes, colours, and positions, require
    desktop screenshots of the affected views. No video is required.
  - Functional changes and new features, including anything that requires
    steps to test, require a short desktop usage video of the affected workflow.
    Show the starting state, relevant actions, and resulting state, with
    deliberate pauses so reviewers can follow. Screenshots are optional
    supplements. PRs with both aesthetic and functional changes require video.
  Capture evidence from the running application on the PR branch after
  completing the changes.
- For videos, use the [PR video recording skill](.agents/skills/record-pr-video/SKILL.md).
  Show a visible cursor, a brief highlight at each click location, and readable
  overlays for simulated keyboard inputs, including shortcuts and special keys.
  Show entered text only when it is safe to disclose. Keep annotations synchronized
  with actions and clear of the controls being demonstrated.
- Upload screenshots and videos as GitHub attachments with `gh pr create --attach` or
  `gh pr edit --attach`. Use a GitHub CLI version that supports this flag. Never
  commit PR screenshots or recordings to the repository. Verify that screenshots
  render and videos play in the PR description before marking the PR ready for
  review. If uploading or verification is unavailable, report the blocker and
  provide the local evidence artifact.

## Git history

- After completing and validating a task, commit its changes on the task's work
  branch, push the branch, and open a pull request against the target branch
  without waiting for a separate request. If the branch already has an open PR,
  update it instead of creating a duplicate.
- Before reporting completion, verify that the remote branch matches local
  `HEAD` and that the open PR targets the intended base and includes that commit.
  Include the PR URL and validation results in the final response.
- If pushing or opening the PR fails, report the blocker and the remaining step.
  Do not describe a local commit alone as completed delivery. Follow explicit
  user instructions to stop before pushing or opening a PR.
- Never commit directly on `main`.
- Keep Git history linear. Rebase feature branches onto the target branch and
  integrate changes with a fast-forward or squash; do not create merge commits.

| Rule | Enforcement |
| --- | --- |
| Deliver validated changes as a pushed branch with an open PR. | Agent completion check against the remote branch and `gh pr view`; include the verified PR URL in the final response. This is a workflow rule, not an automated gate. CI cannot observe work that was never pushed. |

## Product reference video

- Review the local reference at `.audit/pieces-mobile-review/source-recording.mp4` in the primary checkout. The original is `~/Downloads/ScreenRecording_11-12-2026 13-48-42_1.MP4`. Reference media is local and must never be uploaded to GitHub or committed. Worktrees may read it from the primary checkout without copying it into tracked paths.
- Read `.audit/pieces-mobile-review/Pieces-mobile-review.html` for the existing review. Verify claims against the recording rather than treating notes as product requirements.
- Use `ffprobe -v error -show_entries format=duration -show_entries stream=width,height -of json <video>` to inspect metadata. Extract one frame per second with `ffmpeg -i <video> -vf 'fps=1,scale=600:-1' <audit-dir>/frame-%03d.jpg`. Inspect frames with the image-viewing tool. Inspect denser frames around transitions when needed. Report the sampling method rather than claiming continuous playback.
- Timestamp observations and distinguish visible controls, completed actions, and inferences. Use synthetic text in new evidence. Do not quote the private writing or share-sheet contacts.
- Borrow the broad library layout and writing flow, not Pieces' branding, assets, or exact visual design. The accepted scope lives in GitHub issues.

## iOS Simulator workflow

- Use full Xcode, not Command Line Tools alone. Check `xcode-select -p`, `xcodebuild -version`, and `xcrun simctl list devices available` before claiming the simulator is available.
- Install a compatible iOS runtime through Xcode Settings, Components. Use `aarch64-apple-ios-sim` on Apple silicon and `aarch64-apple-ios` for physical devices. Do not confuse simulator success with physical-device validation.
- Select an explicit simulator UDID from the available-device list. Boot with `xcrun simctl boot <udid>` when necessary, then wait with `xcrun simctl bootstatus <udid> -b`. Avoid ambiguous `booted` targets when multiple devices are running.
- Follow [the development runbook](docs/development.md) for setup, logs, tests, persistence checks, and evidence. Build with `scripts/build-simulator.sh` and install/launch with `scripts/install-simulator.sh <udid>`. The bundle is `target/simulator/Threds.app`, identifier `dev.threds.app`. Xcode 27.2 uses Device Hub (`com.apple.dt.Devices`); older installations may use Simulator.
- Use computer use to interact with the Simulator application. Inspect current accessibility state and screenshots before clicking or typing. If needed, choose the desired device window in Simulator. Use coordinates only from a fresh screenshot. `simctl` handles lifecycle and capture, not arbitrary touch input.
- Capture with `xcrun simctl io <udid> screenshot <audit-dir>/screen.png` or `xcrun simctl io <udid> recordVideo <audit-dir>/workflow.mp4`. Stop recordings cleanly with SIGINT. Keep logs, screenshots, and videos under ignored `.audit/` paths.
- Verify keyboard entry, Return, backspace, selection, copy/paste, undo, composition, scrolling, keyboard avoidance, and safe areas. Exercise the software keyboard, not only injected text. Record device, runtime, source SHA, command, and observed result.
- If Xcode, a runtime, signing, or a physical device is unavailable, record the exact blocker in the active issue. Never mark feasibility complete based on compilation or a desktop rendering alone.

## GitHub issues as the work queue

- GitHub issues in `johnnylarner/threds` are the source of truth for scope, dependencies, acceptance criteria, progress, and blockers. Local plans and audit notes are supporting evidence.
- Before starting, read the selected issue and its comments. Confirm prerequisite issues are complete. If no issue is named, choose the first unblocked MVP issue in dependency order. Do not infer priority from issue number alone.
- Before implementation, post a short start comment with the work branch, planned checks, and known blockers. Update the issue when scope changes, a milestone is verified, or a blocker appears. Clearly separate completed evidence from planned work.
- Keep issue bodies concrete and checkable. Link dependencies and follow-up issues. User instructions override stale issue text; reconcile the issue to the user's decision before continuing.
- Close an issue only when every acceptance criterion has evidence. A partial local scaffold or blocked simulator does not complete an issue. Link commits and PRs when authorised, and retain local evidence paths when media cannot be published.
- Current session instruction: update the existing PR when requested. Creating and updating issues is authorised. Do not create additional PRs, close issues, or merge as incidental cleanup.

## Native project checks

- Run `cargo fmt --all -- --check`, `cargo test --locked -p threds-core`, and `scripts/build-simulator.sh`. Check the packaged plist and signature as documented in [the runbook](docs/development.md#test-build-and-install). This repository has no frontend, so the npm and Playwright commands above do not apply.
- Core tests exercise document and filesystem behavior on the host. Check software-keyboard and navigation behavior in the running iOS app separately. Record unobserved interactions as pending. Simulator success does not establish physical-device behavior.
