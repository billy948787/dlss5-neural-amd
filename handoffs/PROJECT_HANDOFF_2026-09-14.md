# Project handoff — 2026-09-14

This is the operational handoff for the work performed with the user on `dlss5-neural-amd`.
It is intended to let another developer or AI resume without reconstructing the long debugging
history from chat. Read this file first, then the repository README, changelog and x86 documents.

## 1. Read this before changing anything

- The active branch is **`x86_testing`**.
- Remote `origin` is `https://github.com/zmodelerlover/dlss5-neural-amd.git`.
- Every path in this document is relative: repository paths to the checkout root, game paths to
  wherever that game is installed. Keep a single checkout of this repository and work only in it.

**Updated later on 2026-09-14.** The work this handoff was written to preserve is committed and
pushed. The worktree is clean and `x86_testing` matches `origin/x86_testing`; the dirty tree the
original text described was separated into these commits, in this order:

| Commit | Contents |
|---|---|
| `b324d57` | D3D9 `Reset`/lost-device recovery and HRESULT diagnostics: the GTA IV Alt+Tab fix |
| `0346842` | experimental D3D8 preset through pinned d3d8to9, with `d3d8R.dll` chaining, licence and import script |
| `8515a57` | ignore `local-x86-mod-package/` |
| `5701dc0` | this handoff |
| `e6d4ddb` | ignore-rule audit: `release-v*/`, `*.zip`, installer manifests and backups, editor noise |
| `cfdd1af` | stop ignoring `tools/patch_runtime.py` and `installer/Cargo.lock`, which are not artifacts |
| `6b273e8` | opt-in stage probe behind `DLSS5_X86BRIDGE_TIMING=1` |

Do not push to `master`; continue through feature branches and PRs.

## 2. Last action and exact rollback state

An experimental classic-D3D9 raster alignment policy was tested after Silent Hill 3 showed a
sharp 60-to-30 FPS transition between Resolution Scale 0.35 and 0.36. The experiment reduced the
0.36 neural raster from the requested 691x389 to 683x384 when a dimension was just above a
32-pixel boundary. It also added per-stage D3D9 timing logs.

**The transition it was chasing has since been explained, and it was not a rendering boundary at
all.** See section 4.2. Resolving it required no change to this project's code.

The user reported that this made the result worse. That experiment was fully removed from source.
Do not reintroduce any of these identifiers or behaviors without new evidence:

- `FrameFlagClassicD3D9`
- `EfficientClassicD3D9Scale`
- `x86bridge D3D9 timing avg`
- automatic trimming/quantization/alignment of the user's requested neural raster

The code was rebuilt after the removal. Native x86/x64 protocol and IPC tests passed, all 50
installer tests passed, and the integrated addon64 build passed.

Silent Hill 3 was restored to the exact binaries used before that experiment:

| File | Restored SHA-256 |
|---|---|
| `dlss5-neural.addon32` | `4A9401CAE8CC8573887915821C74E00E240E3B41040CC203C53FDE8E13F0DD82` |
| `dlss5-neural-host64.exe` | `29284B5F62C44C9C585BDFD2ED461FB4A45ED3A09593A29BEAD3613A3585C184` |
| existing PC Fix `d3d8.dll` | `4D35F4EE85C63FFE012731DC1A4E8EEE2D8D34EBF6384076044E79BE1A479F13` |

The recoverable copy is at:

`.dlss5-manual-backups\20260914-130458`, inside the Silent Hill 3 game directory.

The rebuild refreshed ignored local build/release outputs, but the game intentionally remains on
the restored pre-experiment binary pair until a better performance change is validated.

## 3. Project architecture learned during the work

### 3.1 Native x64 add-on

The normal `dlss5-neural.addon64` is a ReShade add-on that runs the AMD port of the DLSS neural
rendering network. It uses the pinned v0.2.17 runtime and pass-1 weights/runtime. Pass 2 and pass 3
DLLs are not used. The principal implementation is under `src/neural/`.

