"""Opt-in native proof, entirely inside new disposable fixture directories.

Run with --run-native --executable <FreeCADCmd.exe>. No user document, native
window, provider, hardware, model download or existing profile is used.
The app's host-pinned dispatch/recovery integration remains a separate step.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import uuid

ROOT = Path(__file__).resolve().parents[2]
CAPTURE = '''import FreeCAD as App
import json
from pathlib import Path
from native_copy import capture, facts
root=Path.cwd()
(root/"version").mkdir()
doc=App.newDocument("OwnedCopy")
obj=doc.addObject("Part::Box","Box")
obj.Length=25.4;obj.Width=12;obj.Height=8
sphere=doc.addObject("Part::Sphere","Sphere")
sphere.Radius=3
sphere.Placement=App.Placement(App.Vector(60,4,2),App.Rotation(App.Vector(1,0,0),30))
for kind,name,parameters,offset in [
 ("Part::Cylinder","Cylinder",{"Radius":2,"Height":6},80),
 ("Part::Cone","Cone",{"Radius1":5,"Radius2":2,"Height":4},100),
 ("Part::Torus","Torus",{"Radius1":8,"Radius2":2},120),
]:
 part=doc.addObject(kind,name)
 for key,value in parameters.items():setattr(part,key,value)
 part.Placement=App.Placement(App.Vector(offset,4,2),App.Rotation(App.Vector(1,0,0),30))
doc.recompute()
doc.saveAs(str(root/"Original.FCStd"))
original=(root/"Original.FCStd").read_bytes()
obj.Width=22.25
obj.Label="Keep my part label"
doc.Label="Keep my manual document label"
doc.recompute()
state=lambda d: {"file":d.FileName,"label":d.Label,"facts":facts(d)}
before=state(doc)
assert all(abs(a-b)<1e-7 for a,b in zip(before["facts"]["objects"]["Sphere"]["measures_mm"][7:], [60,4,2]))
destination=root/"version"/(doc.Label+".FCStd")
saved=capture(doc,destination,state)
assert (root/"Original.FCStd").read_bytes()==original
assert state(doc)==before
assert obj.Width.Value==22.25
try:
 capture(doc,destination,state)
 raise AssertionError("Existing version was overwritten")
except ValueError as error:
 assert str(error)=="working_copy_destination_exists"
(root/"capture.json").write_text(json.dumps(saved),encoding="utf-8")
(root/"capture-proof.json").write_text(json.dumps({"original_unchanged":True,"live_state_unchanged":True,"existing_version_refused":True}),encoding="utf-8")
App.closeDocument(doc.Name)
'''
VERIFY = '''import FreeCAD as App
import json
from pathlib import Path
from native_copy import verify
root=Path.cwd()
captured=json.loads((root/"capture.json").read_text(encoding="utf-8"))
result=verify(root/"version"/captured["file"]["name"],captured)
assert not App.listDocuments()
assert len(result["embedded_resources"])>1
assert len(captured["expected"]["objects"])==5
(root/"verified.json").write_text(json.dumps(result),encoding="utf-8")
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-native", action="store_true", required=True)
    parser.add_argument("--executable", type=Path, required=True)
    args = parser.parse_args()
    executable = args.executable.resolve(strict=True)
    if not executable.is_file() or executable.name.lower() != "freecadcmd.exe":
        parser.error("Select the identified FreeCADCmd.exe runtime")
    parent = ROOT / "target"
    parent.mkdir(exist_ok=True)
    root = parent / ("native-copy-" + str(uuid.uuid4()))
    root.mkdir()
    if root.resolve().parent != parent.resolve():
        raise RuntimeError("Fixture is outside the workspace target folder")
    resources = ROOT / "apps/desktop/src-tauri/resources/freecad"
    for phase, body in [("capture", CAPTURE), ("verify", VERIFY)]:
        profile = root / phase
        profile.mkdir()
        (profile / "home").mkdir()
        (profile / "temp").mkdir()
        env = {key: os.environ[key] for key in ["SystemRoot", "WINDIR", "COMSPEC"] if key in os.environ}
        env.update({key: str(profile / "home") for key in ["USERPROFILE", "HOME", "APPDATA", "LOCALAPPDATA", "FREECAD_USER_HOME", "FREECAD_USER_DATA"]})
        env.update({key: str(profile / "temp") for key in ["TEMP", "TMP", "FREECAD_USER_TEMP"]})
        script = profile / (phase + ".FCMacro")
        script.write_text("import sys\nsys.path.insert(0," + repr(str(resources)) + ")\n" + body, encoding="utf-8")
        result = subprocess.run([str(executable), "--user-cfg", str(profile / "user.cfg"), "--system-cfg", str(profile / "system.cfg"), str(script)],
                                cwd=root, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=90)
        (profile / "stdout.private.txt").write_bytes(result.stdout)
        (profile / "stderr.private.txt").write_bytes(result.stderr)
        if result.returncode != 0 or not (root / ("capture.json" if phase == "capture" else "verified.json")).exists():
            print("Native copy proof failed during " + phase + "; private logs remain under target.")
            return 1
    verified = json.loads((root / "verified.json").read_text(encoding="utf-8"))
    summary = {key: verified[key] for key in ["source_preserved", "reopen_checked", "native_properties_checked", "native_geometry_checked", "native_primitives_recomputed", "cached_brep_bytes_equal", "headless", "gui_state_checked", "physical_function_checked"]}
    summary.update(json.loads((root / "capture-proof.json").read_text(encoding="utf-8")))
    print(json.dumps(summary))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
