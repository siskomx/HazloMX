"""Exercise the fixed GUI-thread glue without opening or automating a native window."""
import copy
import json
from pathlib import Path
import sys
import time
import types
import unittest
import uuid

SCRIPT = (Path(__file__).resolve().parents[2] / "apps/desktop/src-tauri/resources/freecad/live.py").read_text()


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
        self.opened = {self.doc.Name: self.doc}
        self.calls = []
        self.draws = []
        self.rpc = types.ModuleType("rpc_server.rpc_server")
        self.rpc.dispatch_to_gui = lambda callback: callback()

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
        self.gui.getDocument = lambda name: types.SimpleNamespace(activeView=lambda: types.SimpleNamespace(redraw=lambda: self.draws.append(name)))
        package = types.ModuleType("rpc_server")
        package.rpc_server = self.rpc
        serialize = types.ModuleType("rpc_server.serialize")
        serialize.serialize_object = lambda obj: {"Name": obj.Name, "Label": "Box", "TypeId": "Part::Box", "Properties": {"Length": obj.length}}
        self.modules = {"FreeCAD": app, "FreeCADGui": self.gui, "rpc_server": package, "rpc_server.rpc_server": self.rpc, "rpc_server.serialize": serialize}
        self.originals = {key: sys.modules.get(key) for key in self.modules}
        sys.modules.update(self.modules)

    def tearDown(self):
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
