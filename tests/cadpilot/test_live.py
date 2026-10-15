"""Exercise the fixed GUI-thread glue without opening or automating a native window."""
import copy
import io
import json
from pathlib import Path
import sys
import tempfile
import hashlib
import time
import types
import unittest
import uuid
import zipfile

RESOURCES = Path(__file__).resolve().parents[2] / "apps/desktop/src-tauri/resources/freecad"
SCRIPT = (RESOURCES / "cadpilot_policy.py").read_text() + "\n" + (RESOURCES / "live.py").read_text()


class Document:
    def __init__(self):
        self.Name = self.Label = "Enclosure"
        self.FileName = ""
        self.Objects = [types.SimpleNamespace(Name="Box", State=[], length=25.4, getTypeIdOfProperty=lambda name: "App::PropertyLength")]
        self.HasPendingTransaction = False
        self.UndoMode = 1
        self.UndoNames = []
        self.transactions = []

    def openTransaction(self, name):
        self.HasPendingTransaction = True
        self.pending = (name, copy.deepcopy(self.Objects))

    def commitTransaction(self):
        self.UndoNames.append(self.pending[0])
        self.transactions.append(self.pending[1])
        self.HasPendingTransaction = False

    def abortTransaction(self):
        self.Objects = self.pending[1]
        self.HasPendingTransaction = False

    def recompute(self):
        pass


