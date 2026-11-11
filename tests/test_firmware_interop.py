"""Compile the portable C core and test identity plus real C/Rust PTY transactions."""
from pathlib import Path
import argparse
import os
import subprocess
import tempfile
import tty

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--firmware-dir", type=Path)
parser.add_argument("--identity-only", action="store_true")
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
firmware = args.firmware_dir or root / "firmware"
models = ["LCD_GENERIC", "DC162", "P618_V9", "FUTURE-BOARD.1", "Z" * 32]

with tempfile.TemporaryDirectory(prefix="lcd-generic-peer-") as directory:
    for model in models:
        peer = Path(directory) / model
        subprocess.run([
            "cc", "-std=c99", "-Wall", "-Wextra", "-Werror",
            *([] if model == "LCD_GENERIC" else [f'-DLCS_DEVICE_MODEL="{model}"']), "-I" + str(firmware),
            str(root / "tests/firmware_peer.c"), str(firmware / "lcs_protocol.c"),
            "-o", str(peer),
        ], check=True)
        result = subprocess.run(
            [str(peer)], input="key lcdiq 00000010 hello\r\nkey lcdiq 00000011 get\r\n",
            capture_output=True, text=True, check=True, timeout=3,
        )
        assert f"@LCS2 00000010 hello {model} 2\n" in result.stdout, (model, result.stdout)
        assert "@LCS2 00000011 end F8FCC34C\n" in result.stdout
        if args.identity_only:
            continue
        if model == models[0]:
            subprocess.run([
                "cargo", "build", "--offline", "--locked", "--no-default-features",
                "--example", "usb_probe",
            ], cwd=root, check=True)
        for mode in ["normal", "mismatch"]:
            master, slave = os.openpty()
            tty.setraw(slave)
            process = subprocess.Popen([str(peer), mode], stdin=master, stdout=master, stderr=subprocess.PIPE)
            try:
                command = [str(root / "target/debug/examples/usb_probe"), os.ttyname(slave), model]
                if mode == "mismatch":
                    command.append(mode)
                subprocess.run(command, check=True, timeout=15)
            finally:
                process.terminate()
                process.communicate(timeout=3)
                os.close(master)
                os.close(slave)
        print("C/Rust full settings and readback rejection:", model)
    for model in ["", "bad/model", "bad model", r"bad\0model", "坏机型", "Z" * 33]:
        peer = Path(directory) / "invalid-model"
        subprocess.run([
            "cc", "-std=c99", "-Wall", "-Wextra", "-Werror",
            f'-DLCS_DEVICE_MODEL="{model}"', "-I" + str(firmware),
            str(root / "tests/firmware_peer.c"), str(firmware / "lcs_protocol.c"),
            "-o", str(peer),
        ], check=True)
        result = subprocess.run([str(peer)], input="key lcdiq 00000010 hello\n", capture_output=True, text=True, check=True, timeout=3)
        assert "@LCS2 00000010 err MODEL\n" in result.stdout, (model, result.stdout)
print("Portable C identity validation and generic model protocol checks passed")
