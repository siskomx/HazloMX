"""Native worker semantics using real SDK objects and Hazlo's actual Rust verifier."""
import argparse
import importlib.util
import os
from pathlib import Path
import sys
import subprocess
import unittest
import uuid
import tempfile
from unittest.mock import patch

parser = argparse.ArgumentParser()
parser.add_argument("--site", required=True)
parser.add_argument("--verifier", required=True)
parser.add_argument("--host-apply", action="store_true", help="Exercise two approved production jobs in the controlled native server")
parser.add_argument("--host-runner", help="Desktop-feature Rust test executable for production read routing")
parser.add_argument("--worker", help="Frozen helper for real NNG subprocess coverage")
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
        self.pending = None

    def snapshot(self):
        x, y = self.item.position.x / 1_000_000, self.item.position.y / 1_000_000
        extra = '(net 1 "unapproved")' if self.extra else ""
        return f'(kicad_pcb (footprint "Device:R" (uuid "{IDENTIFIER}") (at {x:.6f} {y:.6f}) (property "Reference" "R1")) {extra})'

    def get_footprints(self):
        return [FootprintInstance(self.item.proto)]

    def update_items(self, items):
        self.calls.append("update")
        self.pending = FootprintInstance(items[0].proto)
        if self.fault == "lost_update":
            raise ConnectionError("private native transport detail")
        if self.fault == "malformed_update":
            raise ValueError("private malformed native response")
        self.extra = self.fault == "unrelated_change"
        return [] if self.fault in {"partial_update", "lost_drop"} else [FootprintInstance(self.pending.proto)]

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
            self.original = FootprintInstance(self.item.proto)
            self.original_extra = self.extra
            reply = BeginCommitResponse()
            reply.id.value = "owned-commit"
            return reply
        if name == "EndCommit":
            if command.action == 2:
                self.calls.append("drop")
                if self.fault == "lost_drop":
                    raise ConnectionError("private native rollback detail")
                self.item = FootprintInstance(self.original.proto)
                self.pending = None
                self.extra = self.original_extra
            else:
                self.calls.append("commit")
                self.item = self.pending
                self.pending = None
                self.committed = True
                if self.fault == "bad_commit":
                    self.extra = True
                if self.fault == "lost_commit":
                    raise ConnectionError("private native commit detail")
            return EndCommitResponse()
        raise AssertionError(name)


