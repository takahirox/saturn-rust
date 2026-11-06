# Unsupported and legacy MMIO compatibility

A programming restriction, missing emulator function and SH-2 architectural
exception are different things. An `UnsupportedDevice` diagnostic is an honest
host implementation boundary; it is **not evidence of a guest bus exception or
of a physical CPU halt**. Never convert it wholesale into a guest exception, zero
read or successful write. Resolve a concrete port using its hardware contract,
revision, direction, width and access source before changing behavior.

| Evidence class | Modeling rule | Existing examples / limits |
| --- | --- | --- |
| Hardware-specified or independently observed software-visible behavior | Implement the specified data, side effects, ordering and deadlines; distinguish measurements from inference. | Normal SCU DMA completion, interrupt status and device timing. An emulator's unit test alone is not hardware evidence. |
| Absent device, unused or removed port with a justified nonfatal compatibility fallback | Explicitly identify the endpoint and implement a deterministic, narrowly bounded fallback. Record uncertain readback separately from write effects. Retain optional local diagnostics. | Unconnected external selects already return pulled-up data. Legacy DSTP now reads zero and ignores longword writes. These are different policies; DSTP zero is not a measured open-bus voltage or retained bus word. |
| Invalid operation with documented device status/error effects | Preserve that device's status, cancellation, IRQ and scheduling consequences. Where electrical behavior is unknown, retain an explicitly labeled emulator diagnostic. | Invalid direct SCU DMA parameters cancel the channel and signal illegal DMA; indirect-mode failures retain diagnostics without fabricating the prohibited illegal IRQ (ST-210 #24). Active-channel rewrites, IST writes, write-only reads and unsupported widths remain bounded diagnostics. |
| Actual SH-2 architectural exception | Follow the CPU's exception entry and recovery rules. Do not replace them with an MMIO workaround. | Misaligned CPU data accesses and invalid fetches retain address-error vector 9. A checked host-bus alignment error and guest architectural dispatch are separate layers. |

A future unknown endpoint needs its own evidence and tests. Keeping an honest
host diagnostic while a device is unmodeled does not claim that silicon traps.
Conversely, an unspecified read value does not justify ignoring writes that could
have meaningful hardware effects. This change does not catch or swallow global
emulator errors. Missing mappings, broken devices, widths and register-specific
restrictions retain their existing behavior.

## DSTP at SCU +0x60

The existing SCU identifies itself as production revision 4 (`VER=4`). This model
continues to apply the **final-specification** function removal rather than
emulating the earlier stop mechanism. The earlier manual (§1.3 p.8) already
restricts force-stop to debugging and warns that DMA cannot subsequently be used. ST-210 does not identify a numeric revision
cutoff for item 14; this is not proof that every chip reporting revision 4 has a
particular electrical response to prohibited software.

Only aligned, four-byte accesses at physical `0x05FE0060` change:

- Reads return zero. There is no guaranteed readback in the reviewed specifications.
  This low-confidence deterministic fallback has precedent in two independent
  implementations, but no measured electrical confirmation.
- Writes of every value complete without storing bits or stopping DMA. Function
  removal is documented; inert-write electrical behavior is a bounded compatibility
  interpretation, not a silicon measurement. No DMA force-stop mechanism is invented.
- Both CPU aliases (`0x05FE0060`, `0x25FE0060`) use the shared SCU endpoint, as do
  legal device-initiated cycles. Idle/armed/requested/active DMA, descriptors,
  retained samples, retriggers, arbitration, cancellation, deadlines and IRQs
  are untouched. Normal bus ownership and access clocks still apply.

ST-210 item 5 requires cache-through register access. The cacheable alias is not
made magically safe: an SH-2 line fill can generate reads of adjacent registers.
The original negative-control firmware loads `0x05FE006C` with caching enabled;
the first generated longword at `+0x60` succeeds, but `+0x64` still reports an
unsupported register. No neighboring holes or inappropriate byte/word accesses
are silently enabled. With CE=0 or data replacement disabled, ordinary P0
longword accesses can complete at DSTP without a fill. Stores do not allocate lines.
DSTA remains a removed, unmodeled port with a diagnostic, not fabricated status.
Disabling a real DMA channel's enable bit while armed still rejects subsequent
triggers; it is not interchangeable with writing DSTP.

## Optional local developer trace

After a reset, native callers can use `Machine::set_dstp_trace(true)` and
`Machine::dstp_trace()`. In browser DevTools, use the same exported methods:

```js
const { emulator } = await import('./app.js');
emulator.set_dstp_trace(true);
// Execute using the normal debugger or Play controls.
console.log(emulator.dstp_trace());
emulator.set_dstp_trace(false);
```

Capture is disabled by default, takes only the first **64 completed DSTP physical
cycles**, and clears on a new enable request, disable, cold reset or system reset.
Read the snapshot repeatedly without changing execution. No automatic logging,
network output, persistent storage, firmware dump or extra guest/device read is
performed. Debugger peeks and instruction replay do not add events.

Each line retains the main-clock completion, master/slave CPU or DMAC/SCU source,
available issuing PC, original CPU operand and width, actual generated address,
physical address, direction, write value and result. Reads distinguish direct
access from cache line fill; buffered writes retain their issuing PC even after
retirement. DMAC cycles have no invented instruction PC or CPU operand. A reset
drain labels its retained context separately. The first-64 cap is a capture limit,
not evidence that execution stopped or that no later accesses occurred. The trace
supports this scoped investigation; it is not a general bus recorder.

The private BIOS bytes and preceding transactions are unavailable. The original
`PC 0x0000181E` message alone cannot identify a read, write/value, alias, instruction,
cache state or generated access. No compatibility branch depends on that PC, a
BIOS hash, title or media identifier. See [Issue #111 evidence and validation](validation-issue-111.md).
