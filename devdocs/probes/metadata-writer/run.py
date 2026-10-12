#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["ds-store==1.3.1"]
# ///
"""Check two private exported flush functions, using disposable files only."""
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import time

def main():
    source = Path(__file__).resolve().parent
    spec = importlib.util.spec_from_file_location(
        "appkit_harness", source.parent / "appkit-trash" / "run.py"
    )
    harness = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(harness)
    output = source.parents[2] / "target" / f"metadata-writer-probe-{time.time_ns()}"
    output.mkdir()
    shutil.copy2(source / "probe.m", output / "probe.m")
    subprocess.run(
        ["xcrun", "clang", "-g", "-O0", "-fobjc-arc", "-framework", "Foundation",
         str(output / "probe.m"), "-o", str(output / "probe")],
        check=True,
    )
    print(output, flush=True)
    modes = ("baseline", "FlushCatalogs", "NodeContextFlush", "linger")
    for repeat in range(3):
        for index, mode in enumerate(modes):
            result = harness.run_case(
                output / "probe", mode, output, repeat * len(modes) + index, post_exit=3
            )
            with (output / "results.jsonl").open("a") as stream:
                stream.write(json.dumps(result) + "\n")
            fields = ("mode", "exit_code", "exit_seconds", "complete", "errors")
            print(json.dumps({key: result[key] for key in fields}), flush=True)


if __name__ == "__main__":
    main()
