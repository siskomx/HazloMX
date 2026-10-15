"""Internal native PCB worker. Host approval and runtime identity checks remain required."""
import hashlib
import importlib.metadata
import json
import math
import ntpath
import os
import subprocess
import sys
import uuid

from kipy import KiCad
from kipy.board import Board
from kipy.client import KiCadClient
from kipy.errors import ConnectionError
from kipy.geometry import Vector2
from kipy.proto.common.commands import (
    BeginCommit, BeginCommitResponse, EndCommit, EndCommitResponse,
    SaveDocumentToString, SavedDocumentResponse,
)
from kipy.proto.common.types import DocumentType, ItemHeader, LockedState

LIMIT = 2 * 1024 * 1024


class Refused(Exception):
    pass


class Uncertain(Exception):
    pass


def digest(data):
    return hashlib.sha256(data).hexdigest()


def document_command(command, document):
    # The pinned SDK preserves these 10.0.7 fields even though its descriptors predate them.
    number = {"kiapi.common.commands.BeginCommit": 1, "kiapi.common.commands.EndCommit": 4}[command.DESCRIPTOR.full_name]
    if document.type != DocumentType.DOCTYPE_PCB or not document.board_filename:
        raise Refused("missing_document")
    data = ItemHeader(document=document).SerializeToString()
    length, size = bytearray(), len(data)
    while size > 127:
        length.append((size & 127) | 128)
        size >>= 7
    length.append(size)
    command.MergeFromString(bytes([number * 8 + 2]) + length + data)
    return command


class BoundClient(KiCadClient):
    def _connect(self):
        super()._connect()
        self._conn.resend_time = -1
        self._conn.recv_max_size = 8 * 1024 * 1024

    def send(self, command, response_type):
        reply = super().send(command, response_type)
        if ("header" in command.DESCRIPTOR.fields_by_name
                and "header" in reply.DESCRIPTOR.fields_by_name
                # KiCad 10.0.7 omits headers on successful item replies.
                and reply.HasField("header")
                and command.header.HasField("document")
                and reply.header.document != command.header.document):
            raise Uncertain("foreign_document_reply")
        return reply


def board_snapshot(board):
    reply = board.client.send(SaveDocumentToString(document=board.document), SavedDocumentResponse)
    if reply.document != board.document or not reply.contents or len(reply.contents.encode()) > LIMIT:
        raise Refused("invalid_snapshot")
    return reply.contents


def observation(board, instance):
    before = board_snapshot(board)
    footprints = board.get_footprints()
    if len(footprints) > 256:
        raise Refused("model_limit")
    components = []
    identifiers = set()
    references = set()
    for item in footprints:
        identifier = str(uuid.UUID(item.id.value))
        reference = item.reference_field.text.value
        value = item.value_field.text.value
        if identifier in identifiers or reference in references or not reference:
            raise Refused("ambiguous_component")
        if any(len(text) > 1024 or any(ord(c) < 32 for c in text) for text in (reference, value)):
            raise Refused("invalid_component")
        identifiers.add(identifier)
        references.add(reference)
        x, y = item.position.x / 1_000_000, item.position.y / 1_000_000
        if not all(math.isfinite(v) and abs(v) <= 10000 for v in (x, y)):
            raise Refused("invalid_component")
        components.append({"uuid": identifier, "reference": reference, "value": value, "x_mm": x, "y_mm": y, "locked": item.locked})
    if board_snapshot(board) != before:
        raise Refused("changed_document")
    result = {"instance_sha256": instance,
            "document_sha256": digest(board.document.SerializeToString(deterministic=True)),
            "state_sha256": digest(before.encode()), "components": components, "source_is_untrusted_data": True}
    if len(json.dumps(result).encode()) > 48000:
        raise Refused("model_limit")
    return result, before, footprints


def verify_with_host(before, after, movement):
    executable = os.environ.get("HAZLOMEXA_PCB_VERIFIER", "")
    if not os.path.isabs(executable) or not os.path.isfile(executable):
        raise Refused("missing_verifier")
    payload = {"before": before, "movement": movement}
    if after is not None:
        payload["after"] = after
    result = subprocess.run([executable, "--validate-pcb-move" if after is None else "--verify-pcb-move"],
        input=json.dumps(payload).encode(),
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=15,
        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0), check=False)
    return result.returncode == 0 and result.stdout.strip() == b'{"valid":true}'