class LiveTests(unittest.TestCase):
    def setUp(self):
        self.doc = Document()
        self.saved_folder = tempfile.TemporaryDirectory(prefix="hazlo-live-save-")
        self.saved_path = Path(self.saved_folder.name) / "Finished.FCStd"
        self.modified = True
        self.save_result = False
        self.save_calls = 0
        self.opened = {self.doc.Name: self.doc}
        self.calls = []
        self.draws = []
        self.rpc = types.ModuleType("rpc_server.rpc_server")
        self.dispatch_timeouts = []
        def dispatch(callback, **options):
            self.dispatch_timeouts.append(options["timeout"])
            return callback()
        self.rpc.dispatch_to_gui = dispatch

        def operation(doc, op):
            self.assertEqual(doc, self.doc.Name)
            self.calls.append(copy.deepcopy(op))
            self.doc.Objects[0].length = op["obj_properties"]["Length"]
            return {"success": not op.get("fail", False)}

        self.rpc.FreeCADRPC = lambda: types.SimpleNamespace(_run_one_operation=operation)
        app = types.ModuleType("FreeCAD")
        app.listDocuments = lambda: self.opened
        self.gui = types.ModuleType("FreeCADGui")
        self.active = self.doc
        self.gui.activeDocument = lambda: types.SimpleNamespace(Document=self.active)
        def save_as():
            self.save_calls += 1
            if self.save_result:
                self.saved_path.write_bytes(b"native fixture bytes")
                self.doc.FileName = str(self.saved_path)
                self.modified = False
            return self.save_result

        self.save_as = save_as
        self.gui.getDocument = lambda name: types.SimpleNamespace(Modified=self.modified, saveAs=lambda: self.save_as(), activeView=lambda: types.SimpleNamespace(redraw=lambda: self.draws.append(name)))
        package = types.ModuleType("rpc_server")
        package.rpc_server = self.rpc
        serialize = types.ModuleType("rpc_server.serialize")
        serialize.serialize_object = lambda obj: {"Name": obj.Name, "Label": "Box", "TypeId": "Part::Box", "Properties": {"Length": obj.length}}
        self.modules = {"FreeCAD": app, "FreeCADGui": self.gui, "rpc_server": package, "rpc_server.rpc_server": self.rpc, "rpc_server.serialize": serialize}
        self.originals = {key: sys.modules.get(key) for key in self.modules}
        sys.modules.update(self.modules)

    def tearDown(self):
        self.saved_folder.cleanup()
        for key, module in self.originals.items():
            if module is None:
                sys.modules.pop(key, None)
            else:
                sys.modules[key] = module

    def call(self, request):
        output = []
        exec(compile(SCRIPT, "hazlo-fixed-native-glue", "exec"), {"HAZLO_REQUEST": request, "task_print": output.append})
        self.assertEqual(len(output), 1)
        return json.loads(output[0])

    def inspect(self):
        result = self.call({"action": "inspect", "document_name": "Enclosure"})
        self.assertTrue(result["ok"], result)
        return result["result"]

    def request(self, observation, length=30.4):
        return {"action": "apply", "document_token": observation["document_token"], "state_sha256": observation["state_sha256"], "job_id": str(uuid.uuid4()), "expires_at": time.time() + 30, "operations": [{"action": "edit_object", "obj_name": "Box", "obj_properties": {"Length": length}}]}

    def export_fixture(self):
        self.export_path = Path(self.saved_folder.name) / "Finished.step"
        self.exported_paths = []
        self.solid = types.SimpleNamespace(Volume=100.0, BoundBox=types.SimpleNamespace(XMin=0.,YMin=0.,ZMin=0.,XMax=10.,YMax=5.,ZMax=2.), CenterOfMass=types.SimpleNamespace(x=5.,y=2.5,z=1.))
        def export(path):
            self.exported_paths.append(path)
            Path(path).write_bytes(b"native STEP fixture")
        self.export_shape = types.SimpleNamespace(isNull=lambda:False,isValid=lambda:True,Solids=[self.solid],exportStep=export)
        self.export_shape.copy = lambda: self.export_shape
        self.export_shape.exportBrepToString = lambda: "native BREP fixture"
        self.read_shape = types.SimpleNamespace(isValid=lambda:True,Solids=[copy.deepcopy(self.solid)],read=lambda path:None)
        part = types.ModuleType("Part")
        part.Shape = lambda: self.read_shape
        qt = types.ModuleType("PySide")
        self.choose_export = lambda: (str(self.export_path), "STEP")
        qt.QtGui = types.SimpleNamespace(QFileDialog=types.SimpleNamespace(getSaveFileName=lambda *args:self.choose_export()))
        geometry = types.ModuleType("rpc_server.geometry_query")
        geometry.global_shape = lambda obj: self.export_shape
        geometry.body_owner = lambda obj: None
        self.doc.Objects[0].getParentGeoFeatureGroup = lambda: None
        self.doc.Objects[0].Shape = self.export_shape
        self.doc.getObject = lambda name: self.doc.Objects[0] if name=="Box" else None
        self.gui.getMainWindow = lambda: None
        for key,module in {"Part":part,"PySide":qt,"rpc_server.geometry_query":geometry}.items():
            self.originals[key] = sys.modules.get(key)
            sys.modules[key] = module
        request = self.request(self.inspect())
        request["operations"] = [{"action":"export_step","obj_name":"Box"}]
        return request

    def test_step_export_uses_native_dialog_and_roundtrip_without_modifying_or_saving_model(self):
        request = self.export_fixture()
        self.doc.UndoMode = 0
        before = self.inspect()
        result = self.call(request)
        self.assertTrue(result["ok"],result)
        report = result["result"]
        self.assertEqual(report["after"], {key:before[key] for key in report["after"]})
        self.assertEqual(report["exported_document"], {"name":"Finished.step","bytes":19,"sha256":hashlib.sha256(b"native STEP fixture").hexdigest()})
        self.assertFalse(report["undo_available"])
        self.assertEqual(self.dispatch_timeouts[-1],600)
        self.assertEqual(self.doc.FileName,"")
        self.assertTrue(self.modified)
        self.assertEqual(self.calls,[])
        self.assertEqual(self.doc.UndoNames,[])
        self.assertNotIn(str(self.export_path),json.dumps(report))
        self.assertEqual(self.call(request)["error"],"already_attempted")

    def test_step_cancel_existing_destination_and_changed_document_do_not_export(self):
        request = self.export_fixture()
        self.choose_export = lambda: ("","")
        self.assertEqual(self.call(request)["error"],"native_export_not_completed")
        self.export_path.write_bytes(b"preserve this file")
        self.choose_export = lambda: (str(self.export_path),"STEP")
        request["job_id"] = str(uuid.uuid4())
        self.assertEqual(self.call(request)["error"],"native_export_destination_exists")
        self.assertEqual(self.export_path.read_bytes(),b"preserve this file")
        self.export_path = self.export_path.with_name("new.step")
        def changed():
            self.doc.Objects[0].length += 1
            return str(self.export_path),"STEP"
        self.choose_export = changed
        request["job_id"] = str(uuid.uuid4())
        self.assertEqual(self.call(request)["error"],"changed_document")
        self.assertEqual(self.exported_paths,[])
        self.assertFalse(self.export_path.exists())

    def test_step_roundtrip_mismatch_is_uncertain_and_never_replayed(self):
        request = self.export_fixture()
        self.read_shape.Solids[0].BoundBox.XMax += 1
        result = self.call(request)
        self.assertFalse(result["ok"])
        self.assertEqual(self.inspect()["recent_attempts"][request["job_id"]]["status"],"uncertain")
        self.assertEqual(self.call(request)["error"],"already_attempted")
        self.assertEqual(len(self.exported_paths),1)
        self.assertEqual(self.doc.Objects[0].length,25.4)
        self.assertEqual(self.doc.FileName,"")

    def test_save_as_uses_native_dialog_verifies_file_and_preserves_geometry_and_undo(self):
        before = self.inspect()
        self.assertEqual(self.dispatch_timeouts[-1], 60)
        request = self.request(before)
        request["operations"] = [{"action": "save_as"}]
        self.save_result = True
        self.doc.UndoMode = 0
        result = self.call(request)
        self.assertTrue(result["ok"], result)
        self.assertEqual(self.dispatch_timeouts[-1], 600)
        saved = result["result"]["saved_document"]
        self.assertEqual(saved, {"name": "Finished.FCStd", "bytes": 20, "sha256": hashlib.sha256(b"native fixture bytes").hexdigest()})
        self.assertFalse(result["result"]["undo_available"])
        self.assertEqual(before["objects"], result["result"]["after"]["objects"])
        self.assertTrue(result["result"]["after"]["has_saved_file"])
        self.assertFalse(result["result"]["after"]["modified"])
        self.assertEqual(self.save_calls, 1)
        self.assertEqual(self.calls, [])
        self.assertEqual(self.doc.UndoNames, [])
        self.assertNotIn(str(self.saved_path), json.dumps(result))
        self.assertEqual(self.call(request)["error"], "changed_document")

    def test_save_as_cancel_and_uncertain_failure_do_not_claim_a_completed_save(self):
        request = self.request(self.inspect())
        request["operations"] = [{"action": "save_as"}]
        self.assertEqual(self.call(request)["error"], "native_save_not_completed")
        self.assertFalse(self.saved_path.exists())
        self.assertEqual(self.inspect()["recent_attempts"][request["job_id"]]["status"], "not_completed")
        self.assertEqual(self.call(request)["error"], "already_attempted")
        self.assertEqual(self.save_calls, 1)

        def failed():
            self.saved_path.write_bytes(b"possibly saved")
            raise RuntimeError("private path or native error must not escape")

        self.save_as = failed
        request["job_id"] = str(uuid.uuid4())
        result = self.call(request)
        self.assertEqual(result, {"ok": False, "error": "native_operation_uncertain"})
        self.assertEqual(self.inspect()["recent_attempts"][request["job_id"]]["status"], "uncertain")
        self.assertEqual(self.call(request)["error"], "already_attempted")
        self.assertEqual(self.calls, [])

    def test_native_save_dialog_blocks_another_write_but_allows_readback(self):
        request = self.request(self.inspect())
        request["operations"] = [{"action": "save_as"}]
        save = self.save_as

        def pending():
            fresh = self.inspect()
            self.assertEqual(fresh["recent_attempts"][request["job_id"]]["status"], "running")
            self.assertEqual(self.call(self.request(fresh))["error"], "native_operation_pending")
            return save()

        self.save_as = pending
        self.save_result = True
        self.assertTrue(self.call(request)["ok"])
        self.assertEqual(self.calls, [])

    def test_native_container_shape_cache_does_not_invalidate_persisted_state(self):
        obj = self.doc.Objects[0]
        obj.getTypeIdOfProperty = lambda name: "Part::PropertyShapeCache" if name=="_Part_ShapeCache" else "App::PropertyLength"
        count = [0]
        def serialized(obj):
            count[0] += 1
            return {"Name":obj.Name,"Properties":{"Length":obj.length,"_Part_ShapeCache":f"<wrapper {count[0]}>"}}
        self.modules["rpc_server.serialize"].serialize_object = serialized
        before = self.inspect()
        self.assertNotIn("_Part_ShapeCache",before["objects"][0]["Properties"])
        self.assertEqual(before["state_sha256"],self.inspect()["state_sha256"])
        obj.length += 1
        self.assertNotEqual(before["state_sha256"],self.inspect()["state_sha256"])

    def test_material_archive_timestamps_do_not_change_state_but_content_does(self):
        obj = self.doc.Objects[0]
        obj.getTypeIdOfProperty = lambda name: "App::PropertyMaterial"
        material = {"uuid": "original", "second": 0}

        def dump(name):
            data = io.BytesIO()
            with zipfile.ZipFile(data, "w") as archive:
                entry = zipfile.ZipInfo("Persistence.xml", (2026, 10, 14, 12, 0, material["second"]))
                archive.writestr(entry, material["uuid"])
            return data.getvalue()

        obj.dumpPropertyContent = dump
        first = self.inspect()
        material["second"] = 4
        self.assertEqual(first["state_sha256"], self.inspect()["state_sha256"])
        material["uuid"] = "changed"
        self.assertNotEqual(first["state_sha256"], self.inspect()["state_sha256"])
        self.assertEqual(self.call(self.request(first))["error"], "changed_document")
        self.assertEqual(self.calls, [])

    def test_two_native_changes_read_the_current_state_and_keep_one_undo_entry_each(self):
        first = self.inspect()
        self.assertFalse(self.doc.HasPendingTransaction)
        self.assertEqual(self.calls, [])
        self.assertEqual(self.doc.UndoNames, [])
        for length in [30.4, 35.4]:
            before = self.inspect()
            result = self.call(self.request(before, length))
            self.assertTrue(result["ok"], result)
            self.assertTrue(result["result"]["undo_available"])
            observed = self.inspect()
            self.assertEqual(observed["objects"][0]["Properties"]["Length"], length)
            self.assertEqual(result["result"]["after"]["state_sha256"], observed["state_sha256"])
            self.assertNotEqual(first["state_sha256"], observed["state_sha256"])
        self.assertEqual(len(self.doc.UndoNames), 2)
        self.assertEqual(self.doc.transactions[0][0].length, 25.4)
        self.assertEqual(self.draws, ["Enclosure", "Enclosure"])

    def test_manual_changes_and_close_reopen_with_identical_geometry_refuse_old_approvals(self):
        before = self.inspect()
        self.doc.Objects[0].length = 22.
        self.assertEqual(self.call(self.request(before))["error"], "changed_document")
        self.opened["Enclosure"] = Document()
        self.assertEqual(self.call(self.request(before))["error"], "stale_document")
        self.assertNotEqual(self.inspect()["document_token"], before["document_token"])
        self.assertEqual(self.calls, [])

    def test_failed_later_native_operation_aborts_the_whole_transaction(self):
        before = self.inspect()
        request = self.request(before)
        request["operations"].append({"action": "edit_object", "obj_name": "Box", "obj_properties": {"Length": 99.}, "fail": True})
        result = self.call(request)
        self.assertEqual(result, {"ok": False, "error": "native_operation_rolled_back"})
        after = self.inspect()
        self.assertEqual(after["state_sha256"], before["state_sha256"])
        self.assertTrue(after["recent_attempts"][request["job_id"]]["rollback_verified"])
        self.assertEqual(self.doc.UndoNames, [])

    def test_expired_hidden_pending_and_undo_disabled_requests_do_not_write(self):
        before = self.inspect()
        request = self.request(before)
        request["expires_at"] = time.time() - 1
        self.assertEqual(self.call(request)["error"], "expired_request")
        self.active = Document()
        self.assertEqual(self.call(self.request(before))["error"], "document_not_visible")
        self.active = self.doc
        self.doc.HasPendingTransaction = True
        self.assertEqual(self.call(self.request(before))["error"], "native_undo_unavailable")
        self.doc.HasPendingTransaction = False
        self.doc.UndoMode = 0
        self.assertEqual(self.call(self.request(before))["error"], "native_undo_unavailable")
        self.assertEqual(self.calls, [])

    def test_duplicate_job_is_refused_even_after_user_undo_restores_its_initial_state(self):
        before = self.inspect()
        request = self.request(before)
        self.assertTrue(self.call(request)["ok"])
        self.doc.Objects = self.doc.transactions.pop()
        self.assertEqual(self.call(request)["error"], "already_attempted")
        self.assertEqual(len(self.calls), 1)


if __name__ == "__main__":
    unittest.main()
