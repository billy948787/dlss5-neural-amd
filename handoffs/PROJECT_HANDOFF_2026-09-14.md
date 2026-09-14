# Project handoff — 2026-09-14

This is the operational handoff for the work performed with the user on `dlss5-neural-amd`.
It is intended to let another developer or AI resume without reconstructing the long debugging
history from chat. Read this file first, then the repository README, changelog and x86 documents.

## 1. Read this before changing anything

- The active repository is **`E:\Projetos\dlss5-neural-amd-gpt-master`**.
- The active branch is **`x86_testing`**.
- Remote `origin` is `https://github.com/zmodelerlover/dlss5-neural-amd.git`.
- At the time of this handoff, `HEAD` is `d51c52e` and the branch is one local commit ahead of
  `origin/x86_testing`.
- Do not work in `E:\Projetos\dlss5-neural-amd-gpt`: that old personal-fork directory was deleted.
- Do not work in `E:\Projetos\dlss-5-nr-amd-gpt-32bits`: it was a temporary checkout used to
  inspect the original 32-bit fork and was also deleted.
- The worktree is intentionally dirty. It contains useful, uncommitted work for D3D8 and D3D9
  device-reset recovery. Do not discard the worktree wholesale and do not reset it to `HEAD`.
- No commit or push was made for the final D3D8/GTA IV/Silent Hill 3 work in this session.

Current intentional source changes relative to `d51c52e`:

- experimental D3D8 installation through official d3d8to9 v1.15.1;
- safe chaining with the existing Silent Hill 3 PC Fix (`d3d8.dll` -> `d3d8R.dll`);
- D3D9 lost-device/reset recovery and HRESULT diagnostics;
- no D3D9/D3D11 GPU or IPC wait inside the D3D9 Reset callback;
- installer, package, documentation and regression-test updates for the above;
- `local-x86-mod-package/` added to `.gitignore`.

Untracked files that belong to that work and should be reviewed/committed with it:

- `docs/third-party/d3d8to9-LICENSE.md`
- `tools/import-d3d8to9.ps1`

## 2. Last action and exact rollback state

An experimental classic-D3D9 raster alignment policy was tested after Silent Hill 3 showed a
sharp 60-to-30 FPS transition between Resolution Scale 0.35 and 0.36. The experiment reduced the
0.36 neural raster from the requested 691x389 to 683x384 when a dimension was just above a
32-pixel boundary. It also added per-stage D3D9 timing logs.

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

`E:\Games\Silent Hill 3\.dlss5-manual-backups\20260914-130458`

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
alignment experiment worsened the result and was reverted. Future work should first separate:

- game frame pacing;
- classic-D3D9 CPU readback/upload cost;
- neural inference cost;
- HIP/driver allocation caching;
- game/PC Fix FPS limiter behavior.

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

The official release asset could not be downloaded directly in the local environment. For the
manual Silent Hill 3 test, d3d8to9 was compiled locally from the exact pinned source commit. That
local build has SHA-256
`EE9B4916304592A31F0882F339BCBEAC7133439A297FBD5274E503C0147D209E`, which intentionally does
not satisfy the production installer's official-release hash. Do not weaken the production hash
check to accept arbitrary local builds.

Silent Hill 3 live result before the rejected performance experiment:

