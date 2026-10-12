# Swift probes for delayed Finder Put Back metadata

On macOS 27.0.1 (26A434), **participating in AppKit's launch and termination
lifecycle did not flush pending Put Back metadata before process exit**. This
also held for an actual `.app` launched through Launch Services and for
`NSWorkspace.recycle` after its completion handler reported success.
A follow-up with a regular, active GUI app and a visible window also reproduced
the failure in every rapid-exit trial.

These are standalone experiments. They do not change trache's implementation.

## Reproduce

From the repository root, with `swiftc` and `uv` available:

```sh
# Minimal Foundation-only reproduction and a three-second lifetime control.
uv run devdocs/probes/appkit-trash/run.py

# Linking, initialization, complete lifecycle, and lifetime control.
uv run devdocs/probes/appkit-trash/run.py --modes \
  appkit-link init-exit init-terminate \
  lifecycle-exit lifecycle-terminate lifecycle-linger

# Additional lifecycle and workspace paths.
uv run devdocs/probes/appkit-trash/run.py --modes \
  lifecycle-ready lifecycle-turn lifecycle-defer lifecycle-services \
  workspace-recycle workspace-serial

# A real .app, launched by Launch Services without taking focus.
uv run devdocs/probes/appkit-trash/run.py --bundle --via-open --modes \
  lifecycle-terminate lifecycle-accessory lifecycle-window

# A regular foreground GUI app with a visible window (briefly takes focus).
uv run devdocs/probes/appkit-trash/run.py --bundle --via-open --modes \
  gui-recycle gui-filemanager gui-linger
```

Each command compiles the probes and runs three repetitions serially. Results,
executables, source snapshots, and any generated `.app` go under a new directory
in `target/`. Use `--output PATH` for a specific new output directory.

[foundation.swift](foundation.swift) uses only Foundation: it trashes the supplied
paths with `FileManager.default.trashItem`, logs the resulting URLs, optionally
sleeps, and returns from top-level code. [appkit.swift](appkit.swift) adds the
AppKit variants. Both can be compiled separately with `swiftc -o OUTPUT SOURCE`;
their arguments are respectively `LINGER_SECONDS FILE...` and `MODE FILE...`.
Only pass disposable files when invoking them directly.

[run.py](run.py) creates three uniquely named files per trial, checks the actual
resulting Trash directories' `.DS_Store` records every approximately 50 ms, and
continues observing for three seconds after process exit. A complete record
requires both `ptbN` (the original filename) and `ptbL` (the original parent).
The observer reopens stores read-only; it never edits them. Cleanup removes
only the generated payloads, after checking inode, device, and content.

## Results

Tested with Apple Swift 6.4 on arm64, on the local APFS home volume. The initial
matrix had 54 trials, all with successful moves and no harness errors:

| Probe | Trials | Complete Put Back records after exit |
| --- | ---: | --- |
| Foundation, immediate exit | 4 | First file only in all trials |
| AppKit linked, no `NSApplication` | 3 | First file only in all trials |
| Initialize `NSApplication`, ordinary exit | 3 | First file only in all trials |
| Initialize `NSApplication`, call `terminate` | 3 | First file only in all trials |
| Full launch lifecycle, ordinary exit | 3 | First file only in all trials |
| Full launch lifecycle, call `terminate` | 4 | First file only in all trials |
| Wait 250 ms after launch before moving files | 3 | First file only in all trials |
| Allow another 100 ms of event-loop activity after moves | 3 | First file only in all trials |
| `terminateLater`, then reply after 50 ms | 3 | First file only in all trials |
| Initialize workspace/document controller; update windows/save empty document list | 3 | First file only twice; none once |
| `NSWorkspace.recycle`, one batch of three files | 3 | First file only in all trials |
| `NSWorkspace.recycle`, three sequential completion handlers | 3 | First file only in all trials |
| Launch Services `.app`, prohibited activation policy | 3 | First file only in all trials |
| Launch Services `.app`, accessory activation policy | 3 | First file only twice; none once |
| Accessory `.app`, create/update/close a window without showing it | 3 | First file only in all trials |
| Foundation, sleep three seconds after moves | 4 | All three in all trials |
| Full AppKit lifecycle, wait three seconds after moves | 3 | All three twice; first and third once |

**None of the 47 quick-exit trials recovered either queued file's complete
metadata.** Waiting in the live process produced all three records in six of
seven controls. In the seventh, the second file had `ptbL` but lacked `ptbN`.
The original controls first observed complete metadata at approximately
2.05–2.10 seconds from process launch, while the probe was still alive.

