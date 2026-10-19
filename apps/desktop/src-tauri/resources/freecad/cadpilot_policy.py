"""Validate typed CADPilot requests and native documents before FreeCAD deserialization."""
import math
from pathlib import PurePosixPath
import re
import uuid
import xml.etree.ElementTree as ET
import zipfile

SCALES = {"mm": 1, "cm": 10, "m": 1000, "in": 25.4, "ft": 304.8}
NATIVE_TYPES = {"Part::Box", "Part::Cylinder", "Part::Sphere", "Part::Cone", "Part::Torus"}
PROPERTY_TYPES = {
    "App::PropertyString", "App::PropertyUUID", "App::PropertyBool", "App::PropertyMap",
    "App::PropertyEnumeration", "App::PropertyFloat", "App::PropertyLength", "App::PropertyAngle",
    "App::PropertyPlacement", "App::PropertyExpressionEngine", "App::PropertyLinkSubList",
    "App::PropertyLink", "Part::PropertyPartShape", "Materials::PropertyMaterial",
}


def unique_fields(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("Duplicate request field")
        result[key] = value
    return result


def verify_step_solids(actual, expected):
    def measures(shape):
        box, center = shape.BoundBox, shape.CenterOfMass
        values = (shape.Volume, box.XMin, box.YMin, box.ZMin, box.XMax, box.YMax, box.ZMax,
                  center.x, center.y, center.z)
        if not all(math.isfinite(value) for value in values) or values[0] <= 0:
            raise ValueError('STEP solid has invalid measurements')
        return values

    remaining = [measures(shape) for shape in expected]
    if len(actual) != len(remaining) or not remaining:
        raise ValueError('STEP solid count differs from the edited copy')
    for shape in actual:
        values = measures(shape)
        for index, approved in enumerate(remaining):
            if (abs(values[0]-approved[0]) <= max(0.01, approved[0]*0.00002)
                    and all(abs(a-b) <= 0.000001 for a, b in zip(values[1:], approved[1:]))):
                remaining.pop(index)
                break
        else:
            raise ValueError('STEP solid dimensions or position differ from the edited copy')


def request(raw):
    if set(raw) != {"schema_version", "job_id", "cadpilot_version", "operation", "object", "units", "parameters"}:
        raise ValueError("Unsupported CADPilot request fields")
    if type(raw["schema_version"]) is not int or raw["schema_version"] != 1 or raw["cadpilot_version"] != "0.7.7":
        raise ValueError("Unsupported CADPilot request version")
    if str(uuid.UUID(raw["job_id"])) != raw["job_id"] or uuid.UUID(raw["job_id"]).int == 0:
        raise ValueError("Invalid job identity")
    name = raw["object"]
    if not isinstance(name, str) or not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]{0,63}", name):
        raise ValueError("Select an internal object name")
    if raw["units"] not in SCALES:
        raise ValueError("Confirm metric or imperial input units")
    fields = {"inspect": set(), "edit_box": {"length", "width", "height"},
              "add_box": {"length", "width", "height", "x", "y", "z"},
              "add_cylinder": {"radius", "height", "x", "y", "z"}}
    operation = raw["operation"]
    if operation not in fields or not isinstance(raw["parameters"], dict) or set(raw["parameters"]) != fields[operation]:
        raise ValueError("Unsupported typed CADPilot operation")
    values = {}
    for key, value in raw["parameters"].items():
        if type(value) not in (int, float) or not math.isfinite(value):
            raise ValueError("Use finite numeric parameters; expressions are not accepted")
        value *= SCALES[raw["units"]]
        if not math.isfinite(value) or (abs(value) > 10000 if key in {"x", "y", "z"} else not 0.1 <= value <= 1000):
            raise ValueError("CADPilot parameter outside supported millimetre range")
        values[key] = value
    return operation, name, values


def xml(data):
    if len(data) > 2 * 1024 * 1024 or b"<!DOCTYPE" in data.upper() or b"<!ENTITY" in data.upper():
        raise ValueError("Unsupported document XML")
    root = ET.fromstring(data)
    if sum(1 for _ in root.iter()) > 50000:
        raise ValueError("Document XML exceeds the node limit")
    return root


def view_metadata(catalog, camera):
    root = ET.Element('Document', SchemaVersion='1')
    providers = ET.SubElement(root, 'ViewProviderData', Count=str(len(catalog)))
    for name in sorted(catalog):
        provider = ET.SubElement(providers, 'ViewProvider', name=name, expanded='0')
        properties = ET.SubElement(provider, 'Properties', Count='1')
        prop = ET.SubElement(properties, 'Property', name='Visibility', type='App::PropertyBool', status='1')
        ET.SubElement(prop, 'Bool', value='true')
    ET.SubElement(root, 'Camera', settings=camera)
    return ET.tostring(root, encoding='utf-8', xml_declaration=True)


