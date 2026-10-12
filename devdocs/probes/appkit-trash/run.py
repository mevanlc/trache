#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["ds-store==1.3.1"]
# ///
"""Serial disposable-file probes; all .DS_Store access is read-only."""
import argparse
from contextlib import suppress
import json
import os
from pathlib import Path
import plistlib
import shutil
import signal
import stat
import subprocess
import time
import uuid

from ds_store import DSStore

SOURCE = Path(__file__).resolve().parent
REPO = SOURCE.parents[2]
MODES = ["foundation", "foundation-linger", "appkit-link", "init-exit", "init-terminate",
         "lifecycle-exit", "lifecycle-terminate", "lifecycle-linger", "lifecycle-ready",
         "lifecycle-turn", "lifecycle-defer", "lifecycle-services", "workspace-recycle",
         "workspace-serial", "lifecycle-accessory", "lifecycle-window",
         "gui-recycle", "gui-filemanager", "gui-linger"]


def executable_name(mode):
    if mode.startswith("foundation"):
        return "foundation"
    return "gui" if mode.startswith("gui-") else "appkit"


def metadata(moves):
    result = []
    for move in moves:
        destination = Path(move["destination"])
        source = Path(move["source"])
        try:
            with DSStore.open(str(destination.parent / ".DS_Store"), "r") as store:
                values = {e.code.decode(): e.value for e in store.find(destination.name)
                          if e.code in (b"ptbN", b"ptbL")}
            result.append({"values": values, "complete":
                           values.get("ptbN") == source.name and
                           values.get("ptbL", "").strip("/") == str(source.parent).strip("/")})
        except Exception as error:
            result.append({"complete": False, "read_error": repr(error)})
    return result


