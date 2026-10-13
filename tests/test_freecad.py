"""Run with HAZLOMEXA_FREECAD_CMD pointing to the provisioned FreeCADCmd.exe."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import uuid

REPO = Path(__file__).resolve().parents[1]
CREATE = REPO / "apps/desktop/src-tauri/resources/freecad/create-solid.FCMacro"
VERIFY = REPO / "tests/freecad/verify-artifacts.FCMacro"
EXECUTABLE = os.environ.get("HAZLOMEXA_FREECAD_CMD")


@unittest.skipUnless(EXECUTABLE, "Set HAZLOMEXA_FREECAD_CMD for native geometry tests")
class FreeCADIntegration(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="Hazlo Mexa á's ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        # FreeCAD 1.1.4 fails to initialize with accented configuration paths.
        self.profile_temp = tempfile.TemporaryDirectory(prefix="hazlomexa-profile-")
        self.addCleanup(self.profile_temp.cleanup)
        self.profile_root = Path(self.profile_temp.name)
        self.request = {"schema_version": 1, "job_id": str(uuid.uuid4()),
                        "dimensions_mm": {"length": 10, "width": 20, "height": 30}}

    def execute(self, macro=CREATE, raw=None, missing_input=False):
        if not missing_input:
            (self.root / "input.json").write_text(
                json.dumps(self.request) if raw is None else raw, encoding="utf-8")
        environment = dict(os.environ)
        for key in ("PYTHONPATH", "PYTHONHOME", "FREECAD_USER_HOME", "FREECAD_USER_DATA", "FREECAD_USER_TEMP"):
            environment.pop(key, None)
        for key, directory in (("FREECAD_USER_HOME", "profile"), ("FREECAD_USER_DATA", "data"), ("FREECAD_USER_TEMP", "temp")):
            location = self.profile_root / directory
            location.mkdir(exist_ok=True)
            environment[key] = str(location)
        return subprocess.run(
            [EXECUTABLE, "--user-cfg", str(self.profile_root / "profile/user.cfg"),
             "--system-cfg", str(self.profile_root / "profile/system.cfg"), str(macro)],
            cwd=self.root, env=environment, stdin=subprocess.DEVNULL,
            capture_output=True, timeout=60)

    def result(self):
        return json.loads((self.root / "result.json").read_text(encoding="utf-8"))

    def test_native_and_step_round_trip(self):
        for size in ((10, 20, 30), (1, 1, 1), (200, 200, 200), (12.5, 7.25, 3.5)):
            with self.subTest(size=size):
                with tempfile.TemporaryDirectory(dir=self.root) as case:
                    original = self.root
                    self.root = Path(case)
                    try:
                        self.request["dimensions_mm"] = dict(zip(("length", "width", "height"), size))
                        run = self.execute()
                        self.assertEqual(run.returncode, 0, (run.stdout + run.stderr)[-4000:])
                        result = self.result()
                        self.assertTrue(result["success"], result)
                        self.assertEqual(result["job_id"], self.request["job_id"])
                        self.assertEqual(result["artifacts"], ["solid.FCStd", "solid.step"])
                        check = self.execute(VERIFY)
                        self.assertEqual(check.returncode, 0, (check.stdout + check.stderr)[-4000:])
                        self.assertTrue(json.loads((self.root / "verified.json").read_text())["success"])
                    finally:
                        self.root = original

    def test_rejects_invalid_dimensions_and_fields(self):
        cases = [False, "10", 0, 201, float("nan"), float("inf")]
        for value in cases:
            with self.subTest(value=value):
                self.request["dimensions_mm"]["length"] = value
                run = self.execute()
                self.assertNotEqual(run.returncode, 0)
                self.assertFalse(self.result()["success"])
                self.assertFalse((self.root / "solid.FCStd").exists())
                (self.root / "result.json").unlink()
        self.request["dimensions_mm"]["length"] = 10
        self.request["shell"] = "unapproved"
        self.execute()
        self.assertFalse(self.result()["success"])

    def test_rejects_duplicate_json_fields(self):
        self.execute(raw='{"schema_version":1,"schema_version":1}')
        self.assertFalse(self.result()["success"])
        self.assertFalse((self.root / "solid.FCStd").exists())

    def test_missing_input_and_unknown_version_fail(self):
        run = self.execute(missing_input=True)
        self.assertNotEqual(run.returncode, 0)
        self.assertFalse(self.result()["success"])
        (self.root / "result.json").unlink()
        self.request["schema_version"] = 2
        self.execute()
        self.assertFalse(self.result()["success"])
        self.assertFalse((self.root / "solid.FCStd").exists())

    def test_preserves_occupied_output(self):
        output = self.root / "solid.step"
        output.write_bytes(b"existing user bytes")
        run = self.execute()
        self.assertNotEqual(run.returncode, 0)
        self.assertFalse(self.result()["success"])
        self.assertEqual(output.read_bytes(), b"existing user bytes")
        self.assertFalse((self.root / "solid.FCStd").exists())

    def test_user_addon_and_python_path_are_not_loaded(self):
        poison = self.root / "untrusted-profile"
        addon = poison / "Mod/Probe"
        addon.mkdir(parents=True)
        sentinel = self.root / "untrusted-code-ran"
        code = "from pathlib import Path\nPath(" + repr(str(sentinel)) + ").write_text('ran')\n"
        (addon / "Init.py").write_text(code, encoding="utf-8")
        (poison / "sitecustomize.py").write_text(code, encoding="utf-8")
        with patch.dict(os.environ, {"PYTHONPATH": str(poison),
                                    "FREECAD_USER_HOME": str(poison),
                                    "FREECAD_USER_DATA": str(poison)}):
            run = self.execute()
        self.assertEqual(run.returncode, 0, (run.stdout + run.stderr)[-4000:])
        self.assertTrue(self.result()["success"])
        self.assertFalse(sentinel.exists())

    def test_result_write_failure_cannot_leave_a_success_record(self):
        (self.root / "result.tmp").write_bytes(b"occupied")
        self.execute()
        self.assertFalse((self.root / "result.json").exists())
        self.assertFalse((self.root / "solid.FCStd").exists())
        self.assertEqual((self.root / "result.tmp").read_bytes(), b"occupied")

    def test_export_exception_does_not_accept_partial_artifacts(self):
        source = CREATE.read_text(encoding="utf-8")
        export = 'box.Shape.exportStep(str(ROOT / "solid.step"))'
        self.assertEqual(source.count(export), 1)
        fault = self.profile_root / "forced-export-failure.FCMacro"
        fault.write_text(source.replace(export, 'raise OSError("forced export failure")'), encoding="utf-8")
        run = self.execute(fault)
        self.assertNotEqual(run.returncode, 0)
        self.assertFalse(self.result()["success"])
        self.assertIn("forced export failure", self.result()["error"])
        self.assertTrue((self.root / "solid.FCStd").exists())
        self.assertFalse((self.root / "solid.step").exists())


if __name__ == "__main__":
    unittest.main()