def visible_document(path, reports):
    low = [min(report['bbox'][axis+'min'] for report in reports.values()) for axis in 'xyz']
    high = [max(report['bbox'][axis+'max'] for report in reports.values()) for axis in 'xyz']
    radius = math.sqrt(sum((b-a)**2 for a, b in zip(low, high))) / 2
    if not math.isfinite(radius) or radius <= 0:
        raise ValueError('Cannot frame empty native geometry')
    distance = 4 * radius
    offset = distance / math.sqrt(3)
    center = [(a+b)/2 for a, b in zip(low, high)]
    position = [center[0]+offset, center[1]-offset, center[2]+offset]
    camera = (f"OrthographicCamera {{ position {' '.join(str(value) for value in position)} "
              f"orientation 0.74290609 0.30772209 0.59447283 1.2171158 "
              f"nearDistance {distance-2*radius} farDistance {distance+2*radius} "
              f"aspectRatio 1 focalDistance {distance} height {4*radius} }}")
    with zipfile.ZipFile(path, 'a', compression=zipfile.ZIP_DEFLATED) as archive:
        if any(name.lower() == 'guidocument.xml' for name in archive.namelist()):
            raise ValueError('Unexpected GUI state in native save')
        archive.writestr('GuiDocument.xml', view_metadata(reports, camera))


def certified_view(data, catalog):
    gui = xml(data)
    node = gui.find('Camera')
    camera = node.get('settings', '') if node is not None else ''
    number = r'(-?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?)'
    pattern = (rf'OrthographicCamera \{{ position {number} {number} {number} '
               rf'orientation 0\.74290609 0\.30772209 0\.59447283 1\.2171158 '
               rf'nearDistance {number} farDistance {number} aspectRatio 1 '
               rf'focalDistance {number} height {number} \}}')
    match = re.fullmatch(pattern, camera)
    if match:
        values = [float(value) for value in match.groups()]
        if (all(math.isfinite(value) and abs(value) <= 1000000 for value in values)
                and all(value >= 0.000001 for value in values[3:]) and values[3] < values[4]
                and ET.canonicalize(data.decode('utf-8'), strip_text=True)
                == ET.canonicalize(view_metadata(catalog, camera).decode('utf-8'), strip_text=True)):
            return
    # shortcut: only our minimal visibility/camera profile is certified; general GUI saves need a fidelity gate.
    raise ValueError('GUI document state is not yet supported by the selected-copy adapter')


def document(path, *, headless=False):
    if path.stat().st_size > 32 * 1024 * 1024:
        raise ValueError("Document exceeds 32 MiB")
    entries = {}
    total = 0
    with zipfile.ZipFile(path) as archive:
        if len(archive.infolist()) > 256:
            raise ValueError("Document has too many resources")
        for item in archive.infolist():
            name = item.filename
            if (item.is_dir() or name.startswith('/') or '\\' in name or ':' in name
                    or any(p in {'.', '..'} for p in PurePosixPath(name).parts)
                    or any(ord(c) < 32 for c in name) or name.lower() in entries
                    or item.external_attr >> 16 & 0o170000 == 0o120000
                    or item.file_size > 32 * 1024 * 1024):
                raise ValueError("Invalid document resource")
            total += item.file_size
            if total > 128 * 1024 * 1024:
                raise ValueError("Expanded document exceeds limit")
            if name not in {"Document.xml", "GuiDocument.xml", "thumbnails/Thumbnail.png"} and not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*\.Shape\.brp", name):
                raise ValueError("Document resource is not certified")
            entries[name.lower()] = archive.read(item)
    root = xml(entries.get('document.xml', b''))
    if root.tag != 'Document' or root.get('SchemaVersion') != '4':
        raise ValueError("Unsupported FreeCAD document schema")
    objects = root.find('Objects')
    data = root.find('ObjectData')
    if objects is None or data is None:
        raise ValueError("Document lacks an object catalog")
    catalog = {}
    for obj in objects.findall('Object'):
        name, kind = obj.get('name'), obj.get('type')
        if (not isinstance(name, str) or not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]{0,63}", name)
                or name in catalog or kind not in NATIVE_TYPES):
            raise ValueError("This document requires an uncertified object type")
        catalog[name] = kind
    if (not catalog or len(catalog) > 32 or len(data.findall('Object')) != len(catalog)
            or {obj.get('name') for obj in data.findall('Object')} != set(catalog)):
        raise ValueError("Invalid native object catalog")
    for node in root.iter():
        if node.tag in {'Property', '_Property'} and node.get('type') not in PROPERTY_TYPES:
            raise ValueError("Unsupported document property type")
        if node.tag == 'Extension' and node.get('type') not in {'Part::AttachExtension','Part::PrismExtension'}:
            raise ValueError("Unsupported document extension")
        if node.tag in {'Expression', 'XLink', 'Python', 'Script'}:
            raise ValueError("Document expressions, Python and external links are not certified")
        if node.tag in {'ExpressionEngine', 'LinkSubList', 'ObjectDeps'} and node.get('count', node.get('Count', '0')) != '0':
            raise ValueError("Document dependencies are not certified")
        if 'file' in node.attrib and node.attrib['file'].lower() not in entries:
            raise ValueError("Document references an uncaptured resource")
        if node.tag == 'Link' and node.get('value', ''):
            raise ValueError("Linked documents are not certified")
    if 'guidocument.xml' in entries and not headless:
        certified_view(entries['guidocument.xml'], catalog)
    return catalog
