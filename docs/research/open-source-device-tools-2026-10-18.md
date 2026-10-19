# Open-source device investigation tools for Hazlo

Research date: 2026-10-18. This is a researched integration shortlist, not an
installation receipt or a claim of tested device compatibility. Use upstream
native interfaces and installed-version discovery; an MCP wrapper is optional.

## Recommended tools

| Need | Project and native interface | Windows and prerequisites | License / packaging consideration |
| --- | --- | --- | --- |
| Network evidence | [Wireshark TShark](https://www.wireshark.org/docs/man-pages/tshark.html), with dumpcap for capture. CLI interface discovery, filters, bounded capture, pcapng, JSON/field output and protocol/field catalogs. | Windows tooling available. Live capture needs a supported capture interface/driver; reading saved captures does not require collecting live traffic. | [Wireshark](https://github.com/wireshark/wireshark/blob/master/README.md) is predominantly GPL-2.0-or-later. [Npcap has separate redistribution restrictions](https://www.wireshark.org/docs/wsdg_html_chunked/ChLibsPcap); do not treat its inclusion in Wireshark's installer as permission to bundle it in Hazlo. |
| USB traffic | [USBPcap](https://github.com/desowin/usbpcap): USBPcapCMD CLI and [Wireshark extcap](https://desowin.org/usbpcap/tour.html). | Windows filter driver and appropriate capture privileges. Identify the actual USB device/root hub; validate the signed release on the Windows version Hazlo supports. Upstream's older tour is not proof of current Windows 11 acceptance. | Driver GPLv2; command-line component BSD-2-Clause. Driver installation may require restart; do not silently install/replace drivers. |
| Serial/UART evidence | [pySerial](https://github.com/pyserial/pyserial): Python API, [port discovery and miniterm](https://pyserial.readthedocs.io/en/latest/tools.html). | Windows backend; COM-port access. A USB-UART adapter and correct electrical interface may be required. Opening a port can affect control lines; it is not always passive observation. | BSD license. Small candidate for an app-local, pinned Python worker. |
| Digital signals and protocol decoding | [sigrok-cli](https://github.com/sigrokproject/sigrok-cli), libsigrokdecode; PulseView as a viewer. [Native device/module/decoder discovery](https://www.sigrok.org/wiki/Sigrok-cli) and [protocol decoders](https://www.sigrok.org/wiki/Protocol_decoders). | [Windows builds exist](https://www.sigrok.org/wiki/Windows). Live acquisition needs supported hardware and suitable drivers; saved traces allow offline decoding. UART, SPI and I2C are relevant examples, not a guarantee for an unknown board. | sigrok-cli GPL-3.0-or-later. Pin and verify a usable Windows build; upstream release and nightly packaging need runtime testing. |
| Application traffic | [mitmproxy / mitmdump](https://github.com/mitmproxy/mitmproxy): headless CLI, saved flows and [Python addons](https://docs.mitmproxy.org/stable/addons/overview/). | [Native Windows packages](https://docs.mitmproxy.org/stable/overview/installation/). [Modes differ by OS](https://docs.mitmproxy.org/stable/concepts/modes/); local process capture is supported on Windows, transparent mode is documented for Linux/macOS. External devices need a usable routing/proxy path. | MIT. HTTPS inspection [requires client certificate trust; pinning can prevent it](https://docs.mitmproxy.org/stable/concepts/certificates/). No automatic trust-store changes. |
| Embedded debugging | [pyOCD](https://github.com/pyocd/pyOCD): CLI, GDB server and [Python API](https://pyocd.io/docs/api_examples.html). [OpenOCD](https://github.com/openocd-org/openocd) offers an alternative target/probe ecosystem and [Tcl RPC](https://openocd.org/doc/html/Tcl-Scripting-API.html). | pyOCD supports Windows and Arm Cortex-M with supported probes. Target support, accessible debug pins, voltage and debug access must be established. OpenOCD needs a verified Windows distribution/configuration. Neither is a universal debugger for any chip. | pyOCD Apache-2.0; OpenOCD GPL. Prefer pyOCD where its target support fits; select OpenOCD where it adds needed support rather than shipping both indiscriminately. |
| Bench measurements | [PyVISA](https://github.com/pyvisa/pyvisa) with [PyVISA-py](https://github.com/pyvisa/pyvisa-py): resource discovery and native Python instrument communication. | Needs a supported instrument and backend/transport. USB, serial, Ethernet and GPIB support depend on dependencies and device/backend support. An arbitrary USB gadget is not automatically a VISA instrument. | Both MIT. Explicitly select the open-source backend where suitable; do not assume a proprietary VISA runtime is installed or redistributable. |
| Replacement-circuit simulation | [ngspice](https://ngspice.sourceforge.io/): batch executable or [shared library](https://ngspice.sourceforge.io/shared.html). | [Windows binaries](https://ngspice.sourceforge.io/download.html). Needs an appropriate circuit and models; results depend on model fidelity. Complements existing KiCad/firmware tools. | [Modified BSD with component exceptions](https://ngspice.sourceforge.io/devel.html). Review the selected binary's notices and model licenses before bundling. |

Further connection families can use [Bleak](https://github.com/hbldh/bleak) for
BLE discovery/GATT through a Python API (MIT; current upstream specifies Windows
11 version 22000+) and [python-can](https://github.com/hardbyte/python-can) for
CAN logging/communication (LGPL-3.0; Windows backend/hardware dependent). Bleak
is a BLE client, not a general Bluetooth traffic sniffer. These are follow-on
candidates when a real device requires them.

## Relationship to Rea

Rea remains the discovered software-analysis tool. It is not a substitute for
capture drivers, electrical instruments or debug probes. Keep original captures
and trace references; use the tool that understands each format, then provide
scoped evidence to the investigation. Route binaries and app packages through
Rea only when its installed engines and schemas support them.

The prior installed test reported missing engines. [Ghidra](https://github.com/NationalSecurityAgency/ghidra)
is an open-source engine candidate for that gap, but adding a separate competing
binary-analysis integration is unnecessary if Rea already exposes the required
backend. The installed Rea version must be checked before configuring engines;
current upstream capabilities do not establish bundled-runtime support.

## Proposed integration order

1. TShark/dumpcap, pySerial and USBPcap: network/USB/serial evidence. Start with
   saved captures and bounded local fixture sessions, then real-device sessions
   with identified interfaces and explicit scope. Driver setup remains visible.
2. sigrok-cli and ngspice: characterize relevant board interfaces and evaluate
   an adapter or replacement design using existing KiCad/firmware workflows.
3. pyOCD or OpenOCD and PyVISA-py: add the target/probe or instrument actually
   required. BLE/CAN support follows concrete device needs.

This order is an engineering recommendation, not an upstream requirement. No
new remote service, paid account or community MCP wrapper is needed merely to
use the documented CLI/API surfaces.

## Hazlo implementation requirements

The existing native transport already runs a selected executable without a shell,
pins its identity, reads its help and uses reviewable native calls. Its current
program scopes are the seven engineering groups plus planning; the tools above
are not already registered integrations. Discovery, routing and the saved
configuration must be extended without labeling a capture tool as a CAD program
or Rea. Existing single-program scope also needs an explicit handoff or managed
connections when investigation uses multiple tools.

Reuse the transport rather than creating a packet analyzer or serial editor in
Hazlo. CLI help, native protocol/decoder catalogs and installed-version schemas
provide capabilities; supported API packages need a versioned, bounded worker
interface. The model should not be given arbitrary Python execution merely to
call a library. Captures and measurements need retained, hashed artifacts with
paged summaries, deadlines/size limits and a usable stop action. A terminated
debugging operation can leave target state changed; stopping is not rollback.

Keep the simple chat: identify the device and question, discover the appropriate
tools, review the scoped action, let the native tool work, and inspect the result.
Repository checks should refresh tool metadata and report releases, as previously
requested; they must not silently install drivers or replace selected runtimes.

## Limits that affect real investigations

A capture program cannot see traffic that never reaches its interface. Wireshark's
[capture setup guidance](https://wiki.wireshark.org/capturesetup) requires a suitable
observation point; [switched networks](https://wiki.wireshark.org/capturesetup/Ethernet)
may require mirroring, capture on the endpoint/router, or another appropriate
topology. Installing TShark on a nearby PC does not establish access to an IoT
device's traffic. USBPcap sees Windows host USB transfers, not an unplugged board's
internal buses; a logic analyzer or probe is a different acquisition path.

An additional open-source acquisition route is
[tcpdump](https://github.com/the-tcpdump-group/tcpdump) with
[libpcap](https://github.com/the-tcpdump-group/libpcap) on a suitable Linux
capture host or supported router, then analysis of the saved capture with TShark
on Windows. This is a proposed topology, not an assumption that the user's router
exposes capture access, nor a drop-in Windows replacement for Npcap.

This research did not install packages, alter drivers/trust/network settings,
capture user traffic, connect to hardware or validate a physical repair. The
shortlist's license labels are upstream metadata for implementation planning;
the exact distributed artifacts and their dependencies still need a packaging
notice/source-obligation review before release.