The add-on has D3D11, D3D12 and Vulkan paths. The single packaged x64 add-on includes the Vulkan
transport; there is no longer a separate Vulkan binary. Vulkan hooks stand down on processes that
do not expose the required imports.

All packaged game-facing binaries use the static MSVC runtime (`/MT`). This matters because some
games, especially Detroit: Become Human, ship old private copies of MSVC runtime DLLs beside the
executable. A `/MD` add-on can register in ReShade but fail during `DllMain` against those copies.

### 3.2 Vulkan route

The Vulkan route crosses the presented image into a private D3D12 device where the existing neural
engine runs. It requires device creation interception so the needed external-memory extensions are
enabled before `VkDevice` creation.

There are two discovery paths:

- normal ReShade/import-table path;
- dynamic device-discovery fallback using vendored MinHook for applications such as Red Dead
  Redemption 2.

Important behavior:

- the presentation queue must support graphics commands;
- an async-only present queue is rejected safely;
- private Vulkan discovery preserves both `DISABLE_VK_LAYER_reshade_1` and
  `DISABLE_VK_LAYER_reshade_2` to avoid recursive ReShade entry and to support DOOM's launcher;
- D3D12 crossing textures begin in `COPY_DEST`, matching the first operation;
- poisoned allocator/command-list slots are recreated after `Reset` or `Close` failure;
- swapchain teardown fully retires imported Vulkan images and D3D12 crossings, even if dimensions
  and format are unchanged;
- fast-path reuse requires all imported images and D3D12 resources to still be live;
- process detach must not remove Vulkan hooks while Windows holds the loader lock.

DOOM Eternal normally presents from an async-only queue. `r_presentFromAsync "0"` tells the game
to present from a graphics-capable queue that the bridge can use. This is what the earlier
`presentFromAsync` discussion referred to.

### 3.3 Experimental x86 bridge

A 32-bit game cannot load the 64-bit HIP/runtime stack directly. The x86 implementation is split:

- `dlss5-neural.addon32`: 32-bit ReShade frontend loaded by the game;
- `dlss5-neural-host64.exe`: 64-bit helper containing/reusing the neural engine;
- a fixed-width named-pipe protocol plus shared/staged frame resources between them.

The frontend and host must always be updated as a pair. Protocol v1 is intentionally rejected by
protocol v2. The pipe validates the client PID/process identity, generation, frame sequence,
adapter LUID and settings revisions. IPC and startup waits are bounded; an unresponsive helper
falls back to the original game image instead of hanging Present indefinitely.

The x86 UI is adapted in `src/x86bridge/overlay32.inc`. It does not perform file, IPC, host launch
or GPU work from the overlay callback. Control synchronization occurs from Present. Async mode is
disabled on x86; same-frame inline behavior is the validated path.

API routes:

| Game API | x86 route |
|---|---|
| D3D11 | native D3D11 frontend -> x64 host |
| D3D9Ex | D3D9 -> shared D3D9/D3D11 GPU textures -> x64 host |
| classic D3D9 | D3D9 -> bounded CPU-compatible readback/upload staging -> D3D11 -> x64 host |
| D3D8 | d3d8to9 -> native D3D9 frontend -> one of the D3D9 paths above |

Classic D3D9 is much more expensive than D3D9Ex because a full frame crosses CPU-visible staging
in each direction. D3D8 support is a compatibility layer, not a second neural renderer.

### 3.4 Depth and motion availability

| Route | Colour | Game depth | Motion |
|---|---|---|---|
| x64 D3D11 | yes | yes, candidate captured and converted to `R32_FLOAT` | game velocity target when found, otherwise estimated |
| x64 D3D12 | yes | no reliable game depth | estimated from colour |
| x64 Vulkan | yes | no | estimated from colour |
| x86 D3D11 | yes | candidate capture exists through the frontend | game candidate when available |
| x86 D3D9/D3D8 | yes | no, colour-only transport | host estimation |

