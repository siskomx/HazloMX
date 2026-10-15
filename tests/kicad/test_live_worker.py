"""Native worker semantics using real SDK objects and Hazlo's actual Rust verifier."""
import argparse
import importlib.util
import os
from pathlib import Path
import sys
import subprocess
import unittest
import uuid

parser = argparse.ArgumentParser()
parser.add_argument("--site", required=True)
parser.add_argument("--verifier", required=True)
args = parser.parse_args()
sys.path.insert(0, args.site)
os.environ["HAZLOMEXA_PCB_VERIFIER"] = str(Path(args.verifier).resolve())
path = Path(__file__).resolve().parents[2] / "apps/desktop/src-tauri/resources/kicad/live.py"
spec = importlib.util.spec_from_file_location("hazlo_kicad_native", path)
worker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(worker)

from kipy.board_types import FootprintInstance, Pad
from kipy.errors import ConnectionError
from kipy.geometry import Vector2
from kipy.proto.common.commands import BeginCommitResponse, EndCommitResponse, SavedDocumentResponse
from kipy.proto.common.types import DocumentSpecifier, DocumentType

IDENTIFIER = "1390739f-0b91-4d50-b8ae-92acd725ec51"


class Board:
    def __init__(self):
        self.document = DocumentSpecifier(type=DocumentType.DOCTYPE_PCB, board_filename="OwnedBoard.kicad_pcb")
        self.client = self
        self.item = FootprintInstance()
        self.item.id.value = IDENTIFIER
        self.item.locked = False
        self.item.position = Vector2.from_xy_mm(10, 20)
        self.item.reference_field.text.value = "R1"
        self.item.value_field.text.value = "10k"
        pad = Pad()
        pad.id.value = "b682b6e3-d96d-4d83-97c7-cd78fa153986"
        pad.position = Vector2.from_xy_mm(11, 21)
        self.item.definition.items = [pad]
        self.original = FootprintInstance(self.item.proto)
        self.extra = False
        self.fault = ""
        self.calls = []
        self.committed = False

    def snapshot(self):
        x, y = self.item.position.x / 1_000_000, self.item.position.y / 1_000_000
        extra = '(net 1 "unapproved")' if self.extra else ""
        return f'(kicad_pcb (footprint "Device:R" (uuid "{IDENTIFIER}") (at {x:.6f} {y:.6f}) (property "Reference" "R1")) {extra})'

    def get_footprints(self):
        return [FootprintInstance(self.item.proto)]

    def update_items(self, items):
        self.calls.append("update")
        self.item = FootprintInstance(items[0].proto)
        if self.fault == "lost_update":
            raise ConnectionError("private native transport detail")
        if self.fault == "malformed_update":
            raise ValueError("private malformed native response")
        self.extra = self.fault == "unrelated_change"
        return [] if self.fault == "partial_update" else [FootprintInstance(self.item.proto)]

    def send(self, command, response_type):
        name = command.DESCRIPTOR.name
        if name == "SaveDocumentToString":
            if self.fault == "lost_readback" and self.committed:
                raise ConnectionError("private native readback detail")
            return SavedDocumentResponse(document=self.document, contents=self.snapshot())
        if name == "BeginCommit":
            self.calls.append("begin")
            if self.fault == "lost_begin":
                raise ConnectionError("private native begin detail")
            reply = BeginCommitResponse()
            reply.id.value = "owned-commit"
            return reply
        if name == "EndCommit":
            if command.action == 2:
                self.calls.append("drop")
                if self.fault == "lost_drop":
                    raise ConnectionError("private native rollback detail")
                self.item = FootprintInstance(self.original.proto)
                self.extra = False
            else:
                self.calls.append("commit")
                self.committed = True
                if self.fault == "lost_commit":
                    raise ConnectionError("private native commit detail")
            return EndCommitResponse()
        raise AssertionError(name)