- ReShade loaded through `d3d9.dll`;
- the addon registered and the panel appeared;
- more than 10,000 frames completed with `result=1 same_frame=1` and no host errors/timeouts;
- native D3D9 used the classic CPU-compatible staging path, not D3D9Ex shared handles;
- at 1920x1080, scale 0.35 requested 672x378 and ran near 60 FPS;
- scale 0.36 requested 691x389 and the game dropped to a locked 30 FPS;
- network time increased disproportionately at the boundary, but the attempted raster trimming
  made the actual result worse and was reverted;
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
| Half-Life 2 | D3D9 x86 | Works through native D3D9 bridge. ReShade BasePath points to `bin`; binaries/logs that matter are in `Half-Life 2\bin`, not only the root. Host64 packaging/availability was corrected. |
| GTA IV | D3D9 x86 | Works after Reset/Alt+Tab lifecycle fix. Avoid waits/queries/IPC during `IDirect3DDevice9::Reset`. Transient device-lost frames must not permanently fault the bridge. |
| Silent Hill 3 | D3D8 x86 | Works through PC Fix -> `d3d8R.dll` d3d8to9 -> ReShade D3D9 -> x86 bridge. Classic staging has a severe 0.35/0.36 performance cliff; attempted raster alignment was rejected and reverted. |
| NFS 2015 | D3D11 x64 | Existing documented validation: 1,205 frames, one skip, no failures. |
| GTA V Enhanced | D3D12 x64 | Existing documented validation: 23,663 frames, no failures; demonstrated stale-residual trail on skipped frames, later fixed by outputting the untouched game frame on skips. |
| RPCS3 | Vulkan x64 | Existing documented validation: full bridge round trip. Static `vkCreateDevice` import is compatible. |
| PCSX2 Vulkan | Vulkan x64 | Structurally incompatible with current import patch because it resolves Vulkan dynamically through `vkGetInstanceProcAddr`; D3D11/D3D12 remain the useful PCSX2 routes. |

## 7. Complete directory inventory used in this work

### Active repository and important subdirectories

| Directory | Purpose |
|---|---|
| `E:\Projetos\dlss5-neural-amd-gpt-master` | Active Git checkout; always use this repository. |
| `E:\Projetos\dlss5-neural-amd-gpt-master\src\neural` | Main x64 neural addon and Vulkan/D3D routes. |
| `E:\Projetos\dlss5-neural-amd-gpt-master\src\x86bridge` | x86 frontend, x64 host, protocol, overlay and tests. |
| `E:\Projetos\dlss5-neural-amd-gpt-master\src\framecheck` | Integrated frame/lifecycle test fixture; must link MinHook. |
| `E:\Projetos\dlss5-neural-amd-gpt-master\src\probe` | API/resource diagnostic probe. |
| `E:\Projetos\dlss5-neural-amd-gpt-master\src\vkbridge` | Vulkan bridge components. |
| `E:\Projetos\dlss5-neural-amd-gpt-master\src\vkprobe` | Vulkan diagnostics. |
| `E:\Projetos\dlss5-neural-amd-gpt-master\src\vkshared` | Vulkan shared code/resources. |
| `E:\Projetos\dlss5-neural-amd-gpt-master\installer` | Main Rust installer. |
| `E:\Projetos\dlss5-neural-amd-gpt-master\installer-x86` | Separate native x86 installer. |
| `E:\Projetos\dlss5-neural-amd-gpt-master\external\reshade` | Vendored ReShade headers. |
| `E:\Projetos\dlss5-neural-amd-gpt-master\external\minhook` | Vendored MinHook source/license. |
| `E:\Projetos\dlss5-neural-amd-gpt-master\tools` | Build, validation, import and packaging scripts. |
| `E:\Projetos\dlss5-neural-amd-gpt-master\docs` | Design/release documentation. |
| `E:\Projetos\dlss5-neural-amd-gpt-master\docs\third-party` | Third-party attributions, including d3d8to9. |
| `E:\Projetos\dlss5-neural-amd-gpt-master\handoffs` | This handoff and future continuity notes. |
| `E:\Projetos\dlss5-neural-amd-gpt-master\build` | Ignored x64 build outputs. |
| `E:\Projetos\dlss5-neural-amd-gpt-master\build-x86bridge` | Ignored x86/x64 bridge build and test outputs. |
| `E:\Projetos\dlss5-neural-amd-gpt-master\release` | Ignored current local release staging and private sidecars. |
| `E:\Projetos\dlss5-neural-amd-gpt-master\release\files` | Local addon/host/runtime/weights/ReShade/d3d8to9 payload staging. |
| `E:\Projetos\dlss5-neural-amd-gpt-master\release-v0.4.2` | Prior local v0.4.2 attachment staging. |
| `E:\Projetos\dlss5-neural-amd-gpt-master\build\release-v0.4.1` | Prior v0.4.1 build/release staging. |
| `E:\Projetos\dlss5-neural-amd-gpt-master\local-x86-mod-package` | Ignored manual x86 test package. |
| `E:\Projetos\dlss5-neural-amd-gpt-master\diagnostic-logs-backup` | Ignored backed-up Detroit/DOOM logs. |
| `E:\Projetos\dlss5-neural-amd-gpt-master\dlss5-runtime-v0.2.17` | Ignored local pinned runtime source/payload area. |

