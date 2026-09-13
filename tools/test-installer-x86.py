"""Portable installer tests. Pass a private release fixture with pinned payloads."""
from pathlib import Path
import subprocess,sys,tempfile,shutil
r=Path(__file__).resolve().parents[1]
if len(sys.argv)!=2:raise SystemExit('Usage: python tools/test-installer-x86.py <release-fixture>')
with tempfile.TemporaryDirectory(prefix='installer-native-test-') as d:
 out=Path(d)/'installer-tests'
 subprocess.run([shutil.which('g++'),'-std=c++20','-O2','-Wall','-Wextra','-Werror','-Wno-misleading-indentation',str(r/'installer-x86/tests.cpp'),'-lcrypto','-lz','-o',str(out)],check=True)
 subprocess.run([str(out),str(Path(sys.argv[1]).resolve())],check=True)
print('UNVALIDATED: MSVC installer GUI build, Windows .NET ZIP extraction and official Setup invocation, live ReShade docking and game/GPU regression.')
