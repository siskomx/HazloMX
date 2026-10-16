"""Prepare or launch a fresh portable Agent fixture without changing the user's IDE profile."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import stat
import subprocess
import time
import uuid
import zipfile

ROOT = Path(__file__).resolve().parents[2]


def main(launch, hold_for_conversation=False):
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
    for name in ["temp", "data", "downloads", "profile/Documents/Arduino", "profile/Desktop", "profile/AppData/Roaming", "profile/AppData/Local"]:
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
        host = ROOT / "target/debug/examples/owned_agent_editor_host.exe"
        assert supervisor.is_file(), "Build the owned_arduino_editor_probe example before launching"
        assert host.is_file(), "Build the owned_agent_editor_host example before launching"
        env = os.environ.copy()
        env["HAZLOMEXA_TEST_AGENT_ROOT"] = str(root)
        if hold_for_conversation:
            env["HAZLOMEXA_TEST_AGENT_CONVERSATION"] = "1"
        with (root / "supervisor.log").open("wb") as log:
            process = subprocess.Popen([str(supervisor)], cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT,
                creationflags=subprocess.CREATE_NO_WINDOW)
        state.update({"pid":process.pid,"supervised":True,"launched":True})
    (ROOT / "target/current-arduino-native-ui.json").write_text(json.dumps(state,indent=2))
    print("Owned portable Arduino Agent fixture " + ("launched." if launch else "prepared."), flush=True)
    if launch:
        native = root / "native-api-result.json"
        host_result = None
        while process.poll() is None:
            try:
                ready = json.loads(native.read_text()).get("pass") is True
            except (FileNotFoundError, json.JSONDecodeError):
                ready = False
            if ready:
                host_env = dict(os.environ, USERPROFILE=str(profile), HAZLOMEXA_NATIVE_EDITOR_TEST_ROOT=str(root))
                with (root / "host-stdout.log").open("wb") as stdout, (root / "host-stderr.log").open("wb") as stderr:
                    try:
                        host_result = subprocess.run([str(host)], cwd=ROOT, env=host_env, stdout=stdout, stderr=stderr,
                            creationflags=subprocess.CREATE_NO_WINDOW, timeout=30).returncode
                    except subprocess.TimeoutExpired:
                        host_result = -1
                (root / "host-default-profile.json").write_text(json.dumps({
                    "passed":host_result == 0,"exit_code":host_result,"copied_registration":False,
                    "registration_override":False,"native_ui_approval":False,"real_provider":False}))
                print("Default-profile native host check " + ("passed." if host_result == 0 else "failed; private evidence retained."), flush=True)
                break
            time.sleep(0.1)
        code = process.wait()
        print("Owned native supervisor finished:", code)
        if code or host_result != 0:
            raise SystemExit(1)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--launch", action="store_true")
    parser.add_argument("--hold-for-conversation", action="store_true", help="Keep the supervised owned editor available for a bounded real-provider test")
    args = parser.parse_args()
    if args.hold_for_conversation and not args.launch:
        parser.error("--hold-for-conversation requires --launch")
    main(args.launch, args.hold_for_conversation)