Enabling `Depth=1` cannot manufacture a missing Vulkan or D3D9 depth buffer. Flat depth and zero
motion in a menu are not proof of failure; guide contents must be observed during real gameplay.
Reliable Vulkan depth and D3D9 depth/motion discovery remain open research tasks.

## 4. Fixes and lessons already validated

### 4.1 Vulkan lifecycle and compatibility

- Fixed the Detroit registration-without-panel problem by statically linking the CRT.
- Validated present queues and command lists before use so unsupported async queues skip safely.
- Fixed immediate activation crashes caused by invalid/null Vulkan command-list paths.
- Fixed disable/re-enable and Alt+Tab cases that left stale imported images or poisoned work slots.
- Added complete swapchain resource retirement and rebuild.
- Added the dynamic Vulkan device fallback used by Red Dead Redemption 2.
- Fixed CI `framecheck` linkage by compiling the vendored MinHook sources into that fixture too.
- The Eden emulator report where the panel disappeared was not a regression: Windows Defender had
  deleted/quarantined the addon. Restoring the file made the mod work.

### 4.2 Resolution Scale and VRAM

Dragging Resolution Scale used to apply every intermediate slider value, repeatedly rebuilding the
network raster and causing unnecessary VRAM growth. The accepted mitigation commits the new scale
only after slider editing ends and retires completed work slots safely.

Repeated completed scale changes can still raise the VRAM residency reported by the driver. The
remaining growth appears consistent with AMD HIP/runtime or driver allocation caching. An
experimental forced runtime purge using an undocumented internal function did not solve it and was
not retained because it added stability risk.

Do not automatically quantize or silently alter the user's scale. The Silent Hill 3 32-pixel tile
alignment experiment worsened the result and was reverted.

**Resolved 2026-09-14: the 0.35/0.36 transition was the game's frame-rate cap, not a raster
boundary.** Silent Hill 3's PC Fix was running a hard 60 FPS cap (`Silent_Hill_3_PC_Fix.ini`,
`FPSMode` 1 or 2). Under a hard cap, missing the 16.67 ms deadline does not cost a proportional
amount of frame rate, it costs the next divisor: 60 becomes a locked 30. Scale 0.36 asks for 5.8%
more pixels than 0.35, 268,799 against 254,016, which is enough to cross that deadline and nothing
more. Setting `FPSMode = 4` (unlocked) normalised the frame rate with no change to this project's
code at all.

Two consequences:

- There is no 32-pixel boundary and nothing special about 0.36. The alignment experiment was
  solving a problem that did not exist, which is why it cost image quality and bought nothing.
  Do not attempt raster alignment, quantization or trimming again on this evidence.
