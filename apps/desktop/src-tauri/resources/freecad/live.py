"""Fixed Hazlo glue, dispatched to FreeCAD's GUI thread by CADPilot.

The host supplies JSON data in HAZLO_REQUEST, never model-written Python.
"""
import hashlib
import json
import time
import uuid

import FreeCAD as App
import FreeCADGui as Gui
from rpc_server import rpc_server as rpc
from rpc_server.serialize import serialize_object


def _digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()).hexdigest()


def _state(doc):
    if len(doc.Objects) > 256:
        raise ValueError("model_limit")
    objects = [serialize_object(obj) for obj in doc.Objects]
    for obj, item in zip(doc.Objects, objects):
        # Shape reprs can contain wrapper addresses; fingerprint the actual BREP below instead.
        item["Properties"].pop("Shape", None)
        for prop in item["Properties"]:
            # FreeCAD's material repr contains a fresh wrapper address on every read.
            if "Material" in obj.getTypeIdOfProperty(prop):
                content = bytes(obj.dumpPropertyContent(prop))
                if len(content) > 1024 * 1024:
                    raise ValueError("model_limit")
                item["Properties"][prop] = {"native_sha256": hashlib.sha256(content).hexdigest()}
    shapes = []
    for obj in doc.Objects:
        shape = getattr(obj, "Shape", None)
        if shape is not None and not shape.isNull():
            brep = shape.exportBrepToString().encode()
            if len(brep) > 16 * 1024 * 1024:
                raise ValueError("model_limit")
            shapes.append([obj.Name, hashlib.sha256(brep).hexdigest()])
    fingerprint = _digest({"objects": objects, "shapes": shapes, "file": doc.FileName, "label": doc.Label})
    # The server's async task output is capped at 64 KiB. Do not return a truncated observation.
    if len(json.dumps(objects, ensure_ascii=False).encode()) > 48000:
        raise ValueError("model_limit")
    return {"document_name": doc.Name, "state_sha256": fingerprint, "objects": objects}


def _run(request):
    if not hasattr(rpc, "_hazlomexa_live"):
        rpc._hazlomexa_live = {"documents": {}, "receipts": {}}
    registry = rpc._hazlomexa_live
    documents = registry["documents"]
    opened = App.listDocuments()
    # Retain wrappers to distinguish a close/reopen even when FreeCAD reuses the document name.
    for token, bound in list(documents.items()):
        if opened.get(bound[0]) is not bound[1]:
            del documents[token]
    action = request["action"]
    if action == "inspect":
        doc = opened.get(request["document_name"])
        if doc is None:
            raise ValueError("closed_document")
        token = next((key for key, value in documents.items() if value[1] is doc), None)
        if token is None:
            if len(documents) >= 32:
                raise ValueError("document_limit")
            token = str(uuid.uuid4())
            documents[token] = (doc.Name, doc)
        result = _state(doc)
        result["document_token"] = token
        result["recent_attempts"] = dict([(job, entry) for job, entry in registry["receipts"].items() if entry["document_token"] == token][-8:])
        return result
    if action != "apply":
        raise ValueError("invalid_action")
    bound = documents.get(request["document_token"])
    if bound is None:
        raise ValueError("stale_document")
    doc = bound[1]
    before = _state(doc)
    if before["state_sha256"] != request["state_sha256"]:
        raise ValueError("changed_document")
    if doc.HasPendingTransaction or doc.UndoMode != 1:
        raise ValueError("native_undo_unavailable")
    # Do not edit a hidden tab while the user is looking at a different document.
    if Gui.activeDocument() is None or Gui.activeDocument().Document is not doc:
        raise ValueError("document_not_visible")
    if time.time() > request["expires_at"]:
        raise ValueError("expired_request")
    receipts = registry["receipts"]
    job = request["job_id"]
    if job in receipts:
        raise ValueError("already_attempted")
    if len(receipts) >= 128:
        raise ValueError("receipt_limit")
    receipts[job] = {"status": "running", "document_token": request["document_token"]}
    transaction = "Hazlo Mexa: " + job
    doc.openTransaction(transaction)
    try:
        results = []
        for operation in request["operations"]:
            result = rpc.FreeCADRPC()._run_one_operation(doc.Name, operation)
            if not result.get("success"):
                raise ValueError("native_operation_failed")
            results.append({key: result[key] for key in ("success", "action", "object_name") if key in result})
        doc.recompute()
        if any("Invalid" in [str(state) for state in obj.State] for obj in doc.Objects):
            raise ValueError("invalid_native_result")
        after = _state(doc)
        doc.commitTransaction()
    except BaseException:
        doc.abortTransaction()
        restored = _state(doc)["state_sha256"] == before["state_sha256"]
        receipts[job] = {"status": "failed", "rollback_verified": restored, "document_token": request["document_token"]}
        raise ValueError("native_operation_rolled_back" if restored else "rollback_uncertain") from None
    undo_available = after["state_sha256"] == before["state_sha256"] or transaction in doc.UndoNames
    receipts[job] = {"status": "completed", "before_sha256": before["state_sha256"], "after_sha256": after["state_sha256"], "document_token": request["document_token"], "undo_available": undo_available}
    Gui.getDocument(doc.Name).activeView().redraw()
    return {"job_id": job, "document_token": request["document_token"], "before_sha256": before["state_sha256"], "after": after, "operations": results, "undo_available": undo_available}


def _dispatch():
    try:
        return {"ok": True, "result": _run(HAZLO_REQUEST)}
    except Exception as error:
        # Do not forward native tracebacks, paths or document contents in failures.
        safe = {"model_limit", "closed_document", "document_limit", "stale_document", "changed_document", "native_undo_unavailable", "document_not_visible", "expired_request", "already_attempted", "receipt_limit", "native_operation_rolled_back", "rollback_uncertain"}
        return {"ok": False, "error": str(error) if str(error) in safe else "native_operation_uncertain"}


task_print(json.dumps(rpc.dispatch_to_gui(_dispatch), ensure_ascii=False, allow_nan=False))
