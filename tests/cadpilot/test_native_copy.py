"""Native-copy boundaries; the separate opt-in probe uses real FreeCADCmd."""
import copy
import json
from pathlib import Path
import sys
import tempfile
import types
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[2]
RESOURCES = ROOT / "apps/desktop/src-tauri/resources/freecad"
sys.path.insert(0, str(RESOURCES))
import native_copy as native
import cadpilot_policy as policy
sys.path.pop(0)


class NativeCopy(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory(prefix="hazlo-owned-native-copy-")
        self.root = Path(self.folder.name)
        self.path = self.root / "Owned.FCStd"
        self.path.write_bytes((ROOT / "docs/research/cadpilot-primitives-2026-10-11/artifacts/Original.FCStd").read_bytes())
        self.facts = {"label": "Owned", "units": "mm", "coordinate_frame": "document_world", "objects": {"Box": {"type": "Part::Box", "properties": {}, "solids": 1, "measures_mm": [10.] * 10, "brep_sha256": "a" * 64}}}
        self.captured = {"version": 1, "file": native._file(self.path), "source_preserved": True,
                         "expected": copy.deepcopy(self.facts), "reopen_checked": False}
        self.opened = self.closed = 0
        self.app = types.ModuleType("FreeCAD")
        self.app.GuiUp = False
        self.app.Version = lambda: ["1", "1", "4"]
        self.app.listDocuments = lambda: {}
        self.app.openDocument = self.open
        self.app.closeDocument = self.close
        self.doc = types.SimpleNamespace(Name="Owned", Label="Owned", HasPendingTransaction=False, Objects=[], recompute=lambda: None)
        self.native_calls = 0
        self.doc.saveCopy = self.save_copy

    def tearDown(self):
        self.folder.cleanup()

    def open(self, path):
        self.opened += 1
        return self.doc

    def close(self, name):
        self.assertEqual(name, "Owned")
        self.closed += 1

    def save_copy(self, path):
        self.native_calls += 1
        Path(path).write_bytes(self.path.read_bytes())

    def test_capture_is_once_preserves_original_and_never_overwrites_existing_work(self):
        destination = self.root / "version" / self.path.name
        destination.parent.mkdir()
        original = self.path.read_bytes()
        state = {"filename": "Original.FCStd", "modified": True, "manual_label": "Keep"}
        with patch.object(native, "facts", return_value=self.facts):
            result = native.capture(self.doc, destination, lambda _: copy.deepcopy(state))
            self.assertTrue(result["source_preserved"])
            self.assertFalse(result["reopen_checked"])
            self.assertEqual(result["expected"], self.facts)
            for path in [destination, self.path]:
                with self.assertRaisesRegex(ValueError, "destination_exists"):
                    native.capture(self.doc, path, lambda _: copy.deepcopy(state))
        self.assertEqual(self.native_calls, 1)
        self.assertEqual(self.path.read_bytes(), original)
        self.assertEqual(destination.read_bytes(), original)

    def test_uncertain_capture_keeps_partial_copy_and_refuses_a_repeat(self):
        destination = self.root / "partial" / "Owned.FCStd"
        destination.parent.mkdir()
        states = iter([{"modified": True}, {"modified": False}])
        with patch.object(native, "facts", return_value=self.facts):
            with self.assertRaisesRegex(ValueError, "source_changed"):
                native.capture(self.doc, destination, lambda _: next(states))
            with self.assertRaisesRegex(ValueError, "destination_exists"):
                native.capture(self.doc, destination, lambda _: {})
        self.assertEqual(self.native_calls, 1)
        self.assertEqual(destination.read_bytes(), self.path.read_bytes())

    def test_capture_does_not_silently_rename_a_manual_document_label(self):
        self.doc.Label = "User label that differs from a filename"
        with self.assertRaisesRegex(ValueError, "label_requires_approval"):
            native.capture(self.doc, self.root / "Other.FCStd", lambda _: {})
        self.assertEqual(self.native_calls, 0)
        self.assertFalse((self.root / "Other.FCStd").exists())

    def test_windows_device_names_and_invalid_names_never_reach_native_save(self):
        for label in ["CON", "COM1", "LPT¹", "NUL.part", "CONOUT$", "bad:name", "bad?name", "bad\x01name"]:
            self.doc.Label=label
            with self.assertRaisesRegex(ValueError, "destination_invalid"):
                native.capture(self.doc,self.root/(label+".FCStd"),lambda _: {})
        self.assertEqual(self.native_calls,0)

    def test_native_primitive_properties_and_frames_are_exact_while_kernel_rounding_is_bounded(self):
        for key,value in [("label", "changed"),("units", "in"),("coordinate_frame", "different")]:
            changed=copy.deepcopy(self.facts)
            changed[key]=value
            self.assertFalse(native._matches(changed,self.facts))
        for key,value in [("properties", {"Placement": "different native property"}),("type", "Part::Sphere"),("solids", 2)]:
            changed=copy.deepcopy(self.facts)
            changed["objects"]["Box"][key]=value
            self.assertFalse(native._matches(changed,self.facts))
        changed=copy.deepcopy(self.facts)
        changed["objects"]["Box"]["brep_sha256"]="b"*64
        changed["objects"]["Box"]["measures_mm"][1]+=1e-12
        self.assertTrue(native._matches(changed,self.facts))
        changed["objects"]["Box"]["measures_mm"][1]+=0.001
        self.assertFalse(native._matches(changed,self.facts))
        changed=copy.deepcopy(self.facts)
        changed["objects"]["Box"]["measures_mm"][0]+=0.001
        self.assertFalse(native._matches(changed,self.facts))
        changed=copy.deepcopy(self.facts)
        changed["objects"]["Box"]["measures_mm"][0]=float("nan")
        self.assertFalse(native._matches(changed,self.facts))

    def test_unsupported_or_pending_native_state_cannot_start_capture(self):
        for pending, unsupported in [(True, False), (False, True)]:
            self.doc.HasPendingTransaction = pending
            with patch.object(native, "facts", side_effect=ValueError("unsupported") if unsupported else None, return_value=self.facts):
                with self.assertRaises(ValueError):
                    native.capture(self.doc, self.root / "New.FCStd", lambda _: {})
        self.assertEqual(self.native_calls, 0)

    def test_verify_checks_policy_before_open_then_matches_native_state_without_rewriting_bytes(self):
        original = self.path.read_bytes()
        def checked(path, *, headless):
            self.assertEqual(self.opened, 0)
            self.assertTrue(headless)
            self.assertEqual(path, self.path)
            return {"Box": "Part::Box"}
        with patch.dict(sys.modules, {"FreeCAD": self.app}), patch.object(native, "document", side_effect=checked), patch.object(native, "facts", return_value=self.facts):
            result = native.verify(self.path, self.captured)
        self.assertEqual((self.opened, self.closed), (1, 1))
        for key in ["reopen_checked", "native_properties_checked", "native_geometry_checked", "native_primitives_recomputed", "headless"]:
            self.assertTrue(result[key])
        self.assertFalse(result["gui_state_checked"])
        self.assertFalse(result["physical_function_checked"])
        self.assertEqual(result["external_dependencies"], [])
        self.assertTrue(any(item["path"] == "Document.xml" for item in result["embedded_resources"]))
        self.assertEqual(self.path.read_bytes(), original)
        self.assertNotIn(str(self.root), json.dumps(result))

    def test_tampered_copy_wrong_runtime_and_unsupported_dependency_never_open(self):
        with patch.dict(sys.modules, {"FreeCAD": self.app}):
            self.app.GuiUp = True
            with self.assertRaisesRegex(ValueError, "verifier_runtime"):
                native.verify(self.path, self.captured)
            self.app.GuiUp = False
            self.app.listDocuments = lambda: {"Existing": self.doc}
            with self.assertRaisesRegex(ValueError, "verifier_runtime"):
                native.verify(self.path, self.captured)
            self.app.listDocuments = lambda: {}
            self.app.Version = lambda: ["1", "0", "0"]
            with self.assertRaisesRegex(ValueError, "verifier_runtime"):
                native.verify(self.path, self.captured)
            self.app.Version = lambda: ["1", "1", "4"]
            changed = copy.deepcopy(self.captured)
            changed["file"]["sha256"] = "0" * 64
            with self.assertRaisesRegex(ValueError, "changed"):
                native.verify(self.path, changed)
            with patch.object(native, "document", side_effect=ValueError("uncaptured dependency")):
                with self.assertRaisesRegex(ValueError, "dependency"):
                    native.verify(self.path, self.captured)
        self.assertEqual((self.opened, self.closed), (0, 0))

    def test_failed_reopen_comparison_closes_only_the_verification_copy(self):
        altered = copy.deepcopy(self.facts)
        altered["label"] = "Different native label"
        with patch.dict(sys.modules, {"FreeCAD": self.app}), patch.object(native, "document", return_value={"Box": "Part::Box"}), patch.object(native, "facts", return_value=altered):
            with self.assertRaisesRegex(ValueError, "reopen_mismatch"):
                native.verify(self.path, self.captured)
        self.assertEqual((self.opened, self.closed), (1, 1))
        self.assertEqual(native._file(self.path), self.captured["file"])

    def test_headless_policy_keeps_default_gui_fidelity_gate_and_still_refuses_code_and_links(self):
        with zipfile.ZipFile(self.path) as archive:
            entries = {item.filename: archive.read(item) for item in archive.infolist()}
        entries["GuiDocument.xml"] = b'<Document SchemaVersion="1"><Camera settings="arbitrary native GUI state"/></Document>'
        def write(values):
            with zipfile.ZipFile(self.path, "w") as archive:
                for name, data in values.items():
                    archive.writestr(name, data)
        write(entries)
        with self.assertRaisesRegex(ValueError, "GUI document state"):
            policy.document(self.path)
        self.assertTrue(policy.document(self.path, headless=True))
        for payload in [b'<Python/>', b'<XLink/>', b'<Link value="outside.FCStd"/>', b'<ExpressionEngine count="1"/>']:
            bad = dict(entries)
            bad["Document.xml"] = entries["Document.xml"].replace(b'</Document>', payload + b'</Document>')
            write(bad)
            with self.assertRaises(ValueError):
                policy.document(self.path, headless=True)


if __name__ == "__main__":
    unittest.main()
