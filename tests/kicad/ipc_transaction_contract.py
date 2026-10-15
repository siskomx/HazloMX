"""Probe the pinned official client against KiCad 10.0.7's wire contract, not an editor."""
import argparse
import importlib.metadata
import queue
import sys
import threading
import uuid

args = argparse.ArgumentParser()
args.add_argument("--site", required=True, help="Isolated candidate client installation")
sys.path.insert(0, args.parse_args().site)

import pynng
from google.protobuf import descriptor_pb2, descriptor_pool, message_factory
from kipy.client import KiCadClient
from kipy.errors import ConnectionError
from kipy.proto.common import ApiRequest, ApiResponse, ApiStatusCode
from kipy.proto.common.commands import BeginCommit, BeginCommitResponse, EndCommit, EndCommitResponse, GetItems, GetItemsResponse
from kipy.proto.common.types import DocumentSpecifier, DocumentType, ItemHeader

assert importlib.metadata.version("kicad-python") == "0.8.0"


from pathlib import Path
import importlib.util
worker_path = Path(__file__).resolve().parents[2] / "apps/desktop/src-tauri/resources/kicad/live.py"
spec = importlib.util.spec_from_file_location("hazlo_kicad_native", worker_path)
worker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(worker)
document_command = worker.document_command
BoundClient = worker.BoundClient


# Independent native contract parser: the two field numbers come from KiCad 10.0.7,
# not from the old client's descriptors or the compatibility function above.
contract = descriptor_pb2.FileDescriptorProto(name="hazlo_native_commit_test.proto", package="hazlo.test")
contract.dependency.append(ItemHeader.DESCRIPTOR.file.name)
for name, number in [("NativeBegin", 1), ("NativeEnd", 4)]:
    message = contract.message_type.add(name=name)
    message.field.add(name="header", number=number, type=11, label=1,
                      type_name=".kiapi.common.types.ItemHeader")
descriptor_pool.Default().AddSerializedFile(contract.SerializeToString())
native_begin = message_factory.GetMessageClass(descriptor_pool.Default().FindMessageTypeByName("hazlo.test.NativeBegin"))
native_end = message_factory.GetMessageClass(descriptor_pool.Default().FindMessageTypeByName("hazlo.test.NativeEnd"))
document = DocumentSpecifier(type=DocumentType.DOCTYPE_PCB, board_filename="OwnedBoard.kicad_pcb")
document.project.name = "owned-fixture"
document.project.path = "C:/owned/" + "nested/" * 40

for command, parser in [(BeginCommit(), native_begin), (EndCommit(), native_end)]:
    payload = document_command(command, document).SerializeToString()
    assert parser.FromString(payload).header.document == document
    # The old generated class must retain the header through another parse/serialize.
    assert parser.FromString(type(command).FromString(payload).SerializeToString()).header.document == document
try:
    document_command(BeginCommit(), DocumentSpecifier())
except worker.Refused:
    pass
else:
    raise AssertionError("Missing target document was accepted")


def exchange(timeout=False, foreign=False, missing_header=False):
    items = foreign or missing_header
    endpoint = "inproc://hazlo-commit-" + str(uuid.uuid4())
    errors = queue.Queue()
    received = []
    with pynng.Rep0(listen=endpoint, recv_timeout=1000, send_timeout=1000) as server:
        def serve():
            try:
                for index in range(1 if timeout or items else 2):
                    request = ApiRequest.FromString(server.recv())
                    received.append(request)
                    parser = native_begin if index == 0 else native_end
                    assert (GetItems.FromString(request.message.value) if items else parser.FromString(request.message.value)).header.document == document
                    assert request.message.type_url.endswith("GetItems" if items else "BeginCommit" if index == 0 else "EndCommit")
                    assert request.header.kicad_token == ("" if index == 0 else "owned-fixture-token")
                    if index:
                        end = EndCommit.FromString(request.message.value)
                        assert end.id.value == "owned-commit" and end.action == 2
                    if timeout:
                        return
                    reply = ApiResponse()
                    reply.status.status = ApiStatusCode.AS_OK
                    reply.header.kicad_token = "owned-fixture-token"
                    result = GetItemsResponse() if items else BeginCommitResponse() if index == 0 else EndCommitResponse()
                    if foreign:
                        result.header.document.CopyFrom(DocumentSpecifier(type=DocumentType.DOCTYPE_PCB, board_filename="OtherBoard.kicad_pcb"))
                        result.status = 1
                    elif missing_header:
                        result.status = 1
                    elif index == 0:
                        result.id.value = "owned-commit"
                    reply.message.Pack(result)
                    server.send(reply.SerializeToString())
            except BaseException as error:
                errors.put(error)

        server_thread = threading.Thread(target=serve)
        server_thread.start()
        client = BoundClient(endpoint, "hazlo-contract-test", "", 150 if timeout else 1000)
        try:
            if foreign:
                try:
                    client.send(GetItems(header=ItemHeader(document=document)), GetItemsResponse)
                    raise AssertionError("Foreign native document reply was accepted")
                except worker.Uncertain:
                    pass
            elif missing_header:
                result = client.send(GetItems(header=ItemHeader(document=document)), GetItemsResponse)
                assert not result.HasField("header")
            elif timeout:
                try:
                    client.send(document_command(BeginCommit(), document), BeginCommitResponse)
                    raise AssertionError("Missing native reply was accepted")
                except ConnectionError:
                    pass
            else:
                commit = client.send(document_command(BeginCommit(), document), BeginCommitResponse)
                end = EndCommit(action=2, message="owned mock rollback")
                end.id.CopyFrom(commit.id)
                client.send(document_command(end, document), EndCommitResponse)
            assert client._conn.resend_time == -1
            assert client._conn.recv_max_size == 8 * 1024 * 1024
        finally:
            if client.connected:
                client._conn.close()
            server_thread.join(2)
        assert not server_thread.is_alive()
        if not errors.empty():
            raise errors.get()
        assert len(received) == (1 if timeout or items else 2)


exchange()
exchange(timeout=True)
exchange(foreign=True)
exchange(missing_header=True)
print("PASS: official-client protobuf preserves native document headers; NNG transaction exchange, token binding, foreign-document and reply-timeout refusal; no editor contacted.")
