"""Worker behavior without opening user hardware. Real library discovery is in diagnostic_probe."""
import importlib.util
import sys
from pathlib import Path
from types import ModuleType, SimpleNamespace
from unittest.mock import patch

sys.dont_write_bytecode = True

path = Path(__file__).resolve().parents[2] / 'apps/desktop/src-tauri/resources/serial/worker.py'
spec = importlib.util.spec_from_file_location('hazlo_serial_worker', path)
worker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(worker)
serial = ModuleType('serial')
serial.VERSION = 'fixture'
tools = ModuleType('serial.tools')
ports = SimpleNamespace(comports=lambda: [SimpleNamespace(device='COM7', description='Owned fixture', vid=1, pid=2)])
tools.list_ports = ports
opened = []


class Connection:
    def __init__(self, **kwargs):
        assert kwargs['port'] is None and kwargs['timeout'] == 0.1
        self.closed = False
        opened.append(self)

    def open(self):
        assert self.port == 'COM7' and self.dtr is False and self.rts is False

    def read(self, count):
        return b'\x00\xffabc'[:count]

    def close(self):
        self.closed = True

    def write(self, data):
        raise AssertionError('Worker transmitted bytes')


serial.Serial = Connection
with patch.dict('sys.modules', {'serial': serial, 'serial.tools': tools}):
    assert worker.execute({'operation': 'list_ports'})['ports'][0]['port'] == 'COM7'
    request = {'operation': 'capture', 'port': 'COM7', 'baudrate': 9600, 'seconds': 1, 'max_bytes': 7}
    reply = worker.execute(request)
    assert reply['bytes'] == 7 and reply['data_hex'] == '00ff61626300ff'
    assert reply['transmitted_bytes'] == 0 and reply['limit_reached'] and opened[-1].closed
    for changes in [{'port': 'socket://localhost:1234'}, {'seconds': 31}, {'max_bytes': 32769}, {'operation': 'exec'}, {'baudrate': True}, {'code': 'print(1)'}]:
        try:
            worker.execute(dict(request, **changes))
        except ValueError:
            pass
        else:
            raise AssertionError(f'Invalid request accepted: {changes}')
    assert len(opened) == 1, 'Rejected requests touched hardware'
print('PASS: bounded binary capture, closure, no transmit, stale/unlisted port refusal and no code/URL injection (fixture)')
