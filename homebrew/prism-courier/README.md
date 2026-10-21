# Prism Courier

An original MIT-licensed Saturn guest: steer the cyan prism through the gap in
the red barrier to the green dock. Arrows move; Enter/Start restarts. Hitting
either barrier turns the result banner red and freezes movement. Delivery turns
it green. The guest counts progress at successive horizontal checkpoints.

Use **Play Prism Courier** in the browser. Enable audio for the original computed
stereo tone. No proprietary BIOS, SDK, artwork, recording or font is included.
All source, generated geometry/texture/audio and the resulting disc are covered
by [LICENSE](LICENSE). The shared assembler/ISO writer and sound program are
original MIT-licensed sources already in this repository.

Build with Python's standard library:

```sh
python3 homebrew/prism-courier/build.py
python3 homebrew/prism-courier/build.py --check
```

Output: `web/prism-courier.iso`, a deterministic 131072-byte cooked ISO9660 disc
containing its own `LICENSE.TXT`. SHA-256:
`2fd953d65b931383518f28f81919f5c89c3ea8974b742546e1606333c2064bee`.
The existing generic Saturn IP/first-read loader starts the executable at
06004000. The executable uploads command tables, an original 8×8 diamond
texture, a 68k program and an SH-2 interrupt handler from its own disc data.

- SH-2 code clears and computes the background stripes in VDP2 VRAM, programs
  NBG0 RGB555 bitmap registers, reads SMPC direct-pad nibbles and polls VBlank.
- VDP1 commands draw the independent polygons, texture and result banner into
  the draw framebuffer; automatic field changes select the display framebuffer.
  VDP2 composes this with NBG0 according to the guest's priorities.
- The SH-2 installs its own VBR table and VDP1 draw-end handler, which increments
  a counter and returns with RTE while preserving the interrupted registers.
- SNDON releases the independent 68k from guest-uploaded vectors. Its arithmetic
  computes bipolar PCM samples, programs SCSP looping/direct pan and handles
  a timer IRQ. It then waits in STOP while SCSP continues generating stereo.

Read-only diagnostic words at 06018000 are X, Y, checkpoint count, outcome
(0=playing, 1=delivered, 2=collision), update count and draw-end IRQ count.
They are written by guest instructions and are not host control inputs.
The host only supplies controller levels and presents the emulated output.

[Native tests](../../tests/prism_courier.rs) and the
[browser suite](../../scripts/browser-smoke.mjs) assert actual disc execution,
pixels, sound computation, IRQ, two input sequences, loss, win and restart.
Replacing the texture in a local copy must change the actual native/Wasm output.
The browser also sends this disc's PCM through its real AudioWorklet.

This supplies the playable VDP1+VDP2+SCSP demonstration required by Issue #17.
It does not exercise every Saturn subsystem and does not replace the separately
required hardware exerciser disc or full hardware conformance tests.