- `FPSMode = 4` disables the PC Fix's own `LimitFPSInStoreroom`, which requires `FPSMode` 1 or 2.
  That patch locks the hospital storeroom (Mirror Room) to 30 FPS so its visual and audio effects
  play correctly, so uncapping to fix the bridge's frame budget breaks a later scene. `FPSMode = 1`
  (ThirteenAG's FPS patch) is worth testing as the setting that keeps both.

The cost itself remains real: the classic D3D9 route pays a fixed CPU round trip that does not
shrink with Resolution Scale. Measure before changing behaviour, in this order:

- game frame pacing and any game or PC Fix frame-rate cap -- **check this first**; it explained the
  only performance cliff ever reported here;
- classic-D3D9 CPU readback/upload cost;
- neural inference cost;
- HIP/driver allocation caching.

Section 4.5 is the tool for the middle two.

### 4.3 D3D9 Reset and Alt+Tab

GTA IV exposed a crash in exclusive-fullscreen Alt+Tab. ReShade emits `destroy_swapchain` while
`IDirect3DDevice9::Reset` is already in progress. Submitting an event query, waiting for D3D9,
waiting for D3D11 or performing IPC from that callback can re-enter `amdxx32.dll` during reset and
crash.

The current uncommitted frontend fix:

- releases local/default-pool D3D9 resources immediately during resize/reset;
- performs no `DropRemote`, query, D3D11 wait, `ClearState` or `Flush` inside that reset branch;
- defers remote generation retirement until the next stable Present/`Bridge::Ensure`;
- returns HRESULTs from D3D9 staging operations instead of collapsing all failures to `false`;
- treats `D3DERR_DEVICELOST`/`D3DERR_DEVICENOTRESET` as a transient reset frame, keeps the host
  connected and resets history on recovery;
- logs other HRESULT failures precisely and falls back safely.

This removed the GTA IV Alt+Tab crash and the later retry/error state observed with NR enabled.
Half-Life 2 continued to work after installing the same build.

### 4.4 Installer safety

The x86 installer is additive and fail-closed:

- rejects PE32+ targets;
- validates SHA-256 for the bridge, host, runtime, weights, ReShade and optional d3d8to9 payload;
- never runs a ReShade installer executable;
- follows ReShade `[INSTALL] BasePath` only when it resolves inside the selected game directory;
- backs up conflicts under `.dlss5-x86bridge-backups/`;
- records ownership/hashes/backups in `dlss5-x86bridge.install.json`;
- preserves user configuration and files modified after installation;
- restores unchanged backups during uninstall;
- fails closed on unknown D3D8 wrappers rather than overwriting them.

The supported x86 ReShade sidecar is ReShade 6.8.0.2156 Full Add-on Support with its pinned hash.
The installer writes that payload as `dxgi.dll` for D3D11 or `d3d9.dll` for D3D9/D3D8.

### 4.5 Measuring the x86 route

`DLSS5_X86BRIDGE_TIMING=1` turns on an opt-in stage probe in the x86 frontend (commit `6b273e8`).
It is off by default; the frontend's startup line reports `probe=on`/`probe=off` so a log says
which it was. Every 120 completed frames it averages one line splitting the bridge into
`input+prepare`, `host` and `output`, and names the staging path measured.

The probe measures and never participates. It issues no query, flush or wait of its own: all three
boundaries are synchronisations the frame already performs, so an enabled probe measures the same
frame that would have run without it. **Preserve that property.** A wait added on this path would
land inside the `IDirect3DDevice9::Reset` window that section 4.3 exists to keep clear. The
contract test in `tools/test-x86bridge.py` pins it and also guards that the reverted experiment's
identifiers stay absent.

**Measured on Silent Hill 3 through the real D3D8 route**, 2,160 frames at 1920x1080 with no
failures, across 18 windows of 120 frames:

| | min | max | mean | spread |
|---|---|---|---|---|
| `host` | 9.93 ms | 40.76 ms | 12.98 ms | 30.83 ms |
| transport (`input+prepare` + `output`) | 5.30 ms | 5.82 ms | 5.56 ms | **0.52 ms** |

That is the finding: **`host` swung 4.1x while transport moved half a millisecond.** The transport
cost is independent of what the network costs. The swing came from warm-up at scale 1.00 settling
into steady state at 960x540, which covered a far wider range than deliberately stepping through
Resolution Scale values would have.

Steady state at scale 0.50: `host` 9.97 ms, transport 5.55 ms, total about 15.5 ms.

So the classic path has a floor. Transport is roughly 36% of a 60 FPS budget spent only moving
pixels, it never shrinks, and lowering Resolution Scale does not touch it -- even with the network
free, this route cannot go below about 5.5 ms per frame. Half-Life 2 reaches `shared GPU staging`
and does not pay it. That is the measured case for D3D9Ex promotion over saving neural pixels.

How much of the 5.5 ms D3D9Ex actually removes is still unmeasured; the probe labels the staging
path on its own line, so a promoted Silent Hill 3 answers it directly.

### 4.6 Open: D3D9 reference count at process exit

ReShade reports `Reference count for IDirect3DDevice9 ... is inconsistent! Leaking resources` at
normal process exit on Silent Hill 3 and GTA IV, and not on Half-Life 2. No crash or minidump is
associated with it. The correlation across the three titles is clean:

| Title | Staging | Final `destroy_swapchain` observed | Reference count |
|---|---|---|---|
| Silent Hill 3 | classic CPU-compatible | no | inconsistent |
| GTA IV | classic CPU-compatible | no | inconsistent |
| Half-Life 2 | shared GPU | yes | clean |

On GTA IV the warning is timestamped two seconds before the add-on is unregistered, so ReShade
released the device while the frontend still held references. The classic path is the one that
creates `readback9` and `upload9`, two `SYSTEMMEM` surfaces the shared path never creates, which
makes them the first thing to check. A plausible fix direction is to release the D3D9 staging on
add-on unregister as well, not only in `destroy_swapchain`.

Treat this as unconfirmed. All three frontend logs end abruptly, so the missing teardown line may
be an unflushed log rather than a callback that never ran; the reference-count warning itself comes
from ReShade and is independent of that.

## 5. D3D8 and Silent Hill 3 details

The selected design is:

`SilentHill3.exe -> existing PC Fix d3d8.dll -> d3d8R.dll (d3d8to9) -> d3d9.dll (ReShade x86) -> dlss5-neural.addon32 -> dlss5-neural-host64.exe`

Why `d3d8R.dll`:

- Silent Hill 3 already has a PC Fix as `d3d8.dll`;
- inspection found that this wrapper explicitly searches for/forwards to `d3d8R.dll`;
- replacing it would break the PC Fix;
- the installer now recognizes only an explicit ASCII or UTF-16 `d3d8R.dll` marker and otherwise
  refuses to replace an unknown wrapper.

Production installer expectations:

- official d3d8to9 version: v1.15.1;
- source commit: `65870f2302e9c496cd6d873d6095961d5c777668`;
- official release asset SHA-256:
  `ab6bf7a9a9f4b3e66a75ca038d8d10289c88acbfe8d52c3b5a8a9a259cb26cd5`;
- imported private filename: `release/files/d3d8to9.dll`;
- import script: `tools/import-d3d8to9.ps1`;
- license: `docs/third-party/d3d8to9-LICENSE.md`;
- d3d8to9 may require the legacy `d3dx9_43.dll` DirectX runtime.

**The official release asset has since been imported.** It was fetched from the upstream release,
verified as a 124,416-byte PE32/i386 image whose SHA-256 matches the pinned constant, and imported
through `tools/import-d3d8to9.ps1`. It now sits at `release/files/d3d8to9.dll`, which is git-ignored:
the payload stays out of the repository and out of public packages, and only the BSD-2-Clause
attribution is tracked.

The earlier manual Silent Hill 3 test used a d3d8to9 compiled locally from the pinned source commit.
That local build has SHA-256
`EE9B4916304592A31F0882F339BCBEAC7133439A297FBD5274E503C0147D209E`, which intentionally does not
satisfy the production installer's official-release hash, and it is still what the game directory
carries as `d3d8R.dll`. Installing through the installer replaces it with the official binary. Do
not weaken the production hash check to accept arbitrary local builds.

**Installed through the installer and verified live.** The D3D8 preset was run against the real
game with the official translator present. A read-only `plan()` rehearsal first confirmed the file
map, then the install ran and was checked by hash:

- the PC Fix `d3d8.dll` was left byte-identical, chosen by the chaining rule reading the real
  wrapper rather than a synthetic one;
- the official translator replaced the locally built `d3d8R.dll`;
- ReShade, runtime and weights reported `IDENTICAL` and were not rewritten;
- `dlss5-neural.ini` and `ReShade.ini` were preserved;
- the manifest records the replaced files as owned, including the pre-experiment pair
  `4a9401ca`/`29284b5f`, so that rollback state is now held by the installer's own backup scheme.

The game then ran 2,160 frames with `LUID MATCH`, `probe=on` and no failures, confirming the whole
chain `sh3.exe -> PC Fix d3d8.dll -> official d3d8R.dll -> ReShade d3d9.dll -> addon32 -> host64`
in the configuration a user would actually receive. Note the installer executable is `wWinMain`
only; its Install button calls `app.install(target, preset)` and that call is what was exercised,
so the GUI itself remains an unvalidated manual test.

Silent Hill 3 live result before the rejected performance experiment:

- ReShade loaded through `d3d9.dll`;
- the addon registered and the panel appeared;
- more than 10,000 frames completed with `result=1 same_frame=1` and no host errors/timeouts;
- native D3D9 used the classic CPU-compatible staging path, not D3D9Ex shared handles;
- at 1920x1080, scale 0.35 requested 672x378 and ran near 60 FPS;
- scale 0.36 requested 691x389 and the game dropped to a locked 30 FPS, later traced to the PC Fix
  frame-rate cap rather than to anything about the raster; see 4.2;
- the attempted raster trimming made the actual result worse and was reverted;
- one startup `IDirect3DDevice9::Reset` returned `D3DERR_INVALIDCALL`, immediately retried and
  recovered;
- ReShade reported an inconsistent D3D9 reference count at normal process exit; no crash/minidump
  was associated with it.

## 6. Live test matrix

| Application | API | Result and key lesson |
|---|---|---|
| Detroit: Become Human | Vulkan x64 | Stable after `/MT` and Vulkan lifecycle fixes; 9,240-frame validation, repeated toggles and two rebuilds, no skips. Later regression check also passed. |
| DOOM Eternal | Vulkan x64 | Works with proper ReShade Full Add-on installation and graphics present queue; use `r_presentFromAsync "0"`; 3,600-frame validation, repeated Alt+Tab, one recovered skip. |
| Red Dead Redemption 2 | Vulkan x64 | Initially panel appeared but effect did not activate; dynamic Vulkan device fallback/MinHook route fixed it. User confirmed correct operation. |
| Eden Nintendo Switch emulator | Vulkan x64 | Works. Missing panel was Windows Defender quarantining/deleting the addon, not a code regression. |
| Half-Life 2 | D3D9 x86 | Works through native D3D9 bridge. ReShade BasePath points to `bin`; binaries/logs that matter are in `Half-Life 2\bin`, not only the root. Host64 packaging/availability was corrected. Revalidated on the committed tree: 21,600 frames, every frame `result=1`, no faults, one reset handled. Uses `shared GPU staging`, so it exercises the D3D9Ex path the other two titles do not. |
| GTA IV | D3D9 x86 | Works after Reset/Alt+Tab lifecycle fix. Avoid waits/queries/IPC during `IDirect3DDevice9::Reset`. Transient device-lost frames must not permanently fault the bridge. Revalidated on the committed tree: 15,480 frames, every frame `result=1`, no faults, 20 disable/enable cycles, one reset handled by the deferred path. Classic CPU-compatible staging. |
| Silent Hill 3 | D3D8 x86 | Works through PC Fix -> `d3d8R.dll` d3d8to9 -> ReShade D3D9 -> x86 bridge. The reported 0.35/0.36 cliff was the PC Fix frame-rate cap, not the bridge: unlocking it (`FPSMode = 4`) normalised the frame rate with no code change. Attempted raster alignment was rejected and reverted. See 4.2, including what `FPSMode = 4` costs in the Mirror Room. Installed through the installer with the official pinned translator and revalidated: 2,160 frames, no failures, PC Fix preserved. Stage-probe figures in 4.5. |
| NFS 2015 | D3D11 x64 | Existing documented validation: 1,205 frames, one skip, no failures. |
| GTA V Enhanced | D3D12 x64 | Existing documented validation: 23,663 frames, no failures; demonstrated stale-residual trail on skipped frames, later fixed by outputting the untouched game frame on skips. |
| RPCS3 | Vulkan x64 | Existing documented validation: full bridge round trip. Static `vkCreateDevice` import is compatible. |
| PCSX2 Vulkan | Vulkan x64 | Structurally incompatible with current import patch because it resolves Vulkan dynamically through `vkGetInstanceProcAddr`; D3D11/D3D12 remain the useful PCSX2 routes. |

## 7. Complete directory inventory used in this work

### Repository layout

Paths are relative to the repository root.

| Directory | Purpose |
|---|---|
| `src/neural` | Main x64 neural addon and Vulkan/D3D routes. |
| `src/x86bridge` | x86 frontend, x64 host, protocol, overlay and tests. |
| `src/framecheck` | Integrated frame/lifecycle test fixture; must link MinHook. |
| `src/probe` | API/resource diagnostic probe. |
| `src/vkbridge` | Vulkan bridge components. |
| `src/vkprobe` | Vulkan diagnostics. |
| `src/vkshared` | Vulkan shared code/resources. |
| `installer` | Main Rust installer. |
| `installer-x86` | Separate native x86 installer. |
| `external/reshade` | Vendored ReShade headers. |
| `external/minhook` | Vendored MinHook source/license. |
| `tools` | Build, validation, import and packaging scripts. |
| `docs` | Design/release documentation. |
| `docs/third-party` | Third-party attributions, including d3d8to9. |
| `handoffs` | This handoff and future continuity notes. |

These are produced locally and are all git-ignored; none of them ship in a clone.

| Directory | Purpose |
|---|---|
| `build` | x64 build outputs. |
| `build-x86bridge` | x86/x64 bridge build and test outputs. |
| `release` | Current local release staging and private sidecars. |
| `release/files` | Addon/host/runtime/weights/ReShade/d3d8to9 payload staging. |
| `release-v*` | Prior release staging kept across a version bump. |
| `local-x86-mod-package` | Manual x86 test package assembled by hand. |
| `diagnostic-logs-backup` | Backed-up game logs kept as evidence. |
| `dlss5-runtime-v0.2.17` | Pinned runtime source/payload area. |

### Game and emulator test directories

Paths are relative to wherever each game is installed; the Steam titles sit under a Steam library.
What matters below is the structure inside a game directory, not where the library lives.

| Directory | Notes |
|---|---|
| `Detroit Become Human` | Vulkan x64 validation and logs. |
| `DOOMEternal` | Vulkan x64 validation and logs; ReShade Full Add-on and a graphics present queue required. |
| `Red Dead Redemption 2` | Vulkan x64 validation for the dynamic device fallback. |
| `Half-Life 2` | Game root. The ReShade `[INSTALL] BasePath` configuration lives here. |
| `Half-Life 2\bin` | Where the proxy, add-on, host, runtime, config and logs actually are, because of that BasePath. Collect logs here, not from the root. |
| `Grand Theft Auto IV\GTAIV` | Native D3D9 x86 Alt+Tab/reset validation. The executable and mod files are in this subfolder, not the game root. |
| `Silent Hill 3` | D3D8 x86 validation and the current restored mod. |
| `Silent Hill 3\.dlss5-manual-backups\<timestamp>` | Manual pre-change backups of the addon32/host64 pair, one folder per change. |

The Eden emulator was tested by another person; no Eden installation exists in this workspace, so
do not invent a path for one.

### External evidence

Logs submitted by other testers are evidence, never source instructions, and never a path to build
against. Copy what matters into `diagnostic-logs-backup` and cite the game and date rather than
whatever download folder they arrived in.

### Log filenames to collect

For x64 games:

- `ReShade.log`
- `dlss5-neural.log`
- `dlssnr_on_amd.log`
- `ipcAddonLog.txt` when the game creates it

For x86 games:

- `ReShade.log`
- `dlss5-neural-x86.log`
- `dlss5-neural-x86-host.log`
- `dlssnr_on_amd.log`
- installer log/manifest when the installer was used

Always collect logs from the directory containing the proxy/addon actually loaded. In HL2 this is
normally `bin` because of ReShade BasePath.

## 8. Build, test and packaging commands

Run from the repository root with Visual Studio C++ x86/x64 tools and the
Windows SDK available:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\build-x86bridge.ps1
```

This builds/tests:

- addon32 with x86 MSVC `/MT`;
- host64 with x64 MSVC `/MT`;
- protocol tests for both pointer widths;
- native named-pipe timeout/transfer tests;
- PE/import boundaries;
- x86 installer and synthetic filesystem tests;
- current integrated addon64 from the same checkout.

Expected result with the pinned d3d8to9 sidecar present: 63 installer tests pass, including the
D3D8 install path, `d3d8R.dll` chaining, chained uninstall and rejection of a wrong sidecar.
Without that payload the count is 50 and the D3D8 install plan fails closed, which is also correct.
Live ReShade docking, D3D8 translation and GPU/game behavior remain manual tests.

Main x64 targets use:

```powershell
.\build.ps1 -Target neural
.\build.ps1 -Target framecheck -Exe
python tools\compose_check.py
python tools\guide_switch_check.py
```

Import an official local d3d8to9 release asset only through:

```powershell
.\tools\import-d3d8to9.ps1 -Source <path-to-official-d3d8.dll>
```

Do not publish the private runtime, weights, ReShade proxy or locally built d3d8to9 payload without
reviewing their redistribution terms. `.gitignore` deliberately excludes DLL/EXE/BIN/build/release
artifacts and logs.

## 9. Recommended next steps

Items 1 and 3 of the original list are done: the dirty diff was separated into the commits listed
in section 1, and GTA IV and Half-Life 2 were revalidated on the committed tree (section 6).

1. Exercise the D3D8 uninstall live. Installation is now validated (section 5), but the uninstall
   path -- restoring the locally built `d3d8R.dll` from the installer's backup and leaving the PC
   Fix `d3d8.dll` alone -- has only been proven against synthetic files.
2. Investigate D3D9Ex promotion/shared-handle feasibility for true D3D8/D3D9 games. This is now the
   highest-value performance item: section 4.5 puts about 6 ms of fixed CPU round trip on the
   classic path, Half-Life 2 already proves the shared path works, and no amount of neural-pixel
   shaving reaches that cost. Compatibility must still be proven, and for D3D8 it depends on what
   d3d8to9 creates.
3. Quantify before optimising. With `DLSS5_X86BRIDGE_TIMING=1` and no frame-rate cap in the way,
   run one title at several Resolution Scales: `input+prepare` and `output` should stay flat while
   only `host` grows. That turns the D3D9Ex decision into a number.
4. Do not silently change the user's scale or raster. Section 4.2 is the record of why.
5. Confirm or dismiss the D3D9 reference-count observation in section 4.6.
6. Treat reliable Vulkan depth as a separate capture/discovery project; do not enable a switch that
   has no real resource behind it.
7. Before committing, run `git diff --check`, the full x86 build above and the main x64 checks. Do
   not push to `master`; continue through feature branches and PRs as the user requested.

## 10. Primary repository references

- `README.md` — user-facing architecture, installation and current supported routes.
- `CHANGELOG.md` — detailed history and measured rationale.
- `docs/x86bridge.md` — protocol v2, ownership, control and frame flow.
- `docs/x86bridge-install.md` — installer safety, payload rules and manual regression plan.
- `docs/installer-merge.md` — decisions for folding the two installers into one; not implemented.
- `build-x86bridge.ps1` — authoritative integrated x86 build/test entry point.
- `src/x86bridge/frontend32.cpp` — native x86 capture/staging/lifecycle code.
- `src/x86bridge/host64.cpp` — x64 helper and engine integration.
- `src/neural/vk_route.inc` — native Vulkan transport/discovery/lifecycle.

