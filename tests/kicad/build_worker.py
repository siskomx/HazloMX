"""Build the private native PCB helper; all dependencies and work files stay in target."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import uuid


ROOT = Path(__file__).resolve().parents[2]


def build(python):
    python = Path(python).resolve(strict=True)
    subprocess.run([str(python), "-I", "-S", "-c",
        "import sys; assert sys.platform == 'win32' and sys.version_info[:2] == (3, 11)"],
        check=True, timeout=10, capture_output=True)
    destination = ROOT / "target/native-pcb-worker"
    destination.mkdir(parents=True, exist_ok=True)
    work = destination / str(uuid.uuid4())
    work.mkdir()
    site = work / "site"
    with (work / "build.log").open("wb") as log:
        subprocess.run([sys.executable, "-m", "pip", "--isolated", "--disable-pip-version-check",
            "install", "--no-input", "--only-binary=:all:", "--require-hashes", "--no-compile",
            "--platform", "win_amd64", "--python-version", "3.11", "--implementation", "cp",
            "--abi", "cp311", "--target", str(site),
            "-r", str(ROOT / "tests/kicad/ipc-client-requirements.txt"),
            "-r", str(ROOT / "tests/kicad/worker-build-requirements.txt")],
            stdout=log, stderr=subprocess.STDOUT, check=True, timeout=180)
        subprocess.run([str(python), "-I", "-S", "-c",
            "import sys; sys.path.insert(0, sys.argv.pop(1)); from PyInstaller.__main__ import run; run()",
            str(site), "--onefile", "--console", "--noupx", "--clean", "--noconfirm",
            "--name", "hazlomexa-kicad", "--recursive-copy-metadata", "kicad-python",
            "--distpath", str(work / "dist"), "--workpath", str(work / "build"),
            "--specpath", str(work), str(ROOT / "apps/desktop/src-tauri/resources/kicad/live.py")],
            cwd=work, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=240)
    executable = work / "dist/hazlomexa-kicad.exe"
    result = subprocess.run([str(executable), "--self-check"],
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True, timeout=30)
    assert not result.stderr
    assert json.loads(result.stdout) == {"native_pcb_version": 1, "client_version": "0.8.0"}
    shutil.copyfile(executable, destination / executable.name)
    receipt = {"sha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
        "source_sha256": hashlib.sha256((ROOT / "apps/desktop/src-tauri/resources/kicad/live.py").read_bytes()).hexdigest(),
        "self_check": True}
    (destination / "build-receipt.json").write_text(json.dumps(receipt, indent=2))
    print("Native PCB worker built; frozen imports and pinned client metadata pass.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--python", required=True, help="Windows x64 CPython 3.11 executable")
    build(parser.parse_args().python)
