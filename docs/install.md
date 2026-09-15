# Installing

One installer, both architectures. It reads the game's executable to work out whether it is 32- or
64-bit and offers only the presets that can work for it, so most of what follows is the same
whichever you have.

## What you need first

**ReShade with full add-on support**, from <https://reshade.me>. Run its installer against the
game's own `.exe` and pick the API the game uses. It has to be the build labelled *with full add-on
support*; the ordinary one cannot load add-ons at all.

**For 32-bit games it has to be ReShade 6.8.0.2156, 32-bit.** This installer verifies it and
refuses any other build, including newer ones, because that is the version the bridge was tested
against.

**The runtime and the weights** — `dlssnr_amd_pass1.dll` and `dlssnr_on_amd_weights.bin` — from the
`files` channel on the [Discord](https://discord.gg/wYhvS3JSHM). They are not distributed here: the
weights are NVIDIA-derived and the runtime is a third-party build. Put both in the `files` folder
that came out of this archive.

## Installing

1. Unzip this archive anywhere.
2. Drop `dlssnr_amd_pass1.dll` and `dlssnr_on_amd_weights.bin` into its `files` folder.
3. Run `dlss5-installer.exe`.
4. **Field 1** is this unzipped folder.
5. **Field 2** is the game. A folder works; the game's `.exe` works and is better, because the
   installer reads its header. For a 32-bit game it wants the executable.
6. Pick the target on the row at the top. Only the presets that fit the detected width are shown.
7. Press **F5**. **F8** uninstalls.

The panel above the buttons says what would stop the install before you press anything: the game
still running and holding a file, a folder needing administrator rights, no room for the weights,
ReShade missing or installed twice. The first of those are refusals rather than warnings — nothing
is written at all until they pass, so a failed install cannot leave half of one behind.

## What it does to the game folder

It records what it installed, what it displaced and where the backup went, in
`dlss5-neural.install.json` (64-bit) or `dlss5-x86bridge.install.json` (32-bit). Uninstall reads
that back: files it replaced are restored from their backup, files it created are removed, and
anything you changed afterwards is kept and reported rather than overwritten. `dlss5-neural.ini` is
your tuning and is never taken away.

If ReShade's `ReShade.ini` has an `[INSTALL] BasePath` pointing inside the game directory, the
installer follows it. That is what puts the files in `bin` for Source-engine games like Half-Life 2.

## Turning it on

It starts switched off. Open the ReShade overlay with **Home**, find **DLSS Neural Rendering (AMD)**,
and enable it — or press **Ctrl+End**. `StartOn=1` in `dlss5-neural.ini` makes it come up enabled.

## If something goes wrong

The installer writes `dlss5-installer.log` beside itself on failure and prints the path. Everything
it saw is in that file.

For a 32-bit game, the add-on and its helper write `dlss5-neural-x86.log` and
`dlss5-neural-x86-host.log` in the folder the add-on actually loaded from — which is `bin` on
Half-Life 2, not the game root. `ReShade.log` is there too and says whether the add-on was loaded at
all.

Setting `DLSS5_X86BRIDGE_TIMING=1` before launching adds a line every 120 frames splitting the
bridge into capture, network and return. `run-with-timing.cmd` in the repository does it for one
launch without leaving the variable behind.

## The 32-bit routes are experimental

They work — Half-Life 2, GTA IV and Silent Hill 3 all run — but a 32-bit game cannot load the
64-bit runtime, so the add-on runs as a pair: a frontend inside the game and a helper beside it.
On plain D3D9 the frame crosses CPU-visible memory in each direction, which costs a fixed few
milliseconds every frame that no Resolution Scale reduces.

Nothing about the 64-bit D3D11, D3D12 or Vulkan routes changed.
