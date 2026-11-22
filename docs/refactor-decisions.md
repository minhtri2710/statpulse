# Structural decisions

Baseline: the working tree on `main` at `2a2b3d6`, **including** the uncommitted
safety and performance work. Locations below are from that baseline. Both
optional cleanups were selected. These are decisions for this refactor, not new
product requirements or changes to the domain ADRs.

## Put a change where its owner can see it

1. **Define navigation once; keep screen-specific interactions with the screen.**
   - Hidden: `App.tsx:96-110,664,1279,1821` and `Header.tsx:23-172` separately
     encode tab identity, order, shortcuts, icons and titles. Settings is
     intentionally docked separately; only the first nine tabs have digit shortcuts.
   - Needless: `App.tsx:1869-4218` ties editing a network drill-down, cache
     confirmation, process list or audio control to unrelated telemetry and update
     lifecycles. Keep cross-screen state above the tab switch (including state that
     survives navigation); put each view and its own actions at its feature boundary.
     Do not replace the component with an untyped context or a whole-app prop bag.
   - Hidden: `App.tsx:769-775,828-836,1041-1053` guesses when to refresh after
     process actions; the stack action silently counts failures as successes.
     Let the mutation invalidate native snapshots and request a refresh on completion.
   - Hidden: the recording buffer, sample acceptance, long-history persistence and
     chart selection are shared across the initial fetch and telemetry listener
     (`App.tsx:152-204,370-428,847-889,1422,1539`). Keep one ingestion path.
     Update prepare/discard ordering is a different lifetime; move it as a unit.
2. **Let domain data, not an individual rendering, own shared display values.**
   - Hidden: `FanRuleManager.tsx:25-45`, native tray labels and
     `thermal_policy/contract.rs:60-80` carry the same preset curves. The Rust
     evaluator is authoritative. The webview may draw a curve, but should not
     invent its breakpoints. Custom rule editing and persisted v1 settings stay separate.
   - Hidden: policy modes, look names, conversion to Fahrenheit and fan percent
     recur across `Header`, `DesignStudioModal`, `App`, `FanRuleManager`, gauges
     and cards. Share values where meanings match; keep precision, placeholder,
     palette, curated order, preview data and alert bands local to presentation.
3. **Apply each committed preference change through one runtime path.**
   - Hidden: `lib.rs:691-728,811-870,1999-2092` repeat cadence notification,
     island reset/peek, geometry, tray refresh, menu checkmarks and UI notification.
     The tray menu also focuses the chosen layout even if it is already selected.
   - Needless: `sync_tray_menu_prefs` publishes the layout watch as a hidden side
     effect of setting checkmarks. Rendering should not own scheduling. The tray
     style builder and handler encode the same thirteen IDs (`1665,2050`).
   - Surplus: every call of `trigger_island_peek` passes an ignored boolean.
     Geometry and cursor tracking belong to the native window, not settings.
4. **Give Keep Awake one observation of the conditions its rules need.**
   - Hidden: command, tray toggle and periodic evaluation in `lib.rs:1124,1923,2162`
     independently gather AC status, count ports 1024-10000 and detect AI activity.
     Only enabled rules should cause probes. Counting ports does not need enrichment.
   - Needless: a tray toggle reads the full OS assertion list just to know if the
     manual hold is indefinite. OS queries must not block the menu event thread.
     Live rules are session state; persisting them would change the product.
5. **Share the hot system measurements, not the expensive collection scopes.**
   - Hidden: `system_extras.rs:578-650,657-732,1247-1289` repeat memory, process
     and network counter interpretation. One owner must advance each pair of
     counters. A pill may reuse a full snapshot but must never satisfy a full
     request or launch full network diagnostics. Put device, network, audio,
     developer-tool and storage collectors with the data and caches they own.
   - Needless: the microphone detector parses an assertion list and discards it
     (`1578`). CLI metrics changes a process-wide SWR switch (`mcp.rs:89,137`),
     also affecting a long-lived MCP process. Disable unrelated background work
     for the one-shot CLI, not for other requests.
   - Keep the rich and cheap port views distinct: cached enriched metadata and
     fresh raw listeners have different consumers and freshness requirements.
     Process display labels and an allowlist used for termination are *not*
     interchangeable. Do not broaden authority to terminate processes.
