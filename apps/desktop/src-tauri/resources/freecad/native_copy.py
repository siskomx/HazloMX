"""Native working-copy capture and headless verification; never run document code.

Call capture on the native GUI thread with the live-state reader. Call verify
only in a separate FreeCADCmd process with an isolated profile and this policy;
the host must pin the input and its directory lineage against replacement and
write for the entire verification, as with existing native-copy adapters.
Paths are host-owned outputs, never model-supplied native arguments.
"""
import hashlib
import io
import json
import math
from pathlib import Path
import re
import stat
import zipfile

from cadpilot_policy import NATIVE_TYPES, PROPERTY_TYPES, document

LIMIT = 32 * 1024 * 1024


def _digest(data):
    return hashlib.sha256(data).hexdigest()


def _file(path):
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400 or path.is_symlink() or not 0 < info.st_size <= LIMIT:
        raise ValueError("working_copy_invalid")
    data = path.read_bytes()
    current = path.lstat()
    identity = lambda value: (value.st_dev, value.st_ino, value.st_size, value.st_mtime_ns, value.st_mode)
    if len(data) != info.st_size or identity(current) != identity(info):
        raise ValueError("working_copy_changed")
    return {"name": path.name, "bytes": len(data), "sha256": _digest(data)}


def _property(obj, name):
    data = bytes(obj.dumpPropertyContent(name))
    if len(data) > LIMIT:
        raise ValueError("working_copy_limit")
    # The native dump is a ZIP; its timestamps are not part of property identity.
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        entries = archive.infolist()
        if len(entries) > 64 or sum(item.file_size for item in entries) > LIMIT:
            raise ValueError("working_copy_limit")
        return sorted([item.filename, _digest(archive.read(item))] for item in entries)


def facts(doc):
    if not 0 < len(doc.Objects) <= 32:
        raise ValueError("working_copy_unsupported")
    result = {}
    for obj in doc.Objects:
        if obj.TypeId not in NATIVE_TYPES:
            raise ValueError("working_copy_unsupported")
        props = {}
        for name in obj.PropertiesList:
            kind = obj.getTypeIdOfProperty(name)
            if kind not in PROPERTY_TYPES:
                raise ValueError("working_copy_unsupported")
            if kind != "Part::PropertyPartShape":
                props[name] = _property(obj, name)
        shape = obj.Shape
        if shape.isNull() or not shape.isValid() or not shape.Solids:
            raise ValueError("working_copy_invalid_geometry")
        bbox = shape.BoundBox
        center = shape.CenterOfMass
        measures = [shape.Volume, bbox.XMin, bbox.YMin, bbox.ZMin,
                    bbox.XMax, bbox.YMax, bbox.ZMax, center.x, center.y, center.z]
        if any(not math.isfinite(v) for v in measures) or measures[0] <= 0:
            raise ValueError("working_copy_invalid_geometry")
        result[obj.Name] = {"type": obj.TypeId, "properties": props,
                            "brep_sha256": _digest(shape.exportBrepToString().encode()),
                            "solids": len(shape.Solids), "measures_mm": measures}
    return {"label": doc.Label, "objects": result, "units": "mm",
            "coordinate_frame": "document_world"}


def capture(doc, destination, state):
    """Serialize current live work once; retain partial files on every failure."""
    path = Path(destination)
    device = path.name.split(".", 1)[0].upper()
    if (str(path).startswith("\\\\") or len(path.name.encode("utf-16-le")) > 510
            or any(ord(c) < 32 or c in '<>:"/\\|?*' for c in path.name)
            or device in {"CON", "PRN", "AUX", "NUL", "CLOCK$", "CONIN$", "CONOUT$"}
            or re.fullmatch(r"(?:COM|LPT)[0-9¹²³]", device)):
        raise ValueError("working_copy_destination_invalid")
    if not path.is_absolute() or path.suffix.lower() != ".fcstd" or path.exists() or path.is_symlink():
        raise ValueError("working_copy_destination_exists")
    if path.stem != doc.Label:
        raise ValueError("working_copy_label_requires_approval")
    before = json.loads(json.dumps(state(doc), allow_nan=False))
    expected = facts(doc)
    if doc.HasPendingTransaction:
        raise ValueError("working_copy_transaction_pending")
    doc.saveCopy(str(path))
    saved = _file(path)
    if state(doc) != before or facts(doc) != expected:
        raise ValueError("working_copy_source_changed")
    return {"version": 1, "file": saved, "expected": expected,
            "source_preserved": True, "reopen_checked": False}


def verify(path, captured):
    """Reject unsupported content before native deserialization in FreeCADCmd."""
    import FreeCAD as App
    if App.GuiUp or App.listDocuments() or ".".join(App.Version()[:3]) != "1.1.4":
        raise ValueError("working_copy_verifier_runtime")
    path = Path(path)
    if captured.get("version") != 1 or captured.get("source_preserved") is not True or _file(path) != captured["file"]:
        raise ValueError("working_copy_changed")
    # The headless process never restores GUI state. Preserve those archive bytes,
    # but do not interpret or certify them as a rendered/view-state check.
    catalog = document(path, headless=True)
    before = _file(path)
    opened = App.openDocument(str(path))
    try:
        cached = facts(opened)
        for obj in opened.Objects:
            obj.touch()
        opened.recompute()
        actual = facts(opened)
        if set(actual["objects"]) != set(catalog) or not _matches(actual, captured["expected"]) or not _matches(cached, captured["expected"]):
            raise ValueError("working_copy_reopen_mismatch")
    finally:
        App.closeDocument(opened.Name)
    if _file(path) != before:
        raise ValueError("working_copy_changed")
    with zipfile.ZipFile(path) as archive:
        resources = [{"path": item.filename, "bytes": item.file_size,
                      "sha256": _digest(archive.read(item))} for item in archive.infolist()]
    return {"version": 1, "file": before, "source_preserved": True,
            "reopen_checked": True, "runtime_version": "1.1.4", "headless": True,
            "native_properties_checked": True, "native_geometry_checked": True,
            "native_primitives_recomputed": True,
            "cached_brep_bytes_equal": all(cached["objects"][key]["brep_sha256"] == value["brep_sha256"] for key, value in captured["expected"]["objects"].items()),
            "geometry_tolerance": {"linear_mm": 1e-7, "volume_relative": 1e-10, "volume_absolute_mm3": 1e-7},
            "units": "mm", "coordinate_frame": "document_world",
            "dependency_scope": "self_contained_native_primitives",
            "external_dependencies": [], "embedded_resources": resources,
            "gui_state_checked": False, "physical_function_checked": False}


def _matches(actual, expected):
    if actual["label"] != expected["label"] or actual["units"] != expected["units"] or actual["coordinate_frame"] != expected["coordinate_frame"] or set(actual["objects"]) != set(expected["objects"]):
        return False
    for key, after in actual["objects"].items():
        before = expected["objects"][key]
        if any(after[field] != before[field] for field in ["type", "properties", "solids"]):
            return False
        # Native primitive properties must match exactly. Kernel BREP persistence
        # can round transformed geometry; compare its measures at stated tolerances.
        if len(after["measures_mm"]) != 10 or len(before["measures_mm"]) != 10:
            return False
        for index, (a, b) in enumerate(zip(after["measures_mm"], before["measures_mm"])):
            if not math.isclose(a, b, rel_tol=1e-10 if index == 0 else 0, abs_tol=1e-7):
                return False
    return True
