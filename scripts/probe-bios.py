#!/usr/bin/env python3
"""Run an authorized local raw/ZIP BIOS without adding firmware to public assets."""
import argparse
import subprocess
import tempfile
from pathlib import Path
from zipfile import ZipFile, is_zipfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bios", type=Path)
    parser.add_argument("--frames", type=int, default=200)
    parser.add_argument("--output", type=Path, default=Path("target/private-bios/frame.ppm"))
    parser.add_argument("--buttons", type=Path, help="optional frame / hexadecimal button-mask TSV")
    args = parser.parse_args()
    if args.frames < 1:
        parser.error("--frames must be positive")
    if is_zipfile(args.bios):
        with ZipFile(args.bios) as archive:
            candidates = [entry for entry in archive.infolist()
                          if not entry.is_dir() and entry.file_size == 524288]
            if len(candidates) != 1:
                parser.error("ZIP must contain exactly one 524288-byte BIOS")
            data = archive.read(candidates[0])
    else:
        if args.bios.stat().st_size != 524288:
            parser.error("raw BIOS must be exactly 524288 bytes")
        data = args.bios.read_bytes()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="saturn-bios-") as directory:
        bios = Path(directory) / "bios.bin"
        bios.write_bytes(data)
        command = ["cargo", "run", "--locked", "--release", "--example", "bios_probe",
                   "--", str(bios), str(args.frames), str(args.output)]
        if args.buttons:
            command.append(str(args.buttons))
        subprocess.run(command, check=True)


if __name__ == "__main__":
    main()
