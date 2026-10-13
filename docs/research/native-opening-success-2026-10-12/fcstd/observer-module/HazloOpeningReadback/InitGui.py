with open("C:/Users/USER\\AppData\\Local\\Temp\\Hazlo opening á-l30jV5\\observer-gui-ready.txt", 'w', encoding='utf-8') as marker:
    marker.write('gui observer loaded')
import __main__, json
import FreeCAD as App
import FreeCADGui as Gui
from PySide import QtCore
__main__.hazloOpeningTicks = 0
def captureHazloOpening():
    import __main__, json, FreeCAD as App
    __main__.hazloOpeningTicks += 1
    doc = App.ActiveDocument
    shapes = []
    if doc:
        for obj in doc.Objects:
            if hasattr(obj, 'Shape') and not obj.Shape.isNull():
                shape = obj.Shape
                box = shape.BoundBox
                shapes.append({'name': obj.Name, 'type': obj.TypeId, 'size': [box.XLength, box.YLength, box.ZLength], 'volume': shape.Volume, 'solids': len(shape.Solids), 'valid': shape.isValid()})
    if shapes or __main__.hazloOpeningTicks >= 400:
        result = {'document': doc.Name if doc else None, 'label': doc.Label if doc else None, 'filename': doc.FileName if doc else None, 'shapes': shapes, 'user_config': App.ConfigGet('UserConfigPath'), 'user_data': App.ConfigGet('UserAppData')}
        with open("C:/Users/USER\\AppData\\Local\\Temp\\Hazlo opening á-l30jV5\\gui-readback.json", 'w', encoding='utf-8') as output:
            json.dump(result, output, ensure_ascii=False)
        __main__.hazloOpeningTimer.stop()
__main__.hazloOpeningTimer = QtCore.QTimer(Gui.getMainWindow())
__main__.hazloOpeningTimer.timeout.connect(captureHazloOpening)
__main__.hazloOpeningTimer.start(100)
