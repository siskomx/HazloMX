"""Bounded pySerial diagnostics. Invoked with isolated Python, never agent code."""
import json
import sys
import time


def execute(request):
    import serial
    from serial.tools import list_ports

    operation = request.get('operation')
    if operation == 'info':
        return {'library': 'pySerial', 'version': serial.VERSION,
                'operations': ['list_ports', 'capture'], 'transmit': False,
                'capture_limits': {'seconds': 30, 'bytes': 32768},
                'opening_port_may_affect_device': True}
    if operation == 'list_ports':
        if set(request) != {'operation'}:
            raise ValueError('Unexpected port discovery arguments')
        ports = sorted(list_ports.comports(), key=lambda port: port.device)
        if len(ports) > 128:
            raise ValueError('Too many ports; result was not truncated')
        return {'ports': [{'port': port.device, 'description': port.description,
                           'vid': port.vid, 'pid': port.pid} for port in ports]}
    if operation != 'capture' or set(request) != {'operation', 'port', 'baudrate', 'seconds', 'max_bytes'}:
        raise ValueError('Unsupported diagnostic request')
    port, baud, seconds, maximum = (request[key] for key in ('port', 'baudrate', 'seconds', 'max_bytes'))
    if (not isinstance(port, str) or not port or len(port) > 256
            or type(baud) is not int or not 50 <= baud <= 4000000
            or type(seconds) not in (int, float) or not 0 < seconds <= 30
            or type(maximum) is not int or not 1 <= maximum <= 32768):
        raise ValueError('Invalid capture limits')
    # Only enumerated local ports; no RFC2217, socket URLs or arbitrary device paths.
    if port not in [item.device for item in list_ports.comports()]:
        raise ValueError('Port is no longer present; discover ports again')
    started = time.monotonic()
    data = bytearray()
    connection = serial.Serial(port=None, baudrate=baud, timeout=0.1,
                               write_timeout=0.1, rtscts=False, dsrdtr=False)
    connection.dtr = False
    connection.rts = False
    connection.port = port
    try:
        connection.open()
        while time.monotonic() - started < seconds and len(data) < maximum:
            data.extend(connection.read(min(1024, maximum - len(data))))
    finally:
        connection.close()
    return {'port': port, 'baudrate': baud, 'bytes': len(data), 'data_hex': data.hex(),
            'elapsed_seconds': round(time.monotonic() - started, 3),
            'limit_reached': len(data) == maximum, 'transmitted_bytes': 0,
            'opening_port_may_affect_device': True}


if __name__ == '__main__':
    try:
        request = {'operation': 'info'} if sys.argv[1:] == ['--help'] else json.loads(sys.argv[1])
        print(json.dumps(execute(request), ensure_ascii=True))
    except Exception as error:
        print(json.dumps({'error': str(error)}, ensure_ascii=True))
        sys.exit(1)
