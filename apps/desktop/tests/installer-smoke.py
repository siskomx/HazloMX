"""Explicit local NSIS lifecycle check; refuses to replace an existing install."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import uuid
import winreg


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


KEY = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Hazlo Mexa"


def registration(hive):
    for view in (winreg.KEY_WOW64_64KEY, winreg.KEY_WOW64_32KEY):
        try:
            with winreg.OpenKey(hive, KEY, 0, winreg.KEY_READ | view) as key:
                return winreg.QueryValueEx(key, "InstallLocation")[0]
        except FileNotFoundError:
            pass
    return None


package, project = (Path(value).resolve(strict=True) for value in sys.argv[1:])
assert not registration(winreg.HKEY_CURRENT_USER), "Existing user installation; refusing replacement"
assert not registration(winreg.HKEY_LOCAL_MACHINE), "Existing machine installation; refusing replacement"
assert (project / "hazlomx.json").is_file(), "Pass an existing test project"
before = {str(file.relative_to(project)): digest(file) for file in project.rglob("*") if file.is_file()}
local = Path(os.environ["LOCALAPPDATA"]).resolve()
root = local / "HazloMexa" / "installer-tests" / str(uuid.uuid4())
root.mkdir(parents=True)
install = root / "app"
assert install.is_relative_to(local) and not install.exists()
appdata = local / "mx.hazlomexa.desktop"
appdata.mkdir(exist_ok=True)
sentinel = appdata / f"installer-preservation-{root.name}.txt"
sentinel.write_text("preserve user data", encoding="utf-8")
sentinel_hash = digest(sentinel)
result = {"package": str(package), "sha256": digest(package), "install": str(install), "project": str(project), "checks": []}
installed = False
try:
    subprocess.run([str(package), "/S", "/NS", f"/D={install}"], check=True, timeout=120)
    installed = True
    executable = install / "hazlomexa.exe"
    uninstaller = install / "uninstall.exe"
    assert executable.is_file() and uninstaller.is_file()
    assert registration(winreg.HKEY_CURRENT_USER).strip('"') == str(install)
    assert not registration(winreg.HKEY_LOCAL_MACHINE)
    result["checks"].append("installation registered only for current user")
    # No /R: installation must not launch the app automatically.
    subprocess.run([str(package), "/S", "/NS", "/UPDATE", f"/D={install}"], check=True, timeout=120)
    assert executable.is_file()
    result["checks"].append("same-version package update succeeds")
    process = subprocess.Popen([str(executable)], cwd=root, stdin=subprocess.DEVNULL,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        time.sleep(3)
        assert process.poll() is None, "Installed application exited during startup"
        result["checks"].append("installed application remains running after startup")
    finally:
        if process.poll() is None:
            process.terminate()
        process.wait(timeout=15)
finally:
    if installed:
        # Normal NSIS temporary uninstaller removes its own executable asynchronously.
        subprocess.run([str(install / "uninstall.exe"), "/S"], check=True, timeout=120)
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline and ((install / "hazlomexa.exe").exists() or registration(winreg.HKEY_CURRENT_USER)):
            time.sleep(0.1)
        assert not (install / "hazlomexa.exe").exists()
        assert not registration(winreg.HKEY_CURRENT_USER)
        result["checks"].append("uninstallation removes application and registration")
    assert sentinel.is_file() and digest(sentinel) == sentinel_hash
    after = {str(file.relative_to(project)): digest(file) for file in project.rglob("*") if file.is_file()}
    assert before == after, "Installer lifecycle modified external project files"
    result["checks"].append("app-data marker retained; external project files preserved byte-for-byte")
    sentinel.unlink()
result["pass"] = True
(root / "result.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
print(json.dumps(result, indent=2))
print(f"Evidence retained at {root}")