def apply_move(board, instance, request, verify):
    current, before, footprints = observation(board, instance)
    if any(request.get(key) != current[key] for key in ("instance_sha256", "document_sha256", "state_sha256")):
        raise Refused("changed_document")
    movement = request["movement"]
    if set(movement) != {"uuid", "reference", "expected_x_mm", "expected_y_mm", "x_mm", "y_mm"}:
        raise Refused("invalid_move")
    target = next((f for f in footprints if f.id.value == movement["uuid"] and f.reference_field.text.value == movement["reference"]), None)
    if target is None or target.proto.locked != LockedState.LS_UNLOCKED:
        raise Refused("missing_or_locked_component")
    if not verify(before, None, movement):
        raise Refused("invalid_move")
    clone = type(target)(target.proto)
    clone.position = Vector2.from_xy_mm(movement["x_mm"], movement["y_mm"])
    commit = None
    finishing = False
    try:
        commit = board.client.send(document_command(BeginCommit(), board.document), BeginCommitResponse).id
        if not commit.value:
            raise Uncertain("missing_commit")
        if board_snapshot(board) != before:
            raise Refused("changed_document")
        updated = board.update_items([clone])
        if len(updated) != 1 or updated[0].proto != clone.proto:
            raise Refused("native_move_mismatch")
        after = board_snapshot(board)
        try:
            verified = verify(before, after, movement)
        except Exception as error:
            raise Refused("verification_failed") from error
        if not verified:
            raise Refused("unapproved_change")
        if board_snapshot(board) != after:
            raise Uncertain("changed_before_commit")
        command = EndCommit(action=1, message="Hazlo Mexa approved component move")
        command.id.CopyFrom(commit)
        finishing = True
        board.client.send(document_command(command, board.document), EndCommitResponse)
    except ConnectionError as error:
        # An accepted write may still finish; never send a blind drop or retry after a lost reply.
        raise Uncertain("native_outcome_unknown") from error
    except Exception as error:
        confirmed = isinstance(error, Refused) and str(error) in {
            "changed_document", "native_move_mismatch", "unapproved_change", "verification_failed"}
        if finishing or not confirmed:
            raise Uncertain("native_outcome_unknown") from error
        if commit is not None:
            try:
                command = EndCommit(action=2)
                command.id.CopyFrom(commit)
                board.client.send(document_command(command, board.document), EndCommitResponse)
                if board_snapshot(board) != before:
                    raise Uncertain("rollback_mismatch")
            except Exception as rollback:
                raise Uncertain("rollback_unknown") from rollback
            return {"status": "rolled_back", "error": "native_move_refused"}
        raise
    try:
        final, final_snapshot, _ = observation(board, instance)
        if final_snapshot != after:
            raise Uncertain("changed_after_commit")
    except Exception as error:
        raise Uncertain("readback_unknown") from error
    return {"status": "completed", "before_sha256": current["state_sha256"], "after": final,
            "native_commit_confirmed": True, "saved": False}


def run(request):
    if importlib.metadata.version("kicad-python") != "0.8.0":
        raise Refused("unsupported_client")
    fields = {"action", "job_id", "board_path"}
    if request.get("action") == "move":
        fields |= {"instance_sha256", "document_sha256", "state_sha256", "movement"}
    if set(request) != fields or request["action"] not in {"inspect", "move"}:
        raise Refused("invalid_request")
    board_path = request["board_path"]
    if not ntpath.isabs(board_path) or len(ntpath.splitdrive(board_path)[0]) != 2 or len(board_path) > 4096 or any(ord(c) < 32 for c in board_path):
        raise Refused("invalid_document")
    job = uuid.UUID(request["job_id"])
    if not job.int:
        raise Refused("invalid_job")
    endpoint = os.environ["KICAD_API_SOCKET"]
    path = endpoint.removeprefix("ipc://")
    if not endpoint.startswith("ipc://") or not ntpath.isabs(path) or len(ntpath.splitdrive(path)[0]) != 2 or len(endpoint) > 1024 or any(ord(c) < 32 for c in endpoint):
        raise Refused("invalid_endpoint")
    token = os.environ.get("KICAD_API_TOKEN", "")
    if len(token) > 4096 or any(ord(c) < 32 for c in token):
        raise Refused("invalid_token")
    client = BoundClient(endpoint, "hazlo-" + str(job), token, 3000)
    api = KiCad.from_client(client)
    try:
        version = api.get_version()
        if (version.major, version.minor, version.patch) != (10, 0, 7) or not client._kicad_token:
            raise Refused("unsupported_editor")
        documents = api.get_open_documents(DocumentType.DOCTYPE_PCB)
        if len(documents) > 32:
            raise Refused("document_limit")
        selected = []
        for doc in documents:
            if doc.type != DocumentType.DOCTYPE_PCB:
                raise Refused("invalid_document")
            filename = doc.board_filename
            if not ntpath.isabs(filename):
                filename = ntpath.join(doc.project.path, filename)
            if ntpath.normcase(ntpath.normpath(filename)) == ntpath.normcase(ntpath.normpath(request["board_path"])):
                selected.append(doc)
        if len(selected) != 1:
            raise Refused("missing_document")
        board = Board(client, selected[0])
        instance = digest(client._kicad_token.encode())
        if request["action"] == "inspect":
            result, _, _ = observation(board, instance)
        else:
            result = apply_move(board, instance, request, verify_with_host)
        result["job_id"] = str(job)
        result["native_pcb_version"] = 1
        return result
    finally:
        if client.connected:
            try:
                client._conn.close()
            except Exception:
                pass


def unique_fields(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise Refused("duplicate_field")
        result[key] = value
    return result


def response(data):
    try:
        if len(data) > 131072:
            raise Refused("request_limit")
        native = run(json.loads(data, object_pairs_hook=unique_fields))
        result = ({"ok": False, "outcome": "rolled_back", "error": "native_move_refused"}
                  if native.get("status") == "rolled_back" else {"ok": True, "result": native})
    except Uncertain:
        result = {"ok": False, "outcome": "uncertain", "error": "native_outcome_unknown"}
    except Exception:
        result = {"ok": False, "outcome": "refused", "error": "native_request_refused"}
    return result


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-check"]:
        assert importlib.metadata.version("kicad-python") == "0.8.0"
        print(json.dumps({"native_pcb_version": 1, "client_version": "0.8.0"}))
    elif len(sys.argv) == 1:
        print(json.dumps(response(sys.stdin.buffer.read(131073)), ensure_ascii=False, allow_nan=False))