The AppKit logs verify `applicationWillFinishLaunching`,
`applicationDidFinishLaunching`, a running main-thread event loop during the
work, `applicationShouldTerminate`, and `applicationWillTerminate`. The deferred
termination variant additionally verifies its reply callback. Bundle trials
verify the bundle path and actual activation policy (2 = prohibited,
1 = accessory). Window trials verify that the window was never visible.

Quick CLI variants exited within 54–551 ms; Launch Services bundle trials
completed within 158–325 ms.
They therefore did not accidentally stay alive long enough for the known
two-second timer. The observer found no later recovery during its three-second
post-exit observation period.

The initial six-trial baseline and eighteen-trial lifecycle matrix used a pipe
for JSON events. Later matrices used a log file to support Launch Services;
the final three trials rechecked Foundation, its lifetime control, and complete
AppKit termination with that final harness. Both transports reproduced the same
behavior. Only the earlier lifecycle matrix lacks activation-policy logging.

Raw local evidence is in these repository-relative directories:

- `target/appkit-trash-baseline/`
- `target/appkit-trash-lifecycle2/`
- `target/appkit-trash-variants/`
- `target/appkit-trash-bundle/`
- `target/appkit-trash-final-check/`

Each has `results.jsonl` and per-trial `result.json` files. All 162 disposable
payload files were removed after observation. No production code was modified.

## Visible GUI follow-up

[gui.swift](gui.swift) is a separate, small GUI app with a titled window, drawn
content, and a standard Quit menu. Its `.app` has `LSUIElement = false` and
regular activation policy, and Launch Services opens it in the foreground.
Before touching files, it waits until the application is active and its window
is visible, key, unoccluded, and has executed its drawing code. These conditions
are logged and were verified in all nine trials.

It then recycles the three generated files and calls `NSApplication.terminate`
immediately after completion. The `gui-recycle` variant uses
`NSWorkspace.recycle`; `gui-filemanager` uses `FileManager.default.trashItem`.
`gui-linger` uses `NSWorkspace.recycle` and waits three seconds after completion
as a lifetime control.

| Visible GUI probe | Trials | Complete Put Back records after exit |
| --- | ---: | --- |
| `NSWorkspace.recycle`, immediate quit | 3 | First file only in all trials |
| `FileManager`, immediate quit | 3 | First file only in all trials |
| `NSWorkspace.recycle`, three-second wait | 3 | First file only twice; all three once |

All six rapid-quit trials reproduced the failure. Launch-to-exit time as observed
by the harness was 269–434 ms, including Launch Services. The moves took 14–19 ms,
and the app reached `applicationWillTerminate` 1.0–1.7 ms after completing them.
Both normal termination callbacks ran. There was no recovery during three
seconds of post-exit observation.

The GUI lifetime controls were less reliable than the earlier Foundation sleep
controls. This difference is observed but not explained by this experiment;
waiting three seconds is not a completion guarantee.

Raw results, source snapshots, and `TracheGUITrashProbe.app` are in
`target/appkit-trash-visible-gui/`. The app expects disposable paths supplied by
the runner; use the command above to rerun it. All 27 payloads were cleaned up,
all nine runs completed successfully, and the observer only read `.DS_Store`.

## Interpretation and limits

The tested AppKit paths do not supply the hypothesized shutdown flush. This
does not establish that no private hook exists elsewhere or under other
conditions. In particular, the experiment does not test Finder's own code,
other macOS releases, or external volumes. The oracle
checks restoration metadata, not a manual Finder Put Back action.

`NSWorkspace.recycle` completing successfully is insufficient to establish that
this metadata has reached disk. Keeping the originating process alive remains
the only successful intervention in this matrix, and is still best effort.

The lifecycle follows Apple's documented
[NSApplication setup](https://developer.apple.com/documentation/appkit/nsapplication),
[termination sequence](https://developer.apple.com/documentation/appkit/nsapplication/terminate(_:)),
and [deferred termination reply](https://developer.apple.com/documentation/appkit/nsapplication/reply(toapplicationshouldterminate:)).
The alternative trash API is
[NSWorkspace.recycle](https://developer.apple.com/documentation/appkit/nsworkspace/recycle(_:completionhandler:)).

The subsequent [private-writer analysis](../metadata-writer/README.md) traces
adjacent flush and lifecycle functions and tests the two exported candidates.
