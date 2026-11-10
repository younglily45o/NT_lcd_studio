"""Package local Windows artifacts. Run after the release build and examples/generate."""
from pathlib import Path
import hashlib
import shutil
import zipfile

root = Path(__file__).resolve().parents[1]
binary = root / "target/x86_64-pc-windows-gnu/release/LCD-Studio.exe"
if not binary.is_file():
    raise SystemExit("Build Windows release first")
dist = root / "dist"
dist.mkdir(exist_ok=True)
out = dist / "LCD-Studio-Windows-x64"
# Staging is owned by this script. Recreate it so stale files cannot enter the ZIP.
if out.exists():
    shutil.rmtree(out)
out.mkdir()
shutil.copyfile(binary, out / binary.name)
shutil.copyfile(root / "README.md", out / "README.md")
shutil.copyfile(root / "assets/FONT-LICENSE.txt", out / "FONT-LICENSE.txt")
(out / "examples").mkdir()
for name in ["dc162.json", "linear.json", "blue-lift.json", "dc162-gamma.h", "color-control.h"]:
    shutil.copyfile(root / "examples" / name, out / "examples" / name)
checksums = []
for file in sorted(out.rglob("*")):
    if file.is_file():
        checksums.append(hashlib.sha256(file.read_bytes()).hexdigest() + "  " + file.relative_to(out).as_posix())
(out / "SHA256SUMS.txt").write_text("\n".join(checksums) + "\n", encoding="utf-8")
archive = dist / "LCD-Studio-Windows-x64.zip"
with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as package:
    for file in sorted(out.rglob("*")):
        if file.is_file():
            package.write(file, file.relative_to(out.parent))
digest = hashlib.sha256(archive.read_bytes()).hexdigest()
archive.with_suffix(".zip.sha256").write_text(digest + "  " + archive.name + "\n")
print(archive)
print("SHA256", digest)
