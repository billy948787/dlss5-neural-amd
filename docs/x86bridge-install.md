# Additive x86 installer + Factory Defaults

Build from the project directory on Windows with Visual Studio C++ and Windows SDK:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\build-x86bridge.ps1
```

The script compiles addon32 x86 /MT, host64 x64 /MT and the new installer x64 /MT, tests the protocol on both architectures, checks PE/import boundaries, and builds the current integrated addon64 from the same checkout. It places the new pair and installer in `release/`. With the private sidecars present, it also runs installer filesystem tests there. Those tests use synthetic PE targets, never the real game folder.

Open `release/dlss5-installer-x86.exe`, browse to the actual x86 executable and select D3D11, D3D9 or D3D8. PE32+ targets are rejected. API selection is manual; import-based autodetection was not added. Close the game before install/uninstall.

## Defaults and UI

- Fresh install only: ColourStrength=0.25, Structure=1, Skin=1, Passes=1. Other values follow upstream; Scale=1 matches original EnsureNeuralIni. Existing dlss5-neural.ini stays byte-identical.
- Host captures constructed upstream settings before LoadSettings. Factory Defaults restores that memory snapshot with the five x86 overrides, including inline=1; it does not save. Constructed Scale is 0.5, whereas upstream fresh INI sets Scale=1; this original distinction is preserved.
- Factory preserves Enabled, StartOn, hotkey/modifiers, alt-tab preference and language. It leaves restart-only diagnostics alone and invalidates history once when temporal/guide switches change.
- Save and Reload retain their previous semantics. Factory uses CommandCode=2 in protocol v2; all wire sizes and frame messages are unchanged. Only SyncControls in Present sends it; overlay sets a pending flag.

## Docking

Installation merges only `[OVERLAY] Docking/Window`. A saved panel entry, even floating, is never changed. When Home is already docked, its actual DockId is reused with no position/size copied. For a completely fresh layout, the ReShade 6.8 dock tree is seeded using the installer's current monitor work area, not fixed 4K coordinates. ReShade resizes its dockspace at runtime. Other INI values remain unchanged.

If there is an unusual existing layout without a docked Home, the installer preserves it and leaves docking manual. No addon startup loop re-docks windows. Manual installation without running the installer does not seed a layout. Runtime behavior still needs Windows validation; passing serialized-layout tests is not proof of live ImGui docking.

## Files, safety and payloads

- D3D11: no dgVoodoo installed. D3D8/9: extract only the matching `MS/x86/D3D8.dll` or `MS/x86/D3D9.dll` plus baseline dgVoodoo.conf from pinned complete ZIP 2.87.4.
- Set OutputAPI=d3d11_fl11_0, DirectX internal3D/VRAM4096, appdriven filtering/mipmap/AA, unforced resolution, watermark=false and FastVideoMemoryAccess=false. Other configuration content is retained.
- SHA256 validates the ZIP, chosen wrapper, runtime, weights, ReShade and bridge pair before changing target files. No embedded dgVoodoo or neural payload in installer EXE; release sidecars remain separate.
- Existing conflicts are backed up under `.dlss5-x86bridge-backups/`; `dlss5-x86bridge.install.json` records hashes/ownership/backups/versions. Reinstall of same content is idempotent. Changing preset requires uninstall first.
- Uninstall restores unchanged backups and removes owned unmodified binaries. Personal configs and files modified after installation are retained with warnings and manifest records. It never removes unrelated files. Installation journals target writes; interrupted installs can be recovered via uninstall; do not delete the manifest/backups.
- Existing HIP installation is a prerequisite, unchanged from the validated setup. No HIP installer or new runtime was introduced.

The installer never launches a ReShade Setup executable. Install ReShade Full Add-on Support manually, or provide the separately obtained x86 `dxgi.dll` sidecar whose SHA-256 matches the supported 6.8.0.2156 build. Other versions fail closed. No third-party license policy is expanded by this contribution; public packaging excludes those payloads and uses sidecars.

## Tests and manual regression

Portable tests cover the protocol, control flow, IPC timeout behavior, overlay compilation and host Factory methods with engine-state doubles. Installer filesystem tests cover SHA validation, exact wrapper extraction, transactional recovery and docking merges when a private pinned-payload fixture is supplied.

MSVC build/PE imports are covered by `build-x86bridge.ps1` and CI. Windows installer UI, Windows ZIP extraction, live docking and GPU/game runtime still require live validation.

Local checklist (no game-specific production code):
1. RE1 HD Remaster and RE5: D3D9 preset; test RE5 4K. Silent Hill 3: D3D8 preset. Verify internal3D/VRAM4096, no ERR08, LUID MATCH and result=1 same_frame=1.
2. Check 1/2 passes and live controls. Fresh install Colour Strength=0.25; new panel docked. Undock, restart/reinstall and verify personal layout remains.
3. Change tuning; Save. Change again; Factory Defaults. Confirm INI unchanged by Factory and preferences preserved. Reload must recover the last Save.
4. Test uninstall in a copied game folder first; inspect retained config/backups. Send installer-x86.log, frontend/host logs and build/import/protocol logs.
