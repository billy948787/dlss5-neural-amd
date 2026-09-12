# x86bridge overlay, protocol v2

Generic native D3D11 x86 frontend to the original x64 neural engine. This source update adds the ReShade panel **DLSS Neural Rendering (AMD)**. The 55 upstream files remain byte-identical. Previous source ZIP remains intact (SHA256 2898160feb1e153869103f9c0a23fdfcf8355bd7451b874efedd5565b54a405f).

## Build and install

On Windows with Visual Studio C++ x86/x64 tools and Windows SDK, from this folder:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\build-x86bridge.ps1
```

The existing script invokes native `Hostx64\x86\cl.exe` and `Hostx64\x64\cl.exe`, C++20, `/MT`; runs protocol tests for both architectures, checks PE/imports, and builds the original addon64 only in a temporary copy. Output:

- `build-x86bridge/dlss5-neural.addon32` (x86)
- `build-x86bridge/dlss5-neural-host64.exe` (x64)

With the application closed, copy **both** files beside its executable, replacing the v1 pair together. Keep existing ReShade x86, `dlss5-neural.ini`, runtime DLLs, weights and runtime directories in place. Do not mix protocol versions. No installer or wrapper changes.

Open ReShade and the DLSS Neural Rendering (AMD) panel. Initial settings come from the host's original LoadSettings, not frontend defaults. If the effect starts disabled, opening the panel requests synchronization through the next D3D11 Present and the existing helper launch path.

This executor has no Windows/MSVC. This delivery contains source, not newly built Windows binaries. Native build, imports and live overlay are **UNVALIDATED** here; no old v1 binary is presented as v2. Run the above command locally.

## Changes and ownership

| State | Owner / behavior |
|---|---|
| Enabled, StartOn, ToggleKey/Mods, DisableOnAltTab, Language | Frontend shadow; operational behavior changes locally. Mirrored into host before Save. |
| Engine/tuning settings | Original host `g.*` atomics. No parallel engine configuration. |
| Save | Pending flag; Present first sends latest SET_STATE, then host executes original SaveSettings. |
| Reload | Host executes original LoadSettings, forces inline for x86, increments revision and returns complete snapshot. Frontend replaces shadow, including hotkey/language/startup/enabled values. |
| History | Only a changed History switch clears original `g.historyValid`; no universal slider reset. |
| Residual measurement | One-shot command with monotonic ID; duplicates/unknown commands rejected. |
| Dynamic status | Sampled host counters/readiness/resolutions/guide probes, plus frontend candidate and last-capture validity. |

`overlay32.inc` adapts the original UI in a separate namespace. It uses local Cells to retain the original widgets, English/Portuguese translations, help, Known tags and risk colors. Original game examples are generalized; profile UI and stale-output claims are not copied. A nonblocking try-lock avoids waiting on Present from ImGui; while busy, a short status replaces the panel for that invocation. No ReadFile/WriteFile/Request/host launch/GPU wait occurs in the overlay or its local helpers.

## Controls ported

- Top: Enabled; Enabled from the first frame; Disable on alt-tab; native virtual-key capture (Ctrl/Alt/Shift, Esc cancels); Language.
- Image: Encoding; Diffuse White; Overall Intensity; Composition; Colour Strength; Highlight Guard; Guard follows Pass Count; Residual Limit; Edge Fade; Structure Intensity; Skin Structure Strength.
- Performance: Timing; Resolution Scale; Pass Count; Taper later passes; all three per-pass overrides; Bicubic Residual Upsample.
- Guides: Read Guides From The Game; Depth; History; Motion Vectors; Motion Scale; Flow Contrast Gate; Flow Accept Ratio; actual host probes.
- Debug: Debug View; Measure Residual Again.
- Engine: Character Mask; Temporal; Tonemap; Tone Channels; Engine Scale; Reset to 1/32; original informational Model A/B/C text.
- Advanced: Local Tone Strength; read-only startup diagnostics.
- Experimental: Network Output, still strictly current-frame on this bridge.
- Status: connected/enabled/engine status, successful processed/skipped requests, dimensions, loaded/active passes, actual guide state, frontend candidates, protocol and transport mode.
- Save Settings; Reload Settings; original warning/tag legend.

Timing always shows Same frame (inline). Async is disabled with an x86-specific note, and host startup/reload force `g.inlineMode=true`, logging any Inline=0 override. Encoding and Tonemap retain restart warnings. Stage/Events/NoBridge/NoBackBuffer remain read-only INI diagnostics; no unsafe engine reinitialization is added. Transport-only keeps result=4 and disables engine widgets; control synchronization does not load HIP.

Incoming finite values clamp to widget ranges; non-finite values reject the whole update. Settings/revision and command checks live in `control_state.h`; exact field/range mapping is in `settings_fields.inc`. Initial host snapshot does not normalize or write the existing INI merely because the UI opened.

## Protocol v2

Packed fixed-width little-endian Windows wire data; uint32_t booleans. No COM pointers, HANDLE, size_t or native bool fields. Every struct has size/offset and standard-layout/trivially-copyable assertions.

| Struct | Bytes | Checked offset(s) |
|---|---:|---|
| Header | 16 | bytes=12 |
| Hello | 16 | luidHigh=8 |
| Texture | 24 | handle=16 |
| Build | 104 | motion=80 |
| Frame | 32 | resetHistory=24 |
| Ack | 48 | generation=24, luidHigh=44 |
| WireSettings | 204 | passOverride=156 |
| WireCommand | 16 | code=8 |
| WireStatus | 108 | depthMin=72 |
| StateSnapshot | 312 | status=204 |

Existing kinds 1..5 remain HELLO/BUILD/FRAME/DROP/QUIT. New kinds 6..11: GET_STATE, SET_STATE, SAVE_SETTINGS, RELOAD_SETTINGS, COMMAND, STATUS. SET_STATE body=204; COMMAND body=16; other controls have no request body. Every control response is the existing 48-byte Ack followed by a fixed 312-byte StateSnapshot, including rejected controls; FRAME replies remain exactly the original Ack. Status uses enum/flags/numbers, not strings. Protocol 1 rejects explicitly through header validation; no accidental compatibility.

Revision starts at 1 per helper session. SET_STATE must have a strictly greater revision; stale/equal/malformed updates are rejected without engine mutation. Reload increments the host revision. Measurement IDs must strictly increase in the session and cannot replay. Pending one-shots are cleared on host loss. No retry can replay a command implicitly.

Only OnPresent calls SyncControls: GET_STATE after HELLO, SET_STATE for dirty revision, pending Save/Reload/Measure, periodic STATUS (250 ms), then the existing frame path. This all runs under the existing frontend lock. Existing DROP/QUIT remain serialized lifecycle operations under that same lock. No worker thread or second launch path is introduced.

Frame order is unchanged: capture -> D3D11 FlushAndWait -> FRAME -> original host engine -> WaitForWorkQueue -> ACK -> confirmed same frame copied back. Host Neural/CopyOnly and frontend capture-to-return blocks compare byte-for-byte with the prior contribution. No async, cached output or stale-output fallback; no guide/copy/handle heuristic changes. No game-specific added code.

## Local regression test

1. Compile both new binaries and install the pair. Open ReShade panel; confirm Protocol v2, LUID MATCH in log and `result=1 same_frame=1` when NR completes.
2. Change intensity/structure/scale and History individually. Verify visible changes without host restart. Rebind hotkey, test enable/off and alt-tab.
3. Change Language/StartOn/hotkey, Save; inspect existing INI. Change a value, Reload; confirm all displayed and operational values return to file values.
4. Test transport-only with existing `DLSS5_X86BRIDGE_TRANSPORT_ONLY=1`; panel must show transport mode and engine controls disabled, result=4. Test resize and helper termination as before.
5. Send `dlss5-neural-x86.log`, `dlss5-neural-x86-host.log`, build/import/protocol logs from build-x86bridge, and any UI screenshot/error.

No claim of newly validated Windows/GPU UI behavior. D3D11 x86 only; D3D8/D3D9 need an external D3D11 translation wrapper. No native support for those APIs was added.


## Incremental update: Factory Defaults and additive installer

See [x86bridge-install.md](x86bridge-install.md). Factory Defaults now restores captured upstream tuning in memory with x86 overrides, preserving operational preferences. A new installer-x86 directory provides generic D3D11/D3D9/D3D8 presets, pinned standalone dgVoodoo and private/public sidecar layouts. Original installer/ is unchanged. Native Windows build for this update is UNVALIDATED.