def native_reply(board, command, response_type):
    from kipy.proto.common.types import ItemHeader
    from google.protobuf.unknown_fields import UnknownFieldSet
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
        phases = []
        def verify(before, after, movement):
            phases.append("before" if after is None else "after")
            self.assertEqual(self.board.committed, after is not None)
            return worker.verify_with_host(before, after, movement)
        result = self.apply(verify)
        self.assertEqual(phases, ["before", "after"])
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

    def test_partial_queued_native_update_is_dropped_without_changing_the_board(self):
        original = self.board.snapshot()
        self.board.fault = "partial_update"
        self.assertEqual(self.apply()["status"], "rolled_back")
        self.assertEqual(self.board.calls, ["begin", "update", "drop"])
        self.assertEqual(self.board.snapshot(), original)

    def test_unrelated_change_before_commit_is_uncertain_without_drop(self):
        self.board.fault = "unrelated_change"
        with self.assertRaises(worker.Uncertain):
            self.apply()
        self.assertEqual(self.board.calls, ["begin", "update"])
        self.assertTrue(self.board.extra)

    def test_unapproved_native_commit_is_uncertain_and_never_rolled_back_blindly(self):
        self.board.fault = "bad_commit"
        with self.assertRaises(worker.Uncertain):
            self.apply()
        self.assertEqual(self.board.calls, ["begin", "update", "commit"])
        self.assertTrue(self.board.extra)

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
        self.assertEqual(self.board.calls, ["begin", "update", "commit"])
        self.assertTrue(self.board.extra)

    def test_failed_rollback_stays_uncertain(self):
        self.board.fault = "lost_drop"
        with self.assertRaises(worker.Uncertain):
            self.apply()
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

    @unittest.skipUnless(args.worker, "Supply --worker to test the frozen subprocess")
    def test_frozen_subprocess_inspects_and_moves_over_real_native_transport(self):
        import json
        import queue
        import threading
        import pynng
        from google.protobuf import symbol_database
        from kipy.proto.common import ApiRequest, ApiResponse, ApiStatusCode
        root = Path(__file__).resolve().parents[2] / "target/native-pcb-worker" / str(uuid.uuid4())
        root.mkdir()
        endpoint = "ipc://" + str(root / "api.sock")
        self.board.document.project.path = str(root) if args.host_runner else "C:/owned"
        failures = queue.Queue()
        stop = threading.Event()
        count = []
        types = {"GetVersion": "GetVersionResponse", "GetOpenDocuments": "GetOpenDocumentsResponse",
                 "GetItems": "GetItemsResponse", "UpdateItems": "UpdateItemsResponse",
                 "BeginCommit": "BeginCommitResponse", "EndCommit": "EndCommitResponse",
                 "SaveDocumentToString": "SavedDocumentResponse"}
        with pynng.Rep0(listen=endpoint, recv_timeout=100, send_timeout=1000) as server:
            def serve():
                while not stop.is_set():
                    try:
                        packet = server.recv()
                    except pynng.Timeout:
                        continue
                    try:
                        request = ApiRequest.FromString(packet)
                        self.assertIn(request.header.kicad_token, ("", "owned-fixture-token"))
                        name = request.message.type_url.rsplit(".", 1)[-1]
                        count.append(name)
                        cls = symbol_database.Default().GetSymbol("kiapi.common.commands." + name)
                        response_type = symbol_database.Default().GetSymbol("kiapi.common.commands." + types[name])
                        command = cls.FromString(request.message.value)
                        native = native_reply(self.board, command, response_type)
                        reply = ApiResponse()
                        reply.status.status = ApiStatusCode.AS_OK
                        reply.header.kicad_token = "owned-fixture-token"
                        reply.message.Pack(native)
                        server.send(reply.SerializeToString())
                    except BaseException as error:
                        failures.put(error)
                        return
            thread = threading.Thread(target=serve)
            thread.start()
            env = {key:os.environ[key] for key in ("SystemRoot", "WINDIR", "COMSPEC") if key in os.environ}
            env.update({"TEMP":str(root), "TMP":str(root), "KICAD_API_SOCKET":endpoint,
                        "HAZLOMEXA_PCB_VERIFIER":str(Path(args.verifier).resolve())})
            def call(request):
                result = subprocess.run([str(Path(args.worker).resolve())], input=json.dumps(request).encode(),
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env, cwd=root,
                    creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0), timeout=30)
                (root / ("call-" + str(len(count)) + ".log")).write_bytes(result.stdout + result.stderr)
                self.assertEqual(result.returncode, 0)
                self.assertEqual(result.stderr, b"")
                reply = json.loads(result.stdout)
                self.assertTrue(reply["ok"], reply)
                self.assertNotIn("C:/owned", str(reply))
                self.assertNotIn("owned-fixture-token", str(reply))
                return reply["result"]
            try:
                base = {"job_id":str(uuid.uuid4()), "board_path":"C:/owned/OwnedBoard.kicad_pcb"}
                if args.host_runner:
                    base["board_path"] = str(root / "OwnedBoard.kicad_pcb")
                    Path(base["board_path"]).write_bytes(b"Saved fixture bytes cannot stand in for native state")
                    host_env = {**os.environ, "HAZLOMEXA_TEST_NATIVE_PCB_ROOT":str(root)}
                    if args.host_apply:
                        host_env["HAZLOMEXA_TEST_NATIVE_PCB_APPLY"] = "1"
                    host = subprocess.run([str(Path(args.host_runner).resolve()), "--ignored", "--exact",
                        "host_tools::tests::actual_embedded_worker_reads_controlled_native_server", "--nocapture"],
                        capture_output=True, env=host_env, cwd=root, timeout=60,
                        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
                    (root / "host-runner.log").write_bytes(host.stdout + host.stderr)
                    self.assertEqual(host.returncode, 0, (host.stdout + host.stderr).decode(errors="replace")[-1500:])
                    initial = json.loads((root / "native-host-result.json").read_text())["result"]
                else:
                    initial = call({**base, "action":"inspect"})
                movement = self.request["movement"]
                if args.host_apply:
                    current = initial["components"][0]
                    movement = {**movement, "expected_x_mm":current["x_mm"], "expected_y_mm":current["y_mm"],
                                "x_mm":86.2, "y_mm":27.5}
                result = call({**base, "action":"move", "movement":movement,
                    **{key:initial[key] for key in ("instance_sha256", "document_sha256", "state_sha256")}})
                self.assertEqual(result["status"], "completed")
                self.assertFalse(result["saved"])
                self.assertEqual(result["after"]["components"][0]["x_mm"], 86.2 if args.host_apply else 35.4)
                moves = 3 if args.host_apply else 1
                self.assertEqual(self.board.calls, ["begin", "update", "commit"] * moves)
                self.assertEqual(count.count("BeginCommit"), moves)
                self.assertEqual(count.count("EndCommit"), moves)
                self.assertEqual(count.count("UpdateItems"), moves)
            finally:
                stop.set()
                thread.join(3)
            self.assertFalse(thread.is_alive())
            if not failures.empty():
                raise failures.get()

    def test_public_run_refuses_wrong_native_version_instance_or_document(self):
        board = self.board
        board.document.project.path = "C:/owned"
        original_client = worker.BoundClient
        original_endpoint = os.environ.get("KICAD_API_SOCKET")
        os.environ["KICAD_API_SOCKET"] = "ipc://C:/owned/api.sock"
        try:
            for fault in ("wrong_version", "missing_instance", "different_project", "duplicate_document"):
                class FaultClient:
                    def __init__(self, *args):
                        self._kicad_token = "" if fault == "missing_instance" else "owned-fixture-token"
                        self.connected = False

                    def send(self, command, response_type):
                        reply = native_reply(board, command, response_type)
                        if command.DESCRIPTOR.name == "GetVersion" and fault == "wrong_version":
                            reply.version.patch = 6
                        if command.DESCRIPTOR.name == "GetOpenDocuments":
                            if fault == "different_project":
                                reply.documents[0].project.path = "C:/other"
                            if fault == "duplicate_document":
                                reply.documents.append(reply.documents[0])
                        return reply
                worker.BoundClient = FaultClient
                with self.subTest(fault=fault), self.assertRaises(worker.Refused):
                    worker.run({"action":"inspect","job_id":str(uuid.uuid4()),
                                "board_path":"C:/owned/OwnedBoard.kicad_pcb"})
            self.assertEqual(board.calls, [])
        finally:
            worker.BoundClient = original_client
            if original_endpoint is None:
                os.environ.pop("KICAD_API_SOCKET", None)
            else:
                os.environ["KICAD_API_SOCKET"] = original_endpoint

    def test_public_inspection_and_move_use_real_sdk_board_and_version_wrappers(self):
        board = self.board
        board.document.project.path = "C:/owned"

        class WireClient:
            def __init__(self, *args):
                self._kicad_token = "owned-fixture-token"
                self.connected = False

            def send(self, command, response_type):
                return native_reply(board, command, response_type)

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


class NativeExport(unittest.TestCase):
    def test_export_serializes_current_native_state_without_save_or_commit(self):
        board = Board()
        current, text, _ = worker.observation(board, "a" * 64)
        with tempfile.TemporaryDirectory(dir=Path(__file__).resolve().parents[2] / "target") as directory:
            path = Path(directory) / "Board.kicad_pcb"
            request = {**current, "board_path": "C:/owned/Original.kicad_pcb", "destination": str(path)}
            result = worker.export_copy(board, "a" * 64, request)
            self.assertEqual(path.read_bytes(), text.encode("utf-8"))
            self.assertEqual(result["exported_sha256"], current["state_sha256"])
            self.assertEqual(result["after"], current)
            self.assertFalse(result["saved"] or result["native_commit_confirmed"])
            self.assertEqual(board.calls, [])
            with self.assertRaises(FileExistsError):
                worker.export_copy(board, "a" * 64, request)
            self.assertEqual(path.read_bytes(), text.encode("utf-8"))

    def test_stale_and_invalid_destinations_never_create_an_export(self):
        board = Board()
        current, _, _ = worker.observation(board, "a" * 64)
        with tempfile.TemporaryDirectory(dir=Path(__file__).resolve().parents[2] / "target") as directory:
            path = Path(directory) / "Board.kicad_pcb"
            request = {**current, "board_path": "C:/owned/Original.kicad_pcb", "destination": str(path)}
            with self.assertRaises(worker.Refused):
                worker.export_copy(board, "b" * 64, request)
            for destination in ["Board.kicad_pcb", str(path.with_name("Other.kicad_pcb")), request["board_path"]]:
                with self.assertRaises(worker.Refused):
                    worker.export_copy(board, "a" * 64, {**request, "destination": destination})
            self.assertFalse(path.exists())
            self.assertEqual(board.calls, [])

    def test_changed_or_lost_readback_is_uncertain_without_repeating_export(self):
        board = Board()
        current, text, _ = worker.observation(board, "a" * 64)
        for final in [({**current, "state_sha256": "b" * 64}, text, []), ConnectionError("private")]:
            with tempfile.TemporaryDirectory(dir=Path(__file__).resolve().parents[2] / "target") as directory:
                path = Path(directory) / "Board.kicad_pcb"
                request = {**current, "board_path": "C:/owned/Original.kicad_pcb", "destination": str(path)}
                with patch.object(worker, "observation", side_effect=[(current, text, []), final]) as read:
                    with self.assertRaises(worker.Uncertain):
                        worker.export_copy(board, "a" * 64, request)
                    self.assertEqual(read.call_count, 2)
                self.assertEqual(path.read_bytes(), text.encode("utf-8"))
                self.assertEqual(board.calls, [])


unittest.main(argv=[sys.argv[0]])