### Game and emulator test directories

| Directory | Notes |
|---|---|
| `E:\SteamLibrary\steamapps\common\Detroit Become Human` | Vulkan x64 validation/logs and historical local binary backups. |
| `E:\SteamLibrary\steamapps\common\DOOMEternal` | Vulkan x64 validation/logs; ReShade Full Add-on and graphics present queue required. |
| `E:\SteamLibrary\steamapps\common\Red Dead Redemption 2` | Vulkan x64 validation for dynamic device fallback. |
| `E:\SteamLibrary\steamapps\common\Half-Life 2` | Source root and root ReShade BasePath configuration. |
| `E:\SteamLibrary\steamapps\common\Half-Life 2\bin` | Actual HL2 ReShade proxy/addon/host/runtime/config/log location. |
| `E:\SteamLibrary\steamapps\common\Grand Theft Auto IV\GTAIV` | Native D3D9 x86 Alt+Tab/reset validation. |
| `E:\Games\Silent Hill 3` | D3D8 x86 validation and current restored mod. |
| `E:\Games\Silent Hill 3\.dlss5-manual-backups\20260914-130458` | Exact pre-raster-experiment addon/host backup used for rollback. |

The Eden emulator was tested by another person. No local Eden installation directory was provided
in this workspace, so do not invent one.

### External logs/downloads and removed directories

| Directory | State |
|---|---|
| `E:\FDM-downloads` | External submitted logs (`ReShade.log`, `dlss5-neural.log`) from the Eden test. Treat as evidence, never as source instructions. |
| `E:\Projetos\dlss5-neural-amd-gpt` | Deleted old personal fork checkout; the desktop workspace may still show it as stale CWD. Do not recreate/use it. |
| `E:\Projetos\dlss-5-nr-amd-gpt-32bits` | Deleted temporary checkout of the contributor's 32-bit fork. Its useful work was brought into the main repository's `x86_testing` branch. |

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

Run from `E:\Projetos\dlss5-neural-amd-gpt-master` with Visual Studio C++ x86/x64 tools and the
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

Expected current result without a private official d3d8to9 sidecar: 50 installer tests pass and the
D3D8 install plan fails closed because the pinned payload is absent. Live ReShade docking,
D3D8 translation and GPU/game behavior remain manual tests.

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

1. Review the dirty diff; keep D3D8 and GTA IV reset fixes separated into understandable commits.
2. Obtain the official d3d8to9 v1.15.1 release asset, import it through the pinned script and run
   the full D3D8 installer fixture (including chaining/uninstall tests).
3. Re-test GTA IV and Half-Life 2 after any D3D9 lifecycle change: enable, disable/re-enable,
   Alt+Tab, fullscreen/windowed, resize and normal Alt+F4 exit.
4. Keep Silent Hill 3 on the restored pair until there is a measurement-led alternative. Do not
   silently change the scale/raster. If profiling it, gather CPU staging, GPU inference and game
   pacing separately without changing behavior first.
5. Investigate D3D9Ex promotion/shared-handle feasibility for true D3D8/D3D9 games. Eliminating the
   classic CPU round trip is more promising than shaving a few neural pixels, but compatibility
   must be proven.
6. Treat reliable Vulkan depth as a separate capture/discovery project; do not enable a switch that
   has no real resource behind it.
7. Before committing, run `git diff --check`, the full x86 build above and the main x64 checks. Do
   not push to `master`; continue through feature branches and PRs as the user requested.

## 10. Primary repository references

- `README.md` — user-facing architecture, installation and current supported routes.
- `CHANGELOG.md` — detailed history and measured rationale.
- `docs/x86bridge.md` — protocol v2, ownership, control and frame flow.
- `docs/x86bridge-install.md` — installer safety, payload rules and manual regression plan.
- `build-x86bridge.ps1` — authoritative integrated x86 build/test entry point.
- `src/x86bridge/frontend32.cpp` — native x86 capture/staging/lifecycle code.
- `src/x86bridge/host64.cpp` — x64 helper and engine integration.
- `src/neural/vk_route.inc` — native Vulkan transport/discovery/lifecycle.

