# dlss5-neural-amd

A ReShade add-on that runs the DLSS 5 neural rendering network on AMD GPUs.

NVIDIA's DLSS 5 tools call `nvngx_dlssnr.dll`, which does nothing on a Radeon. This add-on drives
the AMD port of the same network instead. The port is
[DLSS-NR-on-AMD](https://github.com/danielblnc/DLSS-NR-on-AMD) by danielblnc. This project does not
reimplement the network. See [Credits](#credits).

This is a proof of concept. It works, but it is not finished software.

Discord: <https://discord.gg/wYhvS3JSHM> - for DLSS 5 in general, not a support channel for this.

## What you need

| | |
|---|---|
| GPU | AMD RDNA3 or RDNA4 with the HIP 7 runtime (`amdhip64_7.dll`). HIP 6 does not work. A current Adrenalin driver includes it. Does nothing on NVIDIA or Intel. |
| Renderer | Direct3D 11 works best. Direct3D 12 works but gets less information. Vulkan is experimental. 32-bit games are experimental. OpenGL is not supported. |
| ReShade | The build labelled "with full add-on support", version 6.x. The normal build cannot load add-ons. |
| Disk | About 150 MB for the network weights. |

## Install

Download **AMD-NR ReShade Installer** from the
[Releases](https://github.com/zmodelerlover/dlss5-neural-amd/releases) page. It is one `.exe` and
needs nothing installed to run.

The installer finds your games, works out which renderer each one uses, downloads the runtime and
the weights, checks every hash, installs ReShade and the add-on, and can uninstall all of it. It
writes a record of what it installed, so uninstall restores what it replaced.

Two things it cannot do:

- It cannot set the renderer inside the game. For emulators this setting decides whether the
  add-on works at all. The installer tells you which renderer to pick.
- It cannot install ReShade for Vulkan. On Vulkan, ReShade is a system-wide layer instead of a DLL
  next to the game, so run ReShade's own installer for that.

The renderer detection is usually right but not always. One folder can contain both a D3D11 and a
D3D12 executable. A wrong choice installs fine and then does nothing in the game. Each game card
links to that game's PCGamingWiki page. Check it before installing.

There is a [video of the whole install](https://www.youtube.com/watch?v=L2v0b98wReQ).

## Install by hand

You do not need the installer. There are four steps.

**Step 1. Install ReShade.** Get the build labelled "with full add-on support" from
<https://reshade.me>. The normal build cannot load add-ons and nothing will work without this.

Run it against the game's `.exe` and choose **Direct3D 10/11/12**. You can skip the shader
download. On Vulkan, pick Vulkan instead, and read the Vulkan row in the table below.

Then put these three files next to the same `.exe`.

**Step 2. `dlss5-neural.addon64`** — from the
[Releases](https://github.com/zmodelerlover/dlss5-neural-amd/releases) page. One file covers D3D11,
D3D12 and Vulkan.

**Step 3. `dlssnr_amd_pass1.dll`** (7 MB) and **Step 4. `dlssnr_on_amd_weights.bin`** (141 MB) —
from the `files` channel on the [Discord](https://discord.gg/wYhvS3JSHM). These are not in this
repository and will not be. The weights are NVIDIA-derived and the runtime belongs to another
project.

The runtime must be DLSS-NR-on-AMD v0.3.0 with the patches in `tools/runtime-patches.json`. The
add-on checks its hash when it loads and refuses anything else. This is deliberate: the add-on
writes to hardcoded offsets in that exact binary, and a different build would hang the game.

Check the files against `tools/SHA256SUMS.txt`:

```powershell
Get-FileHash dlssnr_amd_pass1.dll, dlssnr_on_amd_weights.bin -Algorithm SHA256
```

### Where the files go

Next to the executable that renders the game. For most games that is the main `.exe`. For Source
engine games it is the one in `bin`. For emulators it is next to the emulator, not next to the ROMs.

A finished D3D11 install looks like this:

```
Need for Speed\
  NeedForSpeed.exe
  d3d11.dll                  <- ReShade, add-on build
  dlss5-neural.addon64
  dlssnr_amd_pass1.dll
  dlssnr_on_amd_weights.bin
  dlssnr_on_amd.ini          <- created automatically
  dlss5-runtime\             <- created automatically
```

The last two are created by the add-on. Do not create them yourself.

### Per renderer

| Renderer | What to do |
|---|---|
| D3D11 | Install ReShade, choose Direct3D 10/11/12. Nothing else to change. |
| D3D12 | Same as D3D11. The add-on only receives the final image, so results are weaker. |
| PCSX2 | Set the renderer to Direct3D 11 in the emulator. Watch for per-game overrides, which silently beat the global setting. |
| Vulkan | Run ReShade's own installer against the game's `.exe` and pick Vulkan. Experimental. The game must import `vkCreateDevice` by name and present on a graphics-capable queue. |
| 32-bit games | Experimental, and the files are different. See [Older 32-bit games](#older-32-bit-games) below. |

### Older 32-bit games

A 32-bit game needs a different set of files. It cannot load the 64-bit runtime at all, so the
add-on runs as two pieces: a 32-bit part inside the game and a 64-bit helper beside it. This is
experimental.

Use these instead of `dlss5-neural.addon64`:

- `dlss5-neural.addon32` — the 32-bit part, loaded by ReShade
- `dlss5-neural-host64.exe` — the 64-bit helper, started by the add-on

Both are on the [Releases](https://github.com/zmodelerlover/dlss5-neural-amd/releases) page. The
runtime and the weights are the same two files as before, and they go in the same folder.

**ReShade has to be the 32-bit build**, again with full add-on support. The 64-bit one will not
load in a 32-bit game.

| Game uses | ReShade file name |
|---|---|
| D3D11 | `dxgi.dll` |
| D3D9 | `d3d9.dll` |
| D3D8 | `d3d9.dll` |

**D3D8 needs one more file.** D3D8 is translated to D3D9 first, by
[d3d8to9](https://github.com/crosire/d3d8to9). Put its `d3d8.dll` next to the game.

If the game already has its own `d3d8.dll` — a fan patch or a fix pack, like the Silent Hill 3 PC
Fix — do not replace it. Name the d3d8to9 file `d3d8R.dll` instead and leave the existing one
alone. Those wrappers look for `d3d8R.dll` and pass the calls along.

A finished D3D9 install looks like this:

```
Game\
  game.exe
  d3d9.dll                   <- ReShade, add-on build, 32-bit
  dlss5-neural.addon32
  dlss5-neural-host64.exe
  dlssnr_amd_pass1.dll
  dlssnr_on_amd_weights.bin
```

Plain D3D9 without D3D9Ex is slower than the rest. Each frame is copied through system memory
twice, which costs a few milliseconds no matter how low the Resolution Scale is.

## Turn it on

Press **Home** to open ReShade, go to the **Add-ons** tab, and find **DLSS Neural Rendering (AMD)**.

The add-on starts switched off. Tick **Enabled** or press **Ctrl+End**.

It starts off on purpose. The add-on rewrites every frame, and some settings can crash the display
driver, so nothing happens until you have looked at the panel.

You can change this. **Enabled from the first frame** turns it on when the game opens. The toggle
hotkey can be rebound. **Disable the effect on alt-tab** turns it off when the game loses focus.

## Settings

Every control has a `(?)` tooltip that explains what it does. The panel is in English or Brazilian
Portuguese; use the **Language** control to switch.

Settings are saved to `dlss5-neural.ini` as soon as you release a control. Nothing is lost by
closing the game.

The controls are colour-coded:

| Colour | Meaning |
|---|---|
| Red | This value can crash the display driver. |
| Amber | Beyond what has been tested. Not known to break, not known to work. |

The main controls:

- **Resolution Scale** — the size of the network input relative to the frame. 0.50 uses a quarter
  of the pixels. Smaller is faster and loses fine detail.
- **Pass Count** — one to three evaluations. More passes strengthen the effect and can add grain.
- **Residual Limit** — caps how much the image is changed. Default 0.25.
- **Timing** — Same frame waits for the result. Async is older and is not the tested path.

The defaults are a reasonable starting point.

Each control carries a tag saying how well it is understood: `MEASURED`, `TRACED`, `UNKNOWN` or
`INERT`. The legend is at the bottom of the panel.

## Troubleshooting

| What you see | What it means |
|---|---|
| The add-on is not in the Add-ons tab | `ReShade.ini` has `DisabledAddons=` listing it under `[ADDON]`. ReShade writes that line if you ever untick the add-on. Delete the line. |
| The status says the API is wrong | Only D3D11, D3D12 and Vulkan are supported. Check for a per-game renderer override. |
| `HIP: amdhip64_7.dll failed to load` | HIP 7 is not installed. HIP 6 does not count. |
| `hash mismatch; refused` | The wrong `dlssnr_amd_pass1.dll`. Compare with `tools/SHA256SUMS.txt`. |
| `missing:` followed by a file path | That file is not where the add-on looks. Put it at exactly that path. |
| The game crashes with `887A0005` | A Windows driver reset. Lower the Resolution Scale. |
| The colours change but textures look the same | Try Resolution Scale 0.75 or 1.00 and compare the same scene. |
| Vulkan says the present queue is not graphics-capable | The game presents from an async queue. For DOOM Eternal set `r_presentFromAsync "0"`. |

### Reporting a problem

Screenshots rarely help, because flicker is frames alternating and a still image looks fine.

Two things are needed:

1. **The status line** in the panel. It reads `Running: X processed, Y skipped (Z%)` with the
   buffer sizes. Copy that line.
2. **The logs**, both next to the game's `.exe`:
   - `dlss5-neural.log` — what the add-on detected and what it measured.
   - `dlssnr_on_amd.log` — what the runtime did.

If the problem is the installer rather than the add-on, use its own **Report a problem** button. It
collects everything into one `.zip` and opens the folder. Nothing is sent anywhere.

## Building

You do not need to build anything to use this. If you want to:

```powershell
.\build.ps1 -Target neural
```

You need Visual Studio with the C++ tools and the Windows SDK. The ReShade and Dear ImGui headers
are already in `external/`.

### Rebuilding the runtime

You do not need this. The DLL on the Discord is already the rebuilt one. This is here so you can
check what was changed instead of trusting the file.

DLSS-NR-on-AMD ships one `dlssnr_on_amd_setup.exe` with the runtime appended to it raw, so the DLL
can be extracted without running the installer:

```powershell
python tools\extract_runtime.py dlssnr_on_amd_setup.exe version.dll
python tools\patch_runtime.py version.dll tools\runtime-patches.json dlssnr_amd_pass1.dll
```

Both commands print hashes. Compare them with `tools/SHA256SUMS.txt`.

`tools/runtime-patches.json` lists every patch: the offset, the bytes before, the bytes after, and
why. It also lists what is deliberately not patched.

Every change is written in place at the same length, so nothing moves and the add-on's hardcoded
offsets stay valid.

## Limits

- D3D11 is the only route where the game's own depth and motion vectors reach the network. On D3D12
  and Vulkan the add-on only receives the final image.
- FSR upscaling is not implemented and is not planned.
- On 32-bit D3D9 without D3D9Ex, each frame crosses system memory twice. That costs a few
  milliseconds per frame regardless of the Resolution Scale.
- This is tested by one person on one card.

## Support this project

This is one person, one graphics card, and evenings. There is no company behind it and nothing
here is sponsored. Every measurement in this README came from a single RX 9070 XT, which is why
"tested on one card" appears as often as it does.

[![Support this project on Ko-fi](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/T6T213OVFE)

**[ko-fi.com/T6T213OVFE](https://ko-fi.com/T6T213OVFE)**

What it pays for, in order:

- **Research tokens.** Decompiling a closed runtime and chasing one field through a binary is
  metered work. This is the main thing deciding whether the next question gets answered.
- **Other hardware.** Everything is verified on one RDNA4 card. RDNA3 is untested.
- **Time.** Most of the useful work produces one number rather than a feature.
- **Games to test.** Adding a game to the list means owning it and playing it.

Nothing is gated behind a donation. The code is MIT either way.

## Credits

This project is downstream of
**[DLSS-NR-on-AMD](https://github.com/danielblnc/DLSS-NR-on-AMD)** by **danielblnc**. That project
produces the runtime and the weights, which is everything the network needs to run. Neither is
reimplemented or redistributed here. This repository adds the ReShade add-on around it: the D3D11,
D3D12 and Vulkan routes, the 32-bit bridge, the guide capture and the overlay.

It has its own terms. The MIT licence below covers only the code in this repository.

Thanks to everyone who ran a build and sent back a log.

## License

MIT, in `LICENSE`. Third-party headers in `external/` keep their own licences, listed in
`external/reshade/NOTICE.md`.
