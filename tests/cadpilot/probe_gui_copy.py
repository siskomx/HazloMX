"""Owned offscreen native API fixture, launched only by the opt-in Rust test."""
import json
import os
from pathlib import Path
import FreeCAD as App
import FreeCADGui as Gui
from PySide import QtCore
from native_copy import capture, facts

root = Path.cwd()
assert os.environ.get("HAZLO_OWNED_COPY") == root.name
assert not App.listDocuments()
doc = App.newDocument("OwnedCopy")
try:
    doc.UndoMode = 1
    obj = doc.addObject("Part::Box", "Box")
    obj.Length = 25.4
    obj.Width = 12
    obj.Height = 8
    doc.recompute()
    doc.saveAs(str(root / "Original.FCStd"))
    original = (root / "Original.FCStd").read_bytes()
    doc.openTransaction("Owned manual change")
    obj.Width = 22.25
    doc.Label = "Owned manual label"
    doc.recompute()
    doc.commitTransaction()

    def state(document):
        return {"file": document.FileName, "label": document.Label,
                "modified": bool(Gui.getDocument(document.Name).Modified),
                "undo": list(document.UndoNames), "facts": facts(document)}

    before = state(doc)
    assert before["modified"] and before["undo"]
    (root / "NativeVersion").mkdir()
    saved = capture(doc, root / "NativeVersion" / (doc.Label + ".FCStd"), state)
    assert state(doc) == before
    assert (root / "Original.FCStd").read_bytes() == original
    (root / "NativeVersion" / "capture.json").write_text(json.dumps(saved), encoding="utf-8")
    (root / "gui-proof.json").write_text(json.dumps({
        "modified_preserved": True, "undo_preserved": True,
        "original_preserved": True, "native_state_preserved": True,
        "visual_check": False}), encoding="utf-8")
finally:
    # Only this newly created document belongs to the disposable process.
    Gui.getDocument(doc.Name).Modified = False
    App.closeDocument(doc.Name)
    QtCore.QTimer.singleShot(0, Gui.getMainWindow().close)
