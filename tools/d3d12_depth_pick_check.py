"""Which D3D12 buffer is the scene depth, and the three ways picking it went wrong.

SettleD3D12Depth chooses from a tally of depth-stencil binds and clears, once per present. It used
to be one line -- largest wins -- and the report from a Cyberpunk 2077 run on a 7900 XT (scene at
1129x706 into a 1920x1200 swapchain) caught it three separate times:

  1. a 2048x2048 R16 shadow map is larger than the scene depth, and won;
  2. with a shape filter added, a 1920x1200 buffer the game drew into 7 times a frame beat the
     scene depth it drew into about 2,400 times;
  3. with two buffers of the same size, the pick settled on the one the game never clears, so the
     pre-clear snapshot froze on a stale frame.

This is that decision written out once more, in the smallest form that can be asserted against. If
the two disagree, this file is the one that is wrong.

    python tools/d3d12_depth_pick_check.py
"""

HOLD = 3  # presents added up before the first pick, same constant as SettleD3D12Depth
SCREEN = (1920, 1200)


FLOOR = 256  # kGuideFloor: no guide worth having is smaller than this


def screen_shaped(w, h, screen_w, screen_h):
    """An absolute floor always; aspect within 3% and a ninth of the area once there is a
    swapchain to measure against. The swapchain size is zero until the effect has been enabled
    once, and rejecting everything until then meant a fresh install never found a guide."""
    if w < FLOOR or h < FLOOR:
        return False
    if not (screen_w and screen_h):
        return True
    if abs(w / h - screen_w / screen_h) > 0.03 * (screen_w / screen_h):
        return False
    return w * h >= screen_w * screen_h / 9


class Pick:
    def __init__(self):
        self.chosen = None
        self.chosen_binds = 0
        self.cold = 0

    def settle(self, tally):
        """tally: {name: [binds, clears]}, kept across presents unless cleared."""
        eligible = {n: v for n, v in tally.items() if v[1] > 0}
        if not eligible:
            eligible = dict(tally)
        if not eligible:
            tally.clear()
            return
        best = max(eligible, key=lambda n: eligible[n][0])

        if best == self.chosen:
            self.chosen_binds = eligible[best][0]
            self.cold = 0
            tally.clear()
            return
        if self.chosen is None:
            self.cold += 1
            if self.cold < HOLD:
                return  # not cleared: three presents of binds add up
            self.cold = 0
        elif eligible[best][0] <= self.chosen_binds + self.chosen_binds // 4:
            tally.clear()  # no margin, so the incumbent keeps the slot
            return

        self.chosen, self.chosen_binds = best, eligible[best][0]
        tally.clear()


def run(presents, screen=SCREEN):
    """Each present is {name: (w, h, binds, clears)}. Buffers that are not screen-shaped never
    reach the tally, exactly as OnBindDepthStencil never records them."""
    p = Pick()
    tally = {}
    for present in presents:
        for name, (w, h, binds, clears) in present.items():
            if not screen_shaped(w, h, *screen):
                continue
            slot = tally.setdefault(name, [0, 0])
            slot[0] += binds
            slot[1] += clears
        p.settle(tally)
    return p.chosen


scene = (1129, 706, 2400, 1)          # the scene depth: drawn into constantly, cleared every frame
shadow = (2048, 2048, 500, 1)         # a shadow map: bigger than the scene, square
wide = (1920, 1200, 7, 1)             # screen-sized, drawn into seven times
never_cleared = (1129, 706, 2400, 0)  # same size as the scene, never cleared

# 0. The shape filter on its own.
assert screen_shaped(1129, 706, *SCREEN)
assert not screen_shaped(2048, 2048, *SCREEN), "a square shadow map is not screen-shaped"
assert not screen_shaped(256, 160, *SCREEN), "a sixtieth of the area is not the scene"
assert not screen_shaped(1, 1, 0, 0), "a 1x1 never passes, swapchain known or not"
assert screen_shaped(1129, 706, 0, 0), "but a real buffer does, or nothing is ever observed"
assert not screen_shaped(128, 128, *SCREEN), "below the floor, whatever its shape"

# 1. Largest wins took the shadow map. Shape decides it now.
assert run([{"scene": scene, "shadow": shadow}] * 3) == "scene"

# 2. Screen-shaped and barely drawn into does not beat the scene pass.
assert run([{"scene": scene, "wide": wide}] * 3) == "scene"

# 3. Same size, and only one of them is cleared: the cleared one is the scene.
assert run([{"cleared": scene, "stale": never_cleared}] * 3) == "cleared"

# 4. Three presents before anything is taken, so one odd frame cannot decide it.
assert run([{"scene": scene}]) is None
assert run([{"scene": scene}] * 3) == "scene"

# 5. A challenger without a clear margin does not take the slot from the incumbent.
steady = [{"scene": scene}] * 5
close = (1129, 706, 2401, 1)
assert run(steady + [{"scene": scene, "rival": close}] * 5) == "scene"

# 6. A real move -- the game switched buffers and draws into the new one far more -- is followed.
moved = (1129, 706, 9000, 1)
assert run(steady + [{"moved": moved}] * 3) == "moved"

# 7. A clear that arrives before the buffer is bound this present still counts. The add-on
#    records it rather than looking it up, so the buffer the game clears every frame cannot look
#    like one it never clears.
assert run([{"cleared": (1129, 706, 0, 1)}, {"cleared": (1129, 706, 2400, 0)}] * 3) == "cleared"

print("d3d12 depth pick: 8 properties hold")