def run_case(binary, mode, output, ordinal, post_exit, linger=0, via_open=False):
    token = uuid.uuid4().hex
    case_dir = output / f"case-{ordinal:03}-{mode}"
    case_dir.mkdir()
    originals = [case_dir / f"trache-appkit-{token}-{i}.txt" for i in range(3)]
    owned = {}
    for path in originals:
        with path.open("x") as stream:
            stream.write(token)
        st = path.lstat()
        owned[path.name] = (st.st_dev, st.st_ino)
    args = [str(binary), str(linger) if mode == "foundation" else mode, *map(str, originals)]
    stdout_path = case_dir / "stdout.jsonl"
    stderr_path = case_dir / "stderr.txt"
    if via_open:
        args = ["/usr/bin/open", "-n", *([] if mode.startswith("gui-") else ["-g"]), "-W", "--stdout", str(stdout_path),
                "--stderr", str(stderr_path), str(binary.parents[2]), "--args", *args[1:]]
    events = []
    changes = []
    child = None
    moves = []
    errors = []
    start = time.monotonic()
    exit_at = None
    result = {"mode": mode, "linger": linger, "argv": args, "token": token}

    try:
        with stderr_path.open("w") as stderr, stdout_path.open("w") as stdout, stdout_path.open() as reader:
            child = subprocess.Popen(args, stdout=stdout if not via_open else subprocess.DEVNULL,
                                     stderr=stderr, text=True)
            previous = None
            while True:
                now = time.monotonic() - start
                while line := reader.readline():
                    try:
                        event = json.loads(line)
                    except ValueError:
                        errors.append({"stdout": line})
                        continue
                    event["observed_seconds"] = round(now, 6)
                    events.append(event)
                    if event["event"] == "move":
                        moves.append(event)
                if child.poll() is not None and exit_at is None:
                    exit_at = now
                    # Drain stdout on the next iteration before taking the exit snapshot.
                    continue
                if len(moves) == 3:
                    state = metadata(moves)
                    fingerprint = (child.poll() is None, state)
                    if fingerprint != previous:
                        changes.append({"seconds": round(now, 6), "alive": child.poll() is None,
                                        "metadata": state})
                        previous = fingerprint
                if exit_at is not None and now >= exit_at + post_exit:
                    break
                if now >= 20:
                    errors.append({"timeout": now})
                    if via_open:
                        for event in events:
                            if event["event"] == "start":
                                with suppress(ProcessLookupError):
                                    os.kill(event["pid"], signal.SIGKILL)
                    child.kill()
                    child.wait()
                    break
                time.sleep(0.05)
            result.update(exit_code=child.returncode, exit_seconds=round(exit_at or now, 6),
                          events=events, observations=changes, errors=errors,
                          final_metadata=metadata(moves))
            result["complete"] = [m["complete"] for m in result["final_metadata"]]
            (case_dir / "result.json").write_text(json.dumps(result, indent=2) + "\n")
            return result
    finally:
        if child is not None and child.poll() is None:
            if via_open:
                for event in events:
                    if event["event"] == "start":
                        with suppress(ProcessLookupError):
                            os.kill(event["pid"], signal.SIGKILL)
            child.kill()
            child.wait()
        # Only remove this run's exact inode/content. Never edit .DS_Store.
        candidates = set(originals) | {Path(m["destination"]) for m in moves}
        candidates |= {Path.home() / ".Trash" / p.name for p in originals}
        for path in candidates:
            try:
                st = path.lstat()
            except FileNotFoundError:
                continue
            if (not stat.S_ISREG(st.st_mode) or (st.st_dev, st.st_ino) != owned.get(path.name)
                    or path.read_text() != token):
                raise RuntimeError(f"Refusing to remove unexpected file: {path}")
            path.unlink()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--modes", nargs="+", choices=MODES, default=["foundation", "foundation-linger"])
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--post-exit", type=float, default=3)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--bundle", action="store_true", help="Wrap the executable in a .app (regular foreground app for GUI modes)")
    parser.add_argument("--via-open", action="store_true", help="Launch the .app through Launch Services")
    args = parser.parse_args()
    if args.repeats < 1 or not 0 <= args.post_exit < 10:
        parser.error("Use positive --repeats and 0 <= --post-exit < 10 seconds")
    if args.via_open and not args.bundle:
        parser.error("--via-open requires --bundle")
    if args.bundle and any(mode.startswith("foundation") for mode in args.modes):
        parser.error("Bundle probes require AppKit modes")
    if any(mode.startswith("gui-") for mode in args.modes) and not (args.bundle and args.via_open):
        parser.error("GUI modes require --bundle --via-open")
    output = args.output or REPO / "target" / f"appkit-trash-probe-{time.strftime('%Y%m%d-%H%M%S')}"
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    print(f"Results: {output}", flush=True)
    for source in SOURCE.glob("*.*"):
        if source.suffix in (".swift", ".py"):
            shutil.copy2(source, output / source.name)
    binaries = {}
    for name in {executable_name(mode) for mode in args.modes}:
        binary = output / name
        subprocess.run(["swiftc", "-g", "-o", str(binary), str(SOURCE / f"{name}.swift")], check=True)
        if args.bundle:
            bundle = output / ("TracheGUITrashProbe.app" if name == "gui" else "TracheAppKitProbe.app")
            executable = bundle / "Contents" / "MacOS" / "probe"
            executable.parent.mkdir(parents=True)
            shutil.copy2(binary, executable)
            (bundle / "Contents" / "Info.plist").write_bytes(plistlib.dumps({
                "CFBundleIdentifier": f"local.trache.appkit-probe.{uuid.uuid4().hex}",
                "CFBundleName": "Trache AppKit Probe", "CFBundleExecutable": "probe",
                "CFBundlePackageType": "APPL", "CFBundleVersion": "1",
                "NSPrincipalClass": "NSApplication", "LSUIElement": name != "gui",
            }))
            binary = executable
        binaries[name] = binary
    results = []
    with (output / "results.jsonl").open("x") as stream:
        for repeat in range(args.repeats):
            for mode in args.modes:
                name = executable_name(mode)
                result = run_case(binaries[name], "foundation" if name == "foundation" else mode,
                                  output, len(results), args.post_exit,
                                  linger=3 if mode == "foundation-linger" else 0,
                                  via_open=args.via_open)
                result["case"] = mode
                result["repeat"] = repeat
                results.append(result)
                stream.write(json.dumps(result) + "\n")
                stream.flush()
                print(f"{repeat + 1} {mode}: {result['complete']} "
                      f"exit={result['exit_seconds']:.3f}s code={result['exit_code']}", flush=True)
    (output / "environment.txt").write_text(subprocess.check_output(["sw_vers"], text=True) +
                                              subprocess.check_output(["swiftc", "--version"], text=True,
                                                                      stderr=subprocess.STDOUT))


if __name__ == "__main__":
    main()
