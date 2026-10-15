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
from kipy.proto.common.commands import BeginCommit, BeginCommitResponse, EndCommit, EndCommitResponse
from kipy.proto.common.types import DocumentSpecifier, DocumentType, ItemHeader

assert importlib.metadata.version("kicad-python") == "0.8.0"


def document_command(command, document):
    """Preserve 10.0.7 header fields as protobuf unknown fields in the 0.8 client."""
    tags = {"kiapi.common.commands.BeginCommit": 1, "kiapi.common.commands.EndCommit": 4}
    number = tags[command.DESCRIPTOR.full_name]
    assert "header" not in command.DESCRIPTOR.fields_by_name
    assert document.type == DocumentType.DOCTYPE_PCB and document.board_filename
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
except AssertionError:
    pass
else:
    raise AssertionError("Missing target document was accepted")


def exchange(timeout=False):
    endpoint = "inproc://hazlo-commit-" + str(uuid.uuid4())
    errors = queue.Queue()
    received = []
    with pynng.Rep0(listen=endpoint, recv_timeout=1000, send_timeout=1000) as server:
        def serve():
            try:
                for index in range(1 if timeout else 2):
                    request = ApiRequest.FromString(server.recv())
                    received.append(request)
                    parser = native_begin if index == 0 else native_end
                    assert parser.FromString(request.message.value).header.document == document
                    assert request.message.type_url.endswith("BeginCommit" if index == 0 else "EndCommit")
                    assert request.header.kicad_token == ("" if index == 0 else "owned-fixture-token")
                    if index:
                        end = EndCommit.FromString(request.message.value)
                        assert end.id.value == "owned-commit" and end.action == 2
                    if timeout:
                        return
                    reply = ApiResponse()
                    reply.status.status = ApiStatusCode.AS_OK
                    reply.header.kicad_token = "owned-fixture-token"
                    result = BeginCommitResponse() if index == 0 else EndCommitResponse()
                    if index == 0:
                        result.id.value = "owned-commit"
                    reply.message.Pack(result)
                    server.send(reply.SerializeToString())
            except BaseException as error:
                errors.put(error)

        worker = threading.Thread(target=serve)
        worker.start()
        client = BoundClient(endpoint, "hazlo-contract-test", "", 150 if timeout else 1000)
        try:
            if timeout:
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
        finally:
            if client.connected:
                client._conn.close()
            worker.join(2)
        assert not worker.is_alive()
        if not errors.empty():
            raise errors.get()
        assert len(received) == (1 if timeout else 2)


exchange()
exchange(timeout=True)
print("PASS: official-client protobuf preserves native document headers; NNG transaction exchange, token binding and reply-timeout refusal; no editor contacted.")