6. **Keep system adapters on the outside of policy, telemetry and update state.**
   - Hidden: the power callback imports the policy type while policy imports the
     callback's mutable sleep flag (`power_management.rs`, `thermal_policy/state.rs`).
     Share the actual sleep signal, not a test that writes a global. Check it at
     the decision lock, not only at the UI/IPC boundary.
   - Hidden: helper readiness is two identically shaped enums plus an exhaustive
     conversion (`fan_actuation/client.rs:9`, `hardware_telemetry/contract.rs:73`,
     `lib.rs:616-657`). Distinguish artifact presence from *confirmed readiness*;
     use one readiness type. Keep update handoff distinct from normal writes.
   - Hidden: update launch detection, two-phase startup acknowledgement and its
     tests live in `lib.rs:732-762,1360,2418`, separate from the updater's socket
     protocol. Keep them with activation; leave Tauri entry points as adapters.
   - Hidden: `services/updater.ts:23` hard-codes the package version; keep no
     second release number in the frontend. The advertised version must be
     installable under the same stable-version rules. Preparation still verifies
     the official digest independently; release notes never authorize an install.
7. **Make tooling produce the artifact its consumer actually reads.**
   - Hidden: `scripts/generate-types.sh` generates into `src-tauri/bindings` and
     copies to `src/generated`; ordinary Rust tests write only the intermediate
     tree. Use the same export directory for a plain Cargo test and generation.
   - Hidden: `stage-fan-actuation-daemon.sh` infers the requested architecture
     from stale directories. Current Tauri target metadata, not build history,
     determines the helper to build.
   - The app ships as `com.statpulse.app`, with `statpulse` as its executable.
     Do not accept another bundle ID to make an in-place update work. Profile
     export/import transfers preferences and thermal rules without moving the
     old data directory. The Homebrew cask for 1.3.1 links the new executable;
     its quit and zap stanzas target only the new identity. Publish it to the
     tap only when its digest matches the official Universal DMG.
   - Surplus: unused scaffold CSS/graphics, two unreachable thermal cards, and
     frontend-only dependencies with no importers. Keep the favicon referenced by
     `index.html`. Do not confuse JavaScript's unused positioner package with the
     native positioner plugin.
8. **Remove proof of implementation details, not proof of safety (last).**
   - `fan_actuation/socket.rs:285-322` matches every possible `Result`. Keep its
     real idle/slow-peer deadlines and the expected restore on fixture hardware.
   - `console_user.rs:21` can pass without exercising rejection; use deterministic
     ownership input. `lib.rs:2443` advances time before starting the waiter;
     `mcp.rs:438` pins tool order; test waiting and advertised tools, respectively.
   - `scripts/verify-fan-actuation-bundle.sh` greps script prose/shell spelling.
     Inspect bundle files, metadata and executable behavior; do not represent
     text matching or ad-hoc signatures as a successful privileged installation.
   - `cache_cost_fixture.rs:63` **does** call the descriptor-based sizing seam.
     Keep it. Remove the unused marker argument, not the benchmark. Retain Rust
     contract-generation tests, C firmware scenarios and the Node history suite.

## Exceptions that have a requirement

| Family | Requirement that survives |
| --- | --- |
| Telemetry age, future timestamps, direct fan range, hysteresis, per-write clock, lease expiry/retry, sleep and Dust Blast token | Never prolong manual ownership on guessed data; a late completion cannot cancel a newer owner. The daemon and evaluator enforce different trust boundaries. |
| SMC delays, repeated reads, authorization and timed Unix-socket I/O | Firmware may reclaim a fan and an unresponsive client must not hold up System Auto. Hardware acknowledgement, not elapsed time alone, is success. |
| Updater length/digest/identity checks, handshake timeout, rollback; login confirmation and recovery journal | These protect different failure modes and cannot be replaced with optimistic success or a generic retry. |
| Descriptor pinning, no-follow, filesystem boundary, ownership/ACL and inode checks | A path or directory name does not establish that the object is safe to clean. Keep shared-cache versus project-root removal semantics. |
| SWR timeouts (3-180 s), 1.5 s snapshot reuse, 2.5 s ports, fast/cached-only pill probes | Service costs and acceptable staleness differ. Do not make the pill pay for full collection or re-use a pill as a full snapshot. |
| UI timers, event disposal, history clock reset, per-window point budgets | Distinguish UX timing and OS events from guessed completion; never discard a late recording or overwrite expanded data with a pill reply. |
| macOS parser sentinels, optional tools, permissions, 0 RPM, unavailable sensors, preview-only demo data | No readings is different from zero. Demo values are appropriate only in a preview. Platform-specific parsing and local tool discovery are adapters, not generic policy. |
| Two ignored host diagnostics and one synthetic cache timing experiment | These are deliberate opt-ins, not disabled behavioral regressions. Do not enable host probes in the ordinary test lane. |

Evidence: `2a2b3d6` changed both entry points together; `775dbbb` and `7cab9eb`
also pair `lib.rs` and `App.tsx` for update work. File size alone is not a
reason to split the evaluator, updater transaction or cleanup traversal.

## Review and proof decisions

- **Tray actions commit intent before slow observation.** In the baseline
  `lib.rs:1923-1963`, two toggles can read the same Keep Awake rules, wait for
  different probes, then overwrite each other. Assign the requested rules under
  the manager lock; evaluate only if those rules still apply when probes finish.
  A newly committed set of preferences likewise owns publication: the last
  caller to finish drawing must not reinstate an older layout or cadence.