class Tests(unittest.TestCase):
    def setUp(self):
        self.board = Board()
        self.before, _, _ = worker.observation(self.board, "a" * 64)
        self.request = {key: self.before[key] for key in ("state_sha256", "instance_sha256", "document_sha256")}
        self.request["movement"] = {"reference": "R1", "uuid": IDENTIFIER,
            "expected_x_mm": 10., "expected_y_mm": 20., "x_mm": 35.4, "y_mm": 22.5}

    def apply(self, verify=worker.verify_with_host):
        return worker.apply_move(self.board, "a" * 64, self.request, verify)

    def test_actual_sdk_move_and_rust_whole_board_verifier_complete_one_commit(self):
        result = self.apply()
        self.assertEqual(result["status"], "completed")
        self.assertTrue(result["native_commit_confirmed"])
        self.assertFalse(result["saved"])
        self.assertEqual(self.board.calls, ["begin", "update", "commit"])
        self.assertEqual(result["after"]["components"][0]["x_mm"], 35.4)
        self.assertEqual(self.board.item.reference_field.text.position.x, 35_400_000)
        self.assertEqual(self.board.item.definition.items[0].position.x, 36_400_000)
        self.assertEqual(self.board.item.definition.items[0].position.y, 23_500_000)

    def test_stale_binding_and_invalid_moves_never_begin(self):
        for key in ("state_sha256", "instance_sha256", "document_sha256"):
            original = self.request[key]
            self.request[key] = "b" * 64
            with self.assertRaises(worker.Refused):
                self.apply()
            self.request[key] = original
        for value in (10., float("nan"), 10001., 35.4000004):
            self.request["movement"]["x_mm"] = value
            self.request["movement"]["y_mm"] = 20.
            with self.assertRaises(worker.Refused):
                self.apply()
        self.assertEqual(self.board.calls, [])

    def test_unrelated_changes_and_partial_native_updates_are_rolled_back(self):
        for fault in ("unrelated_change", "partial_update"):
            with self.subTest(fault=fault):
                self.setUp()
                original = self.board.snapshot()
                self.board.fault = fault
                self.assertEqual(self.apply()["status"], "rolled_back")
                self.assertEqual(self.board.calls, ["begin", "update", "drop"])
                self.assertEqual(self.board.snapshot(), original)

    def test_lost_replies_never_cause_a_blind_drop_or_retry(self):
        for fault, calls in [("lost_begin", ["begin"]),
                             ("lost_update", ["begin", "update"]),
                             ("malformed_update", ["begin", "update"]),
                             ("lost_commit", ["begin", "update", "commit"]),
                             ("lost_readback", ["begin", "update", "commit"])]:
            with self.subTest(fault=fault):
                self.setUp()
                self.board.fault = fault
                with self.assertRaises(worker.Uncertain):
                    self.apply()
                self.assertEqual(self.board.calls, calls)

    def test_change_during_host_verification_is_uncertain_without_drop(self):
        def concurrent(before, after, movement):
            valid = worker.verify_with_host(before, after, movement)
            if after is not None:
                self.board.extra = True
            return valid
        with self.assertRaises(worker.Uncertain):
            self.apply(concurrent)
        self.assertEqual(self.board.calls, ["begin", "update"])
        self.assertTrue(self.board.extra)

    def test_failed_rollback_stays_uncertain(self):
        self.board.fault = "lost_drop"
        with self.assertRaises(worker.Uncertain):
            self.apply(lambda before, after, movement: after is None)
        self.assertEqual(self.board.calls, ["begin", "update", "drop"])

    def test_locked_components_and_missing_verifier_never_begin(self):
        self.board.item.locked = True
        with self.assertRaises(worker.Refused):
            self.apply()
        self.board.item.locked = False
        original = os.environ.pop("HAZLOMEXA_PCB_VERIFIER")
        try:
            with self.assertRaises(worker.Refused):
                self.apply()
        finally:
            os.environ["HAZLOMEXA_PCB_VERIFIER"] = original
        self.assertEqual(self.board.calls, [])

    def test_private_cli_validator_does_not_echo_native_contents(self):
        self.assertFalse(worker.verify_with_host("private data", "unapproved data", self.request["movement"]))
        rejected = subprocess.run([args.verifier, "--verify-pcb-move"], input=b"private data",
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=10)
        self.assertEqual(rejected.stdout.strip(), b'{"valid":false}')
        self.assertEqual(rejected.stderr, b"")
        with self.assertRaises(worker.Refused):
            worker.unique_fields([("action", "inspect"), ("action", "move")])

    def test_response_never_reports_rollback_as_success_or_exposes_native_errors(self):
        original = worker.run
        try:
            worker.run = lambda request: {"status":"rolled_back", "error":"private native detail"}
            self.assertEqual(worker.response(b"{}"), {"ok":False,"outcome":"rolled_back","error":"native_move_refused"})
            def lost(request):
                raise worker.Uncertain("private native detail")
            worker.run = lost
            self.assertEqual(worker.response(b"{}"), {"ok":False,"outcome":"uncertain","error":"native_outcome_unknown"})
            self.assertEqual(worker.response(b'{"action":"inspect","action":"move"}')["outcome"], "refused")
            self.assertEqual(worker.response(b" " * 131073)["outcome"], "refused")
        finally:
            worker.run = original

    def test_public_inspection_and_move_use_real_sdk_board_and_version_wrappers(self):
        from kipy.proto.common.types import ItemHeader
        from google.protobuf.unknown_fields import UnknownFieldSet
        board = self.board
        board.document.project.path = "C:/owned"

        class WireClient:
            def __init__(self, *args):
                self._kicad_token = "owned-fixture-token"
                self.connected = False

            def send(self, command, response_type):
                name = command.DESCRIPTOR.name
                if name == "GetVersion":
                    response = response_type()
                    response.version.major = 10
                    response.version.minor = 0
                    response.version.patch = 7
                    return response
                if name == "GetOpenDocuments":
                    response = response_type()
                    response.documents.append(board.document)
                    return response
                if name == "GetItems":
                    assert command.header.document == board.document
                    response = response_type()
                    response.status = 1
                    response.items.add().Pack(board.item.proto)
                    return response
                if name == "UpdateItems":
                    assert command.header.document == board.document
                    proto = FootprintInstance().proto
                    assert command.items[0].Unpack(proto)
                    updated = board.update_items([FootprintInstance(proto)])
                    response = response_type()
                    response.status = 1
                    for item in updated:
                        result = response.updated_items.add()
                        result.status.code = 1
                        result.item.Pack(item.proto)
                    return response
                if name in {"BeginCommit", "EndCommit"}:
                    field = next(iter(UnknownFieldSet(command)))
                    assert field.field_number == (1 if name == "BeginCommit" else 4)
                    assert ItemHeader.FromString(field.data).document == board.document
                return board.send(command, response_type)

        original_client = worker.BoundClient
        original_endpoint = os.environ.get("KICAD_API_SOCKET")
        worker.BoundClient = WireClient
        os.environ["KICAD_API_SOCKET"] = "ipc://C:/owned/api.sock"
        try:
            base = {"job_id": str(uuid.uuid4()), "board_path": "C:/owned/OwnedBoard.kicad_pcb"}
            observed = worker.run({**base, "action": "inspect"})
            self.assertEqual(observed["job_id"], base["job_id"])
            self.assertEqual(observed["native_pcb_version"], 1)
            self.assertNotIn("C:/owned", str(observed))
            request = {**base, "action": "move", "movement": self.request["movement"],
                **{key:observed[key] for key in ("instance_sha256", "document_sha256", "state_sha256")}}
            result = worker.run(request)
            self.assertEqual(result["status"], "completed")
            self.assertEqual(board.calls, ["begin", "update", "commit"])
        finally:
            worker.BoundClient = original_client
            if original_endpoint is None:
                os.environ.pop("KICAD_API_SOCKET", None)
            else:
                os.environ["KICAD_API_SOCKET"] = original_endpoint


unittest.main(argv=[sys.argv[0]])
