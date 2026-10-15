"""Restore the official Python runtime needed by the owned native KiCad GUI tests."""
import hashlib
from pathlib import Path
import subprocess


ROOT = Path(__file__).resolve().parents[2]
PROBE = ROOT / "target/kicad-probe"
ARCHIVE = PROBE / "kicad-10.0.7-x86_64.exe"
EXPECTED = "ce3881b6a9188eb34ac1ebaaec7651149e8a025206410addfb11a1a9656ecf95"
RUNTIME = PROBE / "runtime"


def main():
    with ARCHIVE.open("rb") as source:
        assert hashlib.file_digest(source, "sha256").hexdigest() == EXPECTED, "KiCad installer identity changed"
    assert RUNTIME.resolve().is_relative_to((ROOT / "target").resolve())
    seven_zip = Path("C:/Program Files/7-Zip/7z.exe")
    assert seven_zip.is_file(), "Install 7-Zip before preparing the native test runtime"
    with (PROBE / "gui-python-extraction.log").open("wb") as log:
        subprocess.run(
            [str(seven_zip), "x", str(ARCHIVE), "-o" + str(RUNTIME),
             "bin/Lib/*", "bin/DLLs/*", "-aos", "-y"],
            stdout=log, stderr=subprocess.STDOUT, check=True, timeout=120,
        )
    assert (RUNTIME / "bin/Lib/encodings/__init__.py").is_file()
    with (PROBE / "gui-python-check.log").open("wb") as log:
        subprocess.run(
            [str(RUNTIME / "bin/python.exe"), "-I", "-c",
             "import encodings, pcbnew; assert pcbnew.Version().startswith('10.0.7')"],
            stdout=log, stderr=subprocess.STDOUT, check=True, timeout=30,
        )
    print("Official KiCad Python runtime restored; isolated native imports pass.")


if __name__ == "__main__":
    main()
