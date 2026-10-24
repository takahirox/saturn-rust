# CPU participation in SCSP memory cycles

Related to #41 and #17. Primary contract: Sega [ST-077-R2,
§3.1 printed pp.18–19, §3.2 pp.20–21, fig.3.2](https://antime.kapsi.fi/sega/files/ST-077-R2-052594.pdf),
corroborated by the [later Sega chapter 3](https://www.infochunk.com/saturn/segahtml_en/hard/scsp/hon/p03_10.htm).
The main interface buffers address/data and waits for internal completion.
Allocation precedes access, and an admitted operation completes before another
owner replaces it. There are 128 memory cycles/sample, two refresh requests,
with priority PCM/DSP, refresh, internal DMA, main interface, sound CPU.

| Client | Eligibility (model phase convention) | Admission | Memory completion | Acknowledgement |
| --- | --- | --- | --- | --- |
| PCM/DSP | Existing slot/instruction request | Existing highest priority | Existing retained operand pipeline | Existing synthesis pipeline |
| Refresh | Existing steps 124 and 126; deferred requests retained | Before DMA/CPUs | Following boundary | No CPU acknowledgement |
| Internal DMA | DEXE and remaining words | Before CPU requests | Following boundary; address/direction/geometry retained | Final publication raises SCSP pending IRQ |
| Main interface | External physical sound-RAM beat has started; write-buffer admission alone is ineligible | First free boundary; before sound CPU | Following boundary; retained read or one write publication | Later of bridge bus duration and memory completion; each halfword separately |
| Sound CPU | Start of scheduled four-clock byte/word bus cycle; first reset read follows sixteen internal clocks | First free boundary after eligibility | Following boundary; retained read or one write publication | Later of nominal bus duration and memory completion on a sound-CPU edge; replay consumes once |

The references do not specify the oscillator's console power-on phase, the
CPU-interface synchronizer delay or an equal-edge event order. These are explicit
emulator conventions, not measured electrical timing. The existing rational
memory oscillator starts at reference origin zero, with boundary
`B(n) = ceil(n * 26874100 / (44100 * 128))`, n >= 1. SNDON changes the CPU origin
but does not rephase this oscillator. Main PLL choices do not change it.
At an equal reference edge, reset precedes memory completion/allocation, then
sound CPU eligibility/acknowledgement, then main bridge acknowledgement. Requests
introduced after a processed boundary wait for the next boundary. A later high
priority request cannot replace an admitted lower priority access. Debugger
reads/writes inspect memory directly and acquire no cycles.

Independent integer enumeration (before test implementation): B(1..16) =
5, 10, 15, 20, 24, 29, 34, 39, 43, 48, 53, 58, 62, 67, 72, 77.
A main request at reference 2 admits at 5 and samples at 10; its ordinary
eight-main-clock beat can acknowledge at 10. A request introduced at 5 after
allocation admits at 10 and samples at 15. DSP ownership of [10,15) defers a
main request eligible at 7 to grant15/completion20. A main write eligible40
grants43/completes48. The first sound reset read is eligible39 (16 CPU clocks), but main wins
the grant43. Sound grants48/completes53 and acknowledges53 (22 CPU clocks).
The second reset word is eligible53, grants58/completes62 and acknowledges62. Thus reference50 from
the inherited busy-gate test is not a required hardware sample edge.

SNDOFF cancels queued/admitted sound-CPU work and its unread result, preserving
completed writes and independent main/internal work. SCSP RESET cancels unfinished
memory work. The external master may still hold its buffered request: discard its
unfinished grant and re-admit only the unfinished word after release, retaining
completed halves/results. Canceling the external request drops the entire
unfinished operation. RESET never fabricates a main-interface bus fault. This
retention/re-admission across RESET is a functional phase convention; the cited
manual does not establish an electrical RESET/B-bus crossing timing. SYSRES
and cold boot replace volatile transaction state. Manual SH-2 reset retains the
existing accepted operand drain, including its admitted sound-memory operation.
MEM4MB geometry is captured on admission. The installed chip and absent-chip
data behavior are unchanged. Register-only accesses do not request DRAM.

The main interface's documented word-only restriction is unchanged: this work
preserves the existing bridge's byte-lane compatibility behavior rather than
claiming byte cycles are supported by the physical main interface. General 68k
microsequencing and SCSP register-bus arbitration remain outside this change.


A main beat whose CPU-side width is 32 bits retains two separately admitted
16-bit memory operations, including its already sampled high operand. CPU bus
width/duration is unchanged. Memory ownership is arbitrated again before its
second word; each admitted word captures geometry separately. CPU read bridging
continues to expose the established two physical halfword beats.

The original guest fills 256 longwords through on-chip DMAC from the DIVU's
stable quotient. With two-clock bridge phases its unthrottled demand exceeds
DRAM supply: 512 cycles require at least `512*26874100/5644800 = 2437.5...`
reference clocks, so an FRT/8 observation must be at least304. Refresh and the
internal source/read-buffer phases increase that interval. At most three DRAM
cycles per longword plus refresh and the bounded FRT/readback overhead place the
independent upper bound below520 FRT counts. These bounds precede running the
guest and distinguish the inherited CPU busy gate. The 68k polls a shared flag,
reads the independently uploaded longword, programs PCM and a half-gain DSP,
and repeatedly writes waveform operands. Expected stereo includes `(a,a/2)`
and `(b,b/2)`; interpolation may produce the arithmetic midpoint on the left.
The test host changes only ROM operands before boot, never running results.


Trace end events identify the release of an owned memory interval. CPU, PCM
and internal-DMA data completion occurs there. The inherited DSP pipeline samples
its RAM operand at its instruction boundary and makes it available to IWT two
instructions later; its interval-end event is an ownership release, not an
additional DSP access. This change preserves that DSP sampling convention and
does not claim a newly measured physical DSP RAM phase.
