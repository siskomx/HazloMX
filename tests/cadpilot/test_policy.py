"""Trust-boundary checks use real native FCStd and adversarial request/archive variants."""
import importlib.util
import math
from pathlib import Path
import re
import shutil
import tempfile
from types import SimpleNamespace
import unittest
import uuid
import zipfile

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('cadpilot_policy', ROOT/'apps/desktop/src-tauri/resources/freecad/cadpilot_policy.py')
policy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(policy)
ORIGINAL = ROOT/'docs/research/cadpilot-primitives-2026-10-11/artifacts/Original.FCStd'


class Policy(unittest.TestCase):
    def test_headless_only_preserves_opaque_native_view_arrays(self):
        with zipfile.ZipFile(ORIGINAL) as archive:
            contents = {name: archive.read(name) for name in archive.namelist()}
        with tempfile.TemporaryDirectory(prefix='hazlo-headless-policy-') as directory:
            path = Path(directory) / 'copy.FCStd'
            for extra, allowed in [('LineColorArray', True), ('PointColorArray2', True),
                                   ('ShapeAppearance', True), ('ShapeAppearance.py', False),
                                   ('../LineColorArray', False), ('UnknownResource', False)]:
                with zipfile.ZipFile(path, 'w') as archive:
                    for name, data in dict(contents, **{extra:b'opaque view bytes'}).items():
                        archive.writestr(name, data)
                with self.assertRaises(ValueError):
                    policy.document(path)
                if allowed:
                    self.assertEqual(policy.document(path, headless=True), {'Target':'Part::Box'})
                else:
                    with self.assertRaises(ValueError):
                        policy.document(path, headless=True)

    def test_step_solids_match_individually_without_assuming_export_order(self):
        def solid(x=0, length=10, width=20, height=30):
            return SimpleNamespace(Volume=length*width*height,
                BoundBox=SimpleNamespace(XMin=x,YMin=0,ZMin=0,XMax=x+length,YMax=width,ZMax=height),
                CenterOfMass=SimpleNamespace(x=x+length/2,y=width/2,z=height/2))
        first, second = solid(), solid(x=100)
        policy.verify_step_solids([second, first], [first, second])
        policy.verify_step_solids([solid(), solid()], [solid(), solid()])
        policy.verify_step_solids([solid(x=0.0000001)], [first])
        for actual in [[solid(x=1)], [solid(length=20,width=10)], [],
                       [first, first], [solid(x=float('nan'))]]:
            with self.assertRaises(ValueError):
                policy.verify_step_solids(actual, [first] if len(actual) != 2 else [first, second])
        wrong_center = solid()
        wrong_center.CenterOfMass.x += 1
        with self.assertRaises(ValueError):
            policy.verify_step_solids([wrong_center], [first])

    def test_generated_visibility_frames_native_bounds_and_rejects_other_gui_state(self):
        catalog = {'Target': 'Part::Box'}
        reports = {'Target': {'bbox': dict(xmin=-10, xmax=0, ymin=40, ymax=60, zmin=5, zmax=35)}}
        with tempfile.TemporaryDirectory(prefix='hazlomexa-pilot-view-') as directory:
            path = Path(directory)/'copy.FCStd'
            shutil.copyfile(ORIGINAL, path)
            with zipfile.ZipFile(path) as archive:
                original = {name: archive.read(name) for name in archive.namelist()}
            policy.visible_document(path, reports)
            self.assertEqual(policy.document(path), catalog)
            with zipfile.ZipFile(path) as archive:
                data = archive.read('GuiDocument.xml')
                for name, expected in original.items():
                    self.assertEqual(archive.read(name), expected)
            camera = policy.xml(data).find('Camera').get('settings')
            tokens = camera.split()
            self.assertGreater(float(tokens[tokens.index('height')+1]), math.sqrt(10**2+20**2+30**2))
            policy.certified_view(data, catalog)
            pretty = policy.xml(data)
            policy.ET.indent(pretty)
            policy.certified_view(policy.ET.tostring(pretty, encoding='utf-8'), catalog)
            for bad in [data.replace(b'value="true"', b'value="false"'),
                        data.replace(b'name="Target"', b'name="Other"'),
                        data.replace(b'App::PropertyBool', b'App::PropertyPythonObject'),
                        data.replace(b'<Camera ', b'<Script/><Camera '),
                        policy.view_metadata(catalog, camera.replace('aspectRatio 1', 'aspectRatio 0')),
                        policy.view_metadata(catalog, camera.replace('position ', 'position inf ')),
                        policy.view_metadata(catalog, re.sub(r'height \S+', 'height 1e309', camera)),
                        policy.view_metadata(catalog, re.sub(r'height \S+', 'height 1e-20', camera))]:
                with self.assertRaises(ValueError):
                    policy.certified_view(bad, catalog)
            with self.assertRaises(ValueError):
                policy.visible_document(path, reports)

    def test_typed_units_and_invalid_requests(self):
        raw = dict(schema_version=1, job_id=str(uuid.uuid4()), cadpilot_version='0.7.7', operation='edit_box',
                   object='Target', units='in', parameters=dict(length=0.5, width=1, height=0.3))
        operation, name, values = policy.request(raw)
        self.assertEqual((operation,name), ('edit_box','Target'))
        for key, expected in dict(length=12.7,width=25.4,height=7.62).items():
            self.assertAlmostEqual(values[key],expected)
        for value in [True, '=external()', float('nan'), float('inf'), -1, 0, 1e300]:
            with self.assertRaises((ValueError, TypeError)):
                policy.request(dict(raw, parameters=dict(length=value,width=1,height=1)))
        for changed in [dict(raw, operation='execute_code'), dict(raw, object='../escape'), dict(raw, code='external'), dict(raw, units='guess')]:
            with self.assertRaises(ValueError):
                policy.request(changed)
        with self.assertRaises(ValueError):
            policy.unique_fields([('operation','inspect'),('operation','execute_code')])

    def test_native_archive_and_rejected_deserialization(self):
        self.assertEqual(policy.document(ORIGINAL), {'Target':'Part::Box'})
        with zipfile.ZipFile(ORIGINAL) as archive:
            contents = {name: archive.read(name) for name in archive.namelist()}
        for old, new in [
            (b'type="Part::Box"', b'type="Part::FeaturePython"'),
            (b'type="App::PropertyString"', b'type="App::PropertyPythonObject"'),
            (b'type="Part::AttachExtension"', b'type="App::ExtensionPython"'),
            (b'<ExpressionEngine count="0">', b'<ExpressionEngine count="1">'),
            (b'file="Target.Shape.brp"', b'file="../outside.brp"'),
        ]:
            changed = dict(contents)
            self.assertIn(old, contents['Document.xml'])
            changed['Document.xml'] = contents['Document.xml'].replace(old,new)
            self.rejected(changed)
        self.rejected(dict(contents, **{'../outside':b'escape'}))
        self.rejected(dict(contents, **{'Guidocument.xml':b'<GuiDocument/>'}))
        self.rejected(dict(contents, **{'script.py':b'raise RuntimeError()'}))

    def rejected(self, contents):
        with tempfile.TemporaryDirectory(prefix='hazlomexa-pilot-policy-') as directory:
            path = Path(directory)/'test.FCStd'
            with zipfile.ZipFile(path,'w') as archive:
                for name, data in contents.items():
                    archive.writestr(name,data)
            with self.assertRaises((ValueError, policy.ET.ParseError)):
                policy.document(path)


if __name__ == '__main__':
    unittest.main()
