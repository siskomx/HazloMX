"""Fixed Hazlo glue, dispatched to FreeCAD's GUI thread by CADPilot.

The host supplies JSON data in HAZLO_REQUEST, never model-written Python.
"""
import hashlib
import io
import json
import os
import stat
import time
import uuid
import zipfile

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
                # Native property dumps are ZIPs whose timestamps change without a model edit.
                with zipfile.ZipFile(io.BytesIO(content)) as archive:
                    entries = archive.infolist()
                    if len(entries) > 32 or sum(entry.file_size for entry in entries) > 1024 * 1024:
                        raise ValueError("model_limit")
                    persisted = sorted((entry.filename, hashlib.sha256(archive.read(entry)).hexdigest()) for entry in entries)
                item["Properties"][prop] = {"native_sha256": _digest(persisted)}
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
    return {"document_name": doc.Name, "state_sha256": fingerprint, "objects": objects, "has_saved_file": bool(doc.FileName), "modified": bool(getattr(Gui.getDocument(doc.Name), "Modified", True))}


def _saved_file(doc):
    # The destination comes only from FreeCAD's human-owned Save As dialog.
    path = doc.FileName
    before = os.stat(path, follow_symlinks=False)
    if not path.lower().endswith(".fcstd") or not stat.S_ISREG(before.st_mode) or not 0 < before.st_size <= 256 * 1024 * 1024:
        raise ValueError("native_save_uncertain")
    digest = hashlib.sha256()
    total = 0
    with open(path, "rb") as saved:
        for chunk in iter(lambda: saved.read(1024 * 1024), b""):
            total += len(chunk)
            if total > 256 * 1024 * 1024:
                raise ValueError("native_save_uncertain")
            digest.update(chunk)
        after = os.fstat(saved.fileno())
    current = os.stat(path, follow_symlinks=False)
    if total != before.st_size or any((item.st_ino, item.st_size, item.st_mtime_ns) != (before.st_ino, before.st_size, before.st_mtime_ns) for item in (after, current)):
        raise ValueError("native_save_uncertain")
    return {"name": os.path.basename(path), "bytes": before.st_size, "sha256": digest.hexdigest()}


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
    saving = request["operations"] == [{"action": "save_as"}]
    if doc.HasPendingTransaction or (not saving and doc.UndoMode != 1):
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
    if any(entry["status"] == "running" and entry["document_token"] == request["document_token"] for entry in receipts.values()):
        raise ValueError("native_operation_pending")
    if len(receipts) >= 128:
        raise ValueError("receipt_limit")
    receipts[job] = {"status": "running", "document_token": request["document_token"]}
    if saving:
        try:
            if not Gui.getDocument(doc.Name).saveAs():
                receipts[job] = {"status": "not_completed", "document_token": request["document_token"]}
                raise ValueError("native_save_not_completed")
            after = _state(doc)
            saved = _saved_file(doc)
            if after["modified"] or before["objects"] != after["objects"]:
                raise ValueError("native_save_uncertain")
            receipts[job] = {"status": "completed", "document_token": request["document_token"], "before_sha256": before["state_sha256"], "after_sha256": after["state_sha256"], "saved_document": saved}
            return {"job_id": job, "document_token": request["document_token"], "before_sha256": before["state_sha256"], "after": after, "operations": [{"action": "save_as", "success": True}], "undo_available": False, "saved_document": saved}
        except BaseException:
            if receipts[job]["status"] == "running":
                receipts[job]["status"] = "uncertain"
            raise
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
        safe = {"model_limit", "closed_document", "document_limit", "stale_document", "changed_document", "native_undo_unavailable", "document_not_visible", "expired_request", "already_attempted", "receipt_limit", "native_operation_rolled_back", "rollback_uncertain", "native_save_not_completed", "native_save_uncertain", "native_operation_pending"}
        return {"ok": False, "error": str(error) if str(error) in safe else "native_operation_uncertain"}


task_print(json.dumps(rpc.dispatch_to_gui(_dispatch, timeout=600 if HAZLO_REQUEST.get("operations") == [{"action": "save_as"}] else 60), ensure_ascii=False, allow_nan=False))
