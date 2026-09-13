# Incremental x86 update: ColourStrength 0.25 + installer UI

Only two changes: fresh x86 ColourStrength and in-memory Factory Defaults now use 0.25; installer presentation uses dark Win32/GDI panels, a red geometric sidebar, three API cards and a conditional D3D8/D3D9 notice. No Components section or artificial Back navigation. Details retains the complete operation log. Window layout scales to the system DPI/work area and contracts when D3D11 is selected. No external UI dependency or official AMD/Radeon asset.

Existing INI is preserved. Factory does not save; Save and Reload retain their semantics. Structure=1, Skin=1, Passes=1; all other defaults unchanged. Frontend/overlay/protocol source unchanged. Host needs rebuilding for the new default; installer needs rebuilding for both changes. No functional addon32 change.

## Windows build and release

From the extracted project directory, with Visual Studio C++ and Windows SDK:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\build-x86bridge.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File .\tools\package-x86-release.ps1 -Private -OutputPath .\dlss5-x86bridge-visual-025-private-release.zip
```

The complete private build kit already contains the pinned sidecars in release/. The script builds MSVC /MT, tests both protocol architectures, checks PE/imports, builds untouched addon64 in a temporary copy, then stages release/dlss5-installer-x86.exe and release/files/{dlss5-neural.addon32,dlss5-neural-host64.exe}. It rebuilds addon32 as part of its existing full build, although addon behavior is unchanged. Public packaging uses the same package command without -Private; third-party payload remains separate.

## Executed here

52 installer filesystem/payload tests PASS. Protocol/control/Factory/overlay portable tests PASS. Existing INI byte preservation PASS. Factory 0.25, no save and one required history invalidation PASS. Upstream 55/55 SHA256 unchanged. Actual installer C++ syntax checked with Win32/backend declaration doubles; this is not native compilation or Windows API validation.

Native build attempt: powershell.exe unavailable in the Linux executor (exit 127). NEW installer/host/addon binaries, native PE/import checks and original addon64 build are UNVALIDATED for this update. No new EXEs or Windows screenshot are included. The previous supplied binaries are only labeled rollback material in the private kit, never the new release.

## Short local check

1. Open the new installer at 100% and 150% Windows display scaling. Tab/Space through Browse, all three cards and action buttons. Confirm selected border/radio, D3D8/9 notice, D3D11 reflow, readable path/status and Details. Check on a smaller display as well.
2. Use a disposable fresh folder for each preset. ColourStrength must start at 0.25. Reinstall an existing user INI: bytes must stay unchanged.
3. Change tuning, Save, then alter it again. Factory sets Colour=0.25 / Structure=1 / Skin=1 / Passes=1 without changing INI. Reload restores the last Save.

UI layout and interaction remain UNVALIDATED on Windows; report any clipping with display scaling and resolution. Installation backend, docking, frame transport, runtime and payload hashes are unchanged.
