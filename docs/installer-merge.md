# Merging the two installers

There are two installers today. `installer/` is Rust with a ratatui terminal UI and covers the x64
routes; `installer-x86/` is C++ with a hand-drawn Win32 GUI and covers the x86 bridge. They do not
share a line of code, and each has safety the other lacks. This is the plan to make them one.

Nothing here is implemented yet. It records the decisions so the work does not start by
re-litigating them.

## Where they differ today

| | `installer/` (Rust, 1726 lines) | `installer-x86/` (C++, 523 lines) |
|---|---|---|
| Interface | ratatui TUI | Win32 GUI |
| Presets | PCSX2, RPCS3, D3D11, D3D12, Vulkan | D3D11, D3D9, D3D8 |
| PE machine check | no | yes, rejects PE32+ |
| ReShade `[INSTALL] BasePath` | no | yes |
| Backups, manifest, journal, ownership | none | all of it |
| Payload | `addon64` embedded via `include_bytes!` | external, hash-pinned sidecars |
| Installs ReShade | refuses, as policy | yes, from a pinned sidecar |
| Preflight: writable dir, locked file, free space, disabled add-on | yes | no |
| Sweeps legacy `pass2..pass10` | yes | no |
| Tests | 27 `#[test]`, 283 lines | 63 assertions, 85 lines |

The x86 side owns the install-safety model. The x64 side owns the preflight. Neither is optional in
a merged tool, and losing the preflight would be a regression for everyone using the x64 route.

## Decisions

### 1. Payloads stay external, and coupling is kept by pinned hash

The Rust installer embeds `dlss5-neural.addon64` so that "the installer cannot hand out an add-on
from a different release than the one it was built beside". That guarantee is worth keeping. Its
implementation is not.

What that policy actually wants is *version coupling*, and a hash pinned into the binary proves the
same thing as embedding the bytes. The x86 installer already works this way for every payload it
touches, including `payload.sha256` for the bridge pair, and it scales to files that cannot be
embedded: the weights are 147 MB and NVIDIA-derived, the runtime is a third-party build, and
d3d8to9 is someone else's release asset. The merged installer has to handle those externally no
matter what, so embedding only the add-on is an inconsistency rather than a principle.

**Decision:** all payloads external; the build generates the manifest of pinned hashes; the binary
carries the hashes, not the bytes. The coupling guarantee survives and applies uniformly.

### 2. ReShade is installed only from a pinned sidecar, never fetched

The Rust installer refuses to install ReShade and says it is not going to. The x86 installer
installs it when a hash-pinned sidecar is present.

These are less opposed than they look. The x86 rule is: install the pinned build if the release
carries it, otherwise require ReShade already present and validate its hash; never download it,
never execute its setup. Public packages ship without that sidecar, so in the public case the
behaviour *is* the Rust policy — the user installs ReShade, the installer verifies it. The private
case adds a capability that fails closed.

**Decision:** adopt the x86 behaviour. It generalises the Rust policy instead of contradicting it.

### 3. The merged tool is Rust

Porting the x86 safety model into Rust moves `core.h` plus its tests, about 330 lines. Porting the
x64 logic into C++ moves `work.rs`, its 283 lines of tests and the preset logic, closer to 800. The
smaller move also preserves the larger test suite.

**Cost, stated plainly:** the hand-drawn Win32 GUI in `installer-x86/main.cpp` does not survive the
first pass. The TUI does, and it works. This is deferral rather than loss: `installer/Cargo.toml`
already depends on `windows-sys`, so adding the `Win32_Graphics_Gdi` and
`Win32_UI_WindowsAndMessaging` features lets that paint code be ported onto the same core later if
the GUI is wanted back. Do not let that block the merge.

## Target flow

**Bitness is detected, never asked.** It is in the PE header and `installer-x86/core.h` already
reads it (`machine(read(target))==0x14c`). Asking the user invites a wrong answer and creates a
failure mode that does not need to exist. Detect it, then say which route will be used.

The API × bitness matrix is sparse — five of ten cells exist:

| | 32-bit | 64-bit |
|---|---|---|
| D3D8 | d3d8to9 -> D3D9 frontend -> host64 | no such game |
| D3D9 | native D3D9 frontend -> host64 | addon64 has no D3D9 route |
| D3D11 | native D3D11 frontend -> host64 | native addon64 |
| D3D12 | bridge has no D3D12 route | native addon64 |
| Vulkan | bridge has no Vulkan route | native addon64 |

So detecting bitness reduces the offered APIs by itself: 32-bit offers D3D8/D3D9/D3D11, 64-bit
offers D3D11/D3D12/Vulkan. One question instead of two, and no invalid combination is selectable.

API selection stays manual. Import-table autodetection was considered and not added, because a game
can resolve its API dynamically and a confident wrong guess is worse than a question.

**Do not collapse the emulator presets into APIs.** PCSX2 and RPCS3 are not APIs; they carry their
own expected executable and their own warnings, and RPCS3 is one of the two routes that expect no
proxy DLL because ReShade loads as a Vulkan layer. Keep them as named targets that resolve to an
API plus guidance, or that knowledge is lost.

## What the merged tool must keep

From `installer-x86`: transactional journal with rollback, `.dlss5-x86bridge-backups/`, the install
manifest with ownership and hashes, preservation of user config and of files modified after install,
`[INSTALL] BasePath` resolution confined to the game directory, fail-closed handling of unknown D3D8
wrappers, and the PE machine check.

From `installer/`: preflight for writable directory, locked files, free space and add-ons disabled
in `ReShade.ini`; the `pass2..pass10` sweep from the old one-runtime-per-pass layout; the failure
log written beside the executable; and the emulator presets with their guidance.

The union is the requirement. A merge that drops either side's safety is not done.

## Sequence

1. Port `core.h` into Rust as the single install engine — manifest, backups, journal, ownership,
   safe path handling, PE check, BasePath. Port `tests.cpp` alongside it; keep the existing Rust
   tests passing untouched.
2. Move the x64 install path onto that engine, so both routes produce a manifest and backups. This
   is the step that upgrades x64 safety, and it is where regressions would hide: the x64 route has
   never written a manifest before.
3. Fold the preflight in front of both routes.
4. Replace the preset list with detected bitness plus the reduced API list, keeping PCSX2 and RPCS3
   as named targets.
5. Retire `installer-x86/`, and keep one release artifact.

Steps 1 to 3 are invisible to the user and independently verifiable. Only step 4 changes the flow,
which is the right order: the risky part ships last, on top of an engine already proven.

## Out of scope

Restoring the Win32 GUI, import-table API autodetection, and installing ReShade by download. The
first is deferred by decision 3, the second was rejected on merit, and the third stays refused.