- **Keep user actions and background events distinct.** The background updater
  shows its dialog after preparation; an explicit open shows progress immediately.
  A failed process action must remain visible across a successful inventory
  refresh. Package power for process estimates and battery flow for the pill are
  different readings, not duplicated preferences.
- **Trust the build target, not the build tree.** With stale universal output
  present, the old hook incorrectly staged both architectures for each of the two
  single-architecture requests. The new hook passes the architecture fixture
  for all three targets. [Tauri CLI 2.11.4 hook environment](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.11.4/crates/tauri-cli/src/interface/rust.rs#L291-L315).

| Baseline proof and reason it exists | Decision and remaining regression check |
| --- | --- |
| `console_user::active_console_uid_never_authorizes_root`: daemon startup relies on `/dev/console` ownership. It only asserts on the host's successful result. | Replace, do not drop, the authorization check: the same production decision must reject UID 0 and admit a non-root console UID. Socket tests check a different boundary. Run `cargo test --lib console_user`. |
| `socket::{idle_peer_releases_daemon_before_lease_expires, slow_peer_cannot_extend_request_deadline}`: bounded service of idle and trickle clients. | Keep restoration and bounded progress; remove only the matches accepting both `Ok` and `Err`. Disconnected peers may make writing an error fail. Run `cargo test --lib fan_actuation::socket`. |
| `installer::{resource_manifest_rejects_missing_and_symlinked_helper, installed_status_requires_both_regular_executable_artifacts}`: resource validation and installed-state reporting. | Test the symlink starting with a working fixture so missing execute bits cannot account for the refusal. Keep both absence and incomplete-install checks; no unprivileged test claims actual installation. Run `cargo test --lib fan_actuation::installer`. |
| `migration::missing_legacy_path_is_already_migrated`: startup with nothing to migrate. | Preserve the contract; use an absent path in a private tempdir, not a file another test or user might own. Run `cargo test --lib fan_actuation::migration`. |
| `mcp_handles_initialize_notification_and_tools_list`: pinned tool discovery on the stdio interface. | Keep exact membership, notification behavior and request ID. List order is not a protocol requirement. Run `cargo test --lib mcp::tests`. |
| `cache_cost_fixture::cache_size_wall_time`: opt-in synthetic sizing comparison against individual `du -skx` calls. | Keep benchmark and hard-link accounting, direct it through production `inspect` with zero threshold. Now the duplicate `#[cfg(test)] size_kib` traversal entry point and unused marker argument can go. Run the synthetic benchmark separately from host diagnostics. |
| `system_extras::tests`: collector output parsers grouped unrelated commands in five tests; a full host timing check also asserted nonnegative memory. | Move fixture examples with each parser, keep each independent assertion, drop only the nonnegative check. `cargo test --lib system_extras` exercises supported fixture adapters; opt-in host diagnostics stay out of CI. |
| `scripts/verify-fan-actuation-bundle.sh`: four string searches intended to verify an installer. | Remove textual spellings, not checks of the shipped artifact. Verify shell syntax, resource files, plist program and signature. Privileged installation is not a test lane. |

The UID, socket, installer and MCP cases predate this pass; the cache timing
fixture and paused-layout proof belong to the uncommitted baseline. No
hardware-facing parameter or compatibility path is removed merely to simplify a test.

## Review evidence

- **A stored observation is not a plotted point.** At the fastest supported
  cadence the closed two-minute interval has 121 observations. The chart can
  draw at most 120 and must keep both endpoints. Retention and extrema selection
  belong to history, not to individual views. Both Home and System offer the
  same four windows.
- **A Tauri async command is not necessarily a blocking worker.** The pinned
  macro calls synchronous command bodies inside a spawned future. IPC handlers
  doing OS queries, firmware I/O or profile writes therefore need the blocking
  pool. Geometry-only reads can use the committed layout watch without
  contending with a login-item query. The wait for confirmation and the policy
  checks still happen at their original decision points.
- **Compare against the uncommitted input, not just HEAD.** In the browser
  fixture all twelve views and the first nine shortcuts matched the captured
  baseline. Separate scenarios confirmed one command palette, partial
  stack-stop feedback surviving a refresh and a foreground update joining one
  pending preparation. Dismissing the dialog during preparation did not reopen
  it. An installation error does not open a browser without a user action;
  that behaviour already existed in the input.
- **Bound the performance claim.** On 128 synthetic cache roots, individual
  `du` calls took 174.48 ms median and checked native inspection took 18.52 ms
  (nine measured rounds after warm-up). Each root's size matched in every
  round. This checks the existing optimisation after moving its owners; it
  does not measure whole-app frame rate, battery use or a new refactor speedup.
