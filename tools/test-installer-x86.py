"""Portable installer tests. Pass a private release fixture with pinned payloads."""
from pathlib import Path
import os,subprocess,sys,tempfile,shutil
r=Path(__file__).resolve().parents[1]
if len(sys.argv)!=2:raise SystemExit('Usage: python tools/test-installer-x86.py <release-fixture>')
compiler=os.environ.get('CXX') or shutil.which('g++')
if not compiler:raise SystemExit('Set CXX to a C++20 compiler')
with tempfile.TemporaryDirectory(prefix='installer-native-test-') as d:
 out=Path(d)/'installer-tests'
 subprocess.run([compiler,'-std=c++20','-O2','-Wall','-Wextra','-Werror','-Wno-misleading-indentation',str(r/'installer-x86/tests.cpp'),'-lcrypto','-o',str(out)],check=True)
 subprocess.run([str(out),str(Path(sys.argv[1]).resolve())],check=True)
print('UNVALIDATED: live ReShade docking, D3D8 translation and game/GPU regression.')
