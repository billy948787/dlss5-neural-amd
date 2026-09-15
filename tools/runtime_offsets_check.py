"""Check the add-on's hardcoded runtime offsets against the runtime binary itself.

Every address in `InitEngine` and `RecordPasses` is a raw write into someone else's DLL, and a
wrong one does not fail -- it hangs the game or jumps into nothing. The only thing standing
between a bad port and a hang used to be reading the disassembly carefully. This is that reading,
written down so it runs.

It reads the offsets out of `src/neural/neural.cpp` rather than keeping its own copy, so it cannot
drift from the source it is checking.

    python tools/runtime_offsets_check.py <dlssnr_amd_pass1.dll> [src/neural/neural.cpp]

What it proves, in order of how much it would have caught:

  * The record entry's first three tests -- the gate every evaluation passes through -- resolve to
    three addresses the add-on actually uses. Those are `Enabled`, the native-failure byte and the
    engine-ready byte, and they are decoded out of the instruction stream, not assumed. A port that
    moved the option struct but missed one of these lands here.
  * Every data offset falls inside `.data`, and every entry point inside `.text`. Catches a whole
    block re-derived against the wrong build.
  * The file is the exact build `kRuntimeSha256` names, and it has been through
    `patch_runtime.py` -- both patch sites carry their patched bytes.

No binaries ship with this project: the DLL is yours.
"""

import hashlib
import re
import struct
import sys
from pathlib import Path


def sections(data):
    """(name, virtual address, virtual size, raw pointer) per PE section."""
    e = struct.unpack_from("<I", data, 0x3C)[0]
    if data[e:e + 4] != b"PE\0\0":
        raise ValueError("not a PE file")
    count = struct.unpack_from("<H", data, e + 6)[0]
    opt = struct.unpack_from("<H", data, e + 20)[0]
    table = e + 24 + opt
    out = []
    for i in range(count):
        off = table + i * 40
        name = data[off:off + 8].rstrip(b"\0").decode("latin1")
        vsize, va, _, raw = struct.unpack_from("<IIII", data, off + 8)
        out.append((name, va, vsize, raw))
    return out


def source_facts(text):
    """The pins and every runtime offset the add-on writes, read out of neural.cpp."""
    digest = re.search(r"kRuntimeSha256\[32\]\s*=\s*\{(.*?)\}", text, re.S)
    size = re.search(r"kRuntimeSize\s*=\s*(\d+)", text)
    if digest is None or size is None:
        raise ValueError("kRuntimeSha256 / kRuntimeSize not found in the source")
    sha = bytes(int(b, 16) for b in re.findall(r"0x([0-9a-fA-F]{2})", digest.group(1)))
    # Data: every At<T>(module, 0xRVA), plus the engine object handed to the init entry -- that one
    # is `base + rva` like an entry point is, which is exactly why it has to be picked out by what
    # it is cast TO. Calling it code and looking for it in .text is a false alarm, and the first
    # run of this script raised one.
    data = {int(m, 16) for m in re.findall(r"At<[^>]+>\([^,]+,\s*0x([0-9a-fA-F]+)\)", text)}
    data |= {int(m, 16) for m in re.findall(
        r"reinterpret_cast<void\s*\*>\(reinterpret_cast<uintptr_t>\([^)]+\)\s*\+\s*0x([0-9a-fA-F]+)\)", text)}
    # Code: only what is cast to a function pointer and then called.
    code = {int(m, 16) for m in re.findall(
        r"reinterpret_cast<\w+Fn>\(reinterpret_cast<uintptr_t>\([^)]+\)\s*\+\s*0x([0-9a-fA-F]+)\)", text)}
    return sha, int(size.group(1)), data, code


def rip_target(data, text_raw_delta, rva, length):
    """Where a rip-relative instruction at `rva` points. `length` is the whole instruction."""
    disp = struct.unpack_from("<i", data, rva - text_raw_delta + length - 5)[0]
    return rva + length + disp


def main(argv):
    if not 2 <= len(argv) <= 3:
        print(__doc__)
        return 2
    dll = Path(argv[1])
    src = Path(argv[2]) if len(argv) == 3 else Path(__file__).resolve().parent.parent / "src/neural/neural.cpp"

    raw = dll.read_bytes()
    want_sha, want_size, data_rvas, code_rvas = source_facts(src.read_text(encoding="utf-8", errors="replace"))

    bad = []
    def check(ok, said):
        print(("  ok   " if ok else "  FAIL ") + said)
        if not ok:
            bad.append(said)

    print(f"{dll}  against  {src}")

    check(len(raw) == want_size, f"size {len(raw)} == kRuntimeSize {want_size}")
    got = hashlib.sha256(raw).hexdigest()
    check(got == want_sha.hex(), f"sha256 {got[:16]}… == kRuntimeSha256 {want_sha.hex()[:16]}…")
    if bad:
        print("\nthe file is not the build these offsets belong to; nothing below would mean anything")
        return 1

    secs = {name: (va, vsize, rawp) for name, va, vsize, rawp in sections(raw)}
    text_va, text_vsize, text_raw = secs[".text"]
    data_va, data_vsize, _ = secs[".data"]
    delta = text_va - text_raw

    outside = sorted(r for r in data_rvas if not data_va <= r < data_va + data_vsize)
    check(not outside, f"{len(data_rvas)} data offsets inside .data "
                       f"[{data_va:#x}..{data_va + data_vsize:#x})"
                       + ("" if not outside else "  -- outside: " + ", ".join(f"{r:#x}" for r in outside)))

    astray = sorted(r for r in code_rvas if not text_va <= r < text_va + text_vsize)
    check(not astray, f"{len(code_rvas)} entry points inside .text "
                      f"[{text_va:#x}..{text_va + text_vsize:#x})"
                      + ("" if not astray else "  -- outside: " + ", ".join(f"{r:#x}" for r in astray)))

    # The gate. The record entry opens with cmp byte [rip+d],1 then two test byte [rip+d],1, and
    # those three addresses are Enabled, the native-failure byte and the engine-ready byte. Walk
    # them out of the bytes: an offset the add-on writes that the engine does not read here, or the
    # other way round, is the bug this whole file exists to catch.
    record = min(code_rvas, key=lambda r: abs(r - 0x12640)) if code_rvas else None
    gate, at = [], record
    for opcode, length in ((b"\x80\x3d", 7), (b"\xf6\x05", 7), (b"\xf6\x05", 7)):
        found = raw.find(opcode, at - delta, at - delta + 0x80)
        if found < 0:
            break
        at = found + delta
        gate.append(rip_target(raw, delta, at, length))
        at += length
    check(len(gate) == 3, f"record entry {record:#x}: decoded {len(gate)} of 3 opening tests")
    for name, rva in zip(("Enabled", "native-failure", "engine-ready"), gate):
        check(rva in data_rvas, f"record entry gates on {rva:#x} ({name}), which the add-on uses")

    # And that the DLL went through patch_runtime.py, because an unpatched one installs its own
    # detours on top of ours and executes every list twice.
    spec = src.parent.parent / "tools/runtime-patches.json"
    if spec.exists():
        import json
        for change in json.loads(spec.read_text())["changes"]:
            off, after = int(change["offset"], 16), bytes.fromhex(change["after"])
            check(raw[off:off + len(after)] == after,
                  f"patch at {change['offset']} applied ({change['reason'].split(',')[0][:54]}…)")

    print("\n" + ("PASS" if not bad else f"FAIL: {len(bad)} check(s)"))
    return 0 if not bad else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
