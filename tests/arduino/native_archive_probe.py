"""Exercise the identified CLI's sketch reader on owned files; no board, build or download."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import uuid
import zipfile

ROOT = Path(__file__).resolve().parents[2]
CLI_SHA256 = "1017de89179c3167e6b8a38ca6cc4091fa69a3cf97aafa7810f01423003f5571"


def main():
    executable = ROOT / "target/arduino-probe/runtime/arduino-cli.exe"
    if not executable.is_file() or hashlib.sha256(executable.read_bytes()).hexdigest() != CLI_SHA256:
        raise SystemExit("Prepare the identified Arduino CLI 1.5.1 fixture first; nothing was launched.")
    root = ROOT / "target" / ("arduino-native-archive-" + str(uuid.uuid4()))
    root.mkdir()
    assert root.resolve().is_relative_to((ROOT / "target").resolve())
    for folder in ["profile", "data", "downloads", "temp", "sketchbook"]:
        (root / folder).mkdir()
    sketch = root / "sketchbook/OwnedSketch"
    sketch.mkdir()
    files = {
        "OwnedSketch.ino": b'#include "Helper.h"\nvoid setup() {}\nvoid loop() { delay(DELAY); }\n',
        "Helper.h": b"#define DELAY 500\r\n",
        "Other.cpp": b"// owned extra translation unit\n",
        "src/Other.h": b"// nested header\n",
        "data/reference.txt": b"owned reference data\n",
        ".theia/settings.json": b'{"files.autoSave":"off"}\n',
        ".git/config": b"[owned]\n",
        "build/not-a-dependency.txt": b"owned build output\n",
    }
    for name, body in files.items():
        path = sketch / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(body)
    config = root / "arduino-cli.yaml"
    config.write_text("directories:\n" + "".join(
        "  " + key + ": " + json.dumps(str(root / folder)) + "\n"
        for key, folder in [("data", "data"), ("downloads", "downloads"), ("user", "sketchbook")]
    ), encoding="utf-8")
    env = {key: os.environ[key] for key in ["SystemRoot", "WINDIR", "COMSPEC"] if key in os.environ}
    env.update({key: str(root / "profile") for key in ["USERPROFILE", "APPDATA", "LOCALAPPDATA"]})
    env.update({"TEMP": str(root / "temp"), "TMP": str(root / "temp")})
    version = subprocess.run([str(executable), "version", "--json"], cwd=root, env=env,
                             capture_output=True, timeout=10)
    assert version.returncode == 0
    identity = json.loads(version.stdout)
    assert identity["Application"] == "arduino-cli" and identity["VersionString"] == "1.5.1"
    archive = root / "NativeReadback.zip"
    run = subprocess.run([str(executable), "sketch", "archive", str(sketch), str(archive),
                          "--include-build-dir", "--config-file", str(config), "--json"],
                         cwd=root, env=env, capture_output=True, timeout=45)
    (root / "stdout.private").write_bytes(run.stdout)
    (root / "stderr.private").write_bytes(run.stderr)
    assert run.returncode == 0
    seen = set()
    with zipfile.ZipFile(archive) as native:
        for entry in native.infolist():
            name = entry.filename.replace("\\", "/")
            assert name.startswith("OwnedSketch/") and ".." not in name.split("/")
            relative = name.removeprefix("OwnedSketch/")
            assert not entry.is_dir() and relative in files and relative not in seen
            assert entry.file_size == len(files[relative]) and native.read(entry) == files[relative]
            seen.add(relative)
    assert seen == set(files)
    assert all((sketch / name).read_bytes() == body for name, body in files.items())
    proof = {"version": 1, "pass": True, "runtime_version": "1.5.1", "executable_sha256": CLI_SHA256,
             "include_build_dir": True, "exact_file_count": len(seen), "originals_preserved": True,
             "compilation_checked": False, "dependency_resolution_checked": False,
             "native_gui_checked": False, "model_download_requested": False}
    (root / "acceptance.json").write_text(json.dumps(proof, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(proof))


if __name__ == "__main__":
    main()
