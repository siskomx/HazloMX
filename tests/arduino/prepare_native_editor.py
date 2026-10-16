"""Prepare or launch a fresh portable Agent fixture without changing the user's IDE profile."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import stat
import subprocess
import uuid
import zipfile

ROOT = Path(__file__).resolve().parents[2]


def main(launch):
    archive = ROOT / "target/arduino-agent-probe/arduino-agent-windows.zip"
    with archive.open("rb") as source:
        assert hashlib.file_digest(source, "sha256").hexdigest() == "a02ccf9fb97fa1e88b421ace4387bb5d912abe622152fcef2a9b89cae2603f58"
    root = ROOT / "target" / ("arduino-native-ui-" + str(uuid.uuid4()))
    root.mkdir()
    assert root.resolve().is_relative_to((ROOT / "target").resolve())
    runtime = root / "runtime"
    with zipfile.ZipFile(archive) as package:
        for item in package.infolist():
            relative = PurePosixPath(item.filename.replace("\\", "/"))
            assert not relative.is_absolute() and ".." not in relative.parts and ":" not in item.filename
            assert not stat.S_ISLNK(item.external_attr >> 16)
        package.extractall(runtime)
    executable = runtime / "Arduino Agent.exe"
    with executable.open("rb") as source:
        assert hashlib.file_digest(source,"sha256").hexdigest() == "d7c6e956ceecf5214867ff063e0ecf6bf0276946950ddb1f34946de31d2c5cd9"
    # Theia selects this portable path before constructing its Electron preferences store.
    (runtime / "resources/app/data/app-data").mkdir(parents=True)
    profile = root / "profile"
    config = profile / ".arduinoIDE"
    config.mkdir(parents=True)
    for name in ["connection", "temp", "data", "downloads", "profile/Documents/Arduino", "profile/Desktop", "profile/AppData/Roaming", "profile/AppData/Local"]:
        (root / name).mkdir(parents=True, exist_ok=True)
    sketch = root / "sketchbook/OwnedSketch/OwnedSketch.ino"
    sketch.parent.mkdir(parents=True)
    sketch.write_text("#include <Arduino.h>\nvoid setup() {}\nvoid loop() { delay(1000); }\n", newline="\n")
    (sketch.parent / ".theia").mkdir()
    settings = json.dumps({"files.autoSave":"off"})
    (config / "settings.json").write_text(settings)
    (sketch.parent / ".theia/settings.json").write_text(settings)
    (config / "arduino-cli.yaml").write_text(json.dumps({"directories":{
        "data":str(root / "data"),"downloads":str(root / "downloads"),"user":str(root / "sketchbook")}}))
    plugins = config / "plugins"
    shutil.copytree(ROOT / "tests/arduino/native-editor-probe", plugins / "native-probe")
    (root / "owned-native-probe.json").write_text(json.dumps({"owned":True}))
    state = {"root":str(root),"executable":str(executable),"launched":False}
    if launch:
        supervisor = ROOT / "target/debug/examples/owned_arduino_editor_probe.exe"
        assert supervisor.is_file(), "Build the owned_arduino_editor_probe example before launching"
        env = os.environ.copy()
        env["HAZLOMEXA_TEST_AGENT_ROOT"] = str(root)
        with (root / "supervisor.log").open("wb") as log:
            process = subprocess.Popen([str(supervisor)], cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT,
                creationflags=subprocess.CREATE_NO_WINDOW)
        state.update({"pid":process.pid,"supervised":True,"launched":True})
    (ROOT / "target/current-arduino-native-ui.json").write_text(json.dumps(state,indent=2))
    print("Owned portable Arduino Agent fixture " + ("launched." if launch else "prepared."), flush=True)
    if launch:
        code = process.wait()
        print("Owned native supervisor finished:", code)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--launch", action="store_true")
    main(parser.parse_args().launch)
