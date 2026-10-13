"""Compatibility entry point. First run: npm ci --prefix tools/icon-export."""
import pathlib
import subprocess
subprocess.run(["npm", "run", "export", "--prefix", str(pathlib.Path(__file__).resolve().parent.parent / "tools/icon-export")], check=True)
