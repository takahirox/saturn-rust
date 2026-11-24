// Stereo 44.1 kHz guest PCM -> host sample rate. Fixed ring, no shared host data.
const CAPACITY = 8192;
const HOP = 384;
// A 40 Hz cycle is 1,103 frames at 44.1 kHz. Search on both sides of
// the nominal position so low notes can align across an entire cycle.
const SEARCH = 576;
const GUEST_RATE = 44100;
const TARGET_QUEUE = 2400;
const RATE_INTERVAL = 2048;
const MIN_RATE = 0.38;
const LOW_QUEUE_RATE = 0.25;
class SaturnPcm extends AudioWorkletProcessor {
  constructor() {
    super();
    this.ring = new Int16Array(CAPACITY * 2);
    this.read = 0;
    this.length = 0;
    this.phase = 0;
    this.stretch = false;
    this.block = new Float32Array(HOP * 2);
    this.tail = new Float32Array(HOP * 2);
    this.cursor = HOP;
    this.analysis = 0;
    this.haveTail = false;
    this.rate = 1;
    this.ingressRate = 1;
    this.rateFrames = 0;
    this.lastReceived = 0;
    this.paused = true;
    this.muted = false;
    this.stats = { received: 0, rendered: 0, underflow: 0, dropped: 0, peak: 0, queued: 0, underflowing: false };
    this.sinceReport = 0;
    this.spaceRequest = null;
    this.port.onmessage = ({ data }) => {
      if (data.paused !== undefined) this.paused = data.paused;
      if (data.muted !== undefined) this.muted = data.muted;
      if (data.stretch !== undefined) {
        this.stretch = data.stretch;
        this.lastReceived = this.stats.received;
      }
      if (data.clear) {
        this.length = 0; this.read = 0; this.phase = 0;
        this.spaceRequest = null;
        this.stretch = false;
        this.cursor = HOP;
        this.analysis = 0;
        this.haveTail = false;
        this.rate = this.ingressRate = 1;
        this.rateFrames = 0;
        this.stats.stretchRate = 1;
      }
      if (data.waitForSpace) this.spaceRequest = data.waitForSpace;
      if (data.pcm) {
        const pcm = data.pcm;
        const frames = pcm.length >>> 1;
        this.stats.received += frames;
        for (let i = 0; i < frames; i++) {
          if (this.length === CAPACITY) {
            this.read = (this.read + 1) % CAPACITY;
            this.length--;
            this.analysis = Math.max(0, this.analysis - 1);
            this.stats.dropped++;
          }
          const write = (this.read + this.length) % CAPACITY;
          this.ring[write * 2] = pcm[i * 2];
          this.ring[write * 2 + 1] = pcm[i * 2 + 1];
          this.length++;
        }
        this.port.postMessage({ ack: frames, queued: this.length });
      }
      this.reportSpace();
    };
  }
  reportSpace() {
    const request = this.spaceRequest;
    if (request === null || this.length + request > CAPACITY) return;
    this.spaceRequest = null;
    this.port.postMessage({ queued: this.length });
  }
  // Compare the previous grain's tail with candidate input at the next
  // analysis position. Both channels use the same alignment.
  prepareBlock() {
    const near = Math.floor(this.analysis);
    const first = Math.max(0, near - SEARCH);
    const last = Math.min(near + SEARCH, this.length - 2 * HOP);
    if (first > last) return false;
    let start = Math.min(near, last);
    // Slow consumption before the queue reaches the two grains needed
    // to prepare the next block.
    const surplusFrames = this.length - 2 * HOP;
    const rate = Math.min(this.rate, Math.max(LOW_QUEUE_RATE, surplusFrames / 1600));
    if (this.haveTail && rate < 0.997) {
      let energy = 0;
      for (let i = 0; i < HOP * 2; i += 8) {
        energy += this.tail[i] ** 2 + this.tail[i + 1] ** 2;
      }
      if (energy > 1e-7) {
        let best = -Infinity;
        for (let candidate = first; candidate <= last; candidate += 2) {
          let dot = 0, nextEnergy = 0;
          for (let i = 0; i < HOP; i += 4) {
            const at = ((this.read + candidate + i) & (CAPACITY - 1)) * 2;
            const l = this.ring[at] / 32768, r = this.ring[at + 1] / 32768;
            dot += this.tail[i * 2] * l + this.tail[i * 2 + 1] * r;
            nextEnergy += l * l + r * r;
          }
          const score = dot / Math.sqrt(energy * nextEnergy + 1e-12) - Math.abs(candidate - near) * 0.00005;
          if (score > best) { start = candidate; best = score; }
        }
      }
    }
    for (let i = 0; i < HOP; i++) {
      const head = ((this.read + start + i) & (CAPACITY - 1)) * 2;
      const tail = ((this.read + start + HOP + i) & (CAPACITY - 1)) * 2;
      const weight = this.haveTail ? i / HOP : 1;
      for (let ch = 0; ch < 2; ch++) {
        const out = i * 2 + ch;
        this.block[out] = this.tail[out] * (1 - weight) + this.ring[head + ch] * (weight / 32768);
        this.tail[out] = this.ring[tail + ch] / 32768;
      }
    }
    this.haveTail = true;
    this.analysis += HOP * rate;
    this.stats.stretchRate = rate;
    const consumed = Math.min(this.length, Math.max(0, Math.floor(this.analysis) - SEARCH));
    this.analysis -= consumed;
    this.read = (this.read + consumed) & (CAPACITY - 1);
    this.length -= consumed;
    return true;
  }
  stretchFrame(left, right, i) {
    if (this.cursor >= HOP) {
      if (!this.prepareBlock()) return false;
      this.cursor -= HOP;
    }
    const at = Math.floor(this.cursor), alpha = this.cursor - at;
    const following = at + 1 === HOP ? this.tail : this.block;
    const next = at + 1 === HOP ? 0 : (at + 1) * 2;
    left[i] = this.block[at * 2] * (1 - alpha) + following[next] * alpha;
    right[i] = this.block[at * 2 + 1] * (1 - alpha) + following[next + 1] * alpha;
    this.cursor += GUEST_RATE / sampleRate;
    return true;
  }
  adjustRate(frames) {
    this.rateFrames += frames;
    if (this.rateFrames < RATE_INTERVAL) return;
    const incoming = (this.stats.received - this.lastReceived) / (this.rateFrames * GUEST_RATE / sampleRate);
    this.lastReceived = this.stats.received;
    this.rateFrames = 0;
    this.ingressRate += (Math.min(1.15, incoming) - this.ingressRate) * 0.22;
    // The arrival estimate tracks sustained slow emulation. Queue feedback
    // speeds up playout when headroom returns and consumes the reserve
    // more slowly when it is nearly gone.
    this.rate = Math.max(MIN_RATE, Math.min(1, this.ingressRate + (this.length - TARGET_QUEUE) / 24000));
  }
  process(_inputs, outputs) {
    const [left, right] = outputs[0];
    this.stats.underflowing = false;
    if (!this.paused && this.stretch) this.adjustRate(left.length);
    for (let i = 0; i < left.length; i++) {
      if (this.paused) continue;
      if (this.stretch) {
        if (!this.stretchFrame(left, right, i)) {
          this.stats.underflow++;
          this.stats.underflowing = true;
          continue;
        }
        if (this.muted) left[i] = right[i] = 0;
        this.stats.peak = Math.max(this.stats.peak, Math.abs(left[i]), Math.abs(right[i]));
        this.stats.rendered++;
        continue;
      }
      if (!this.length) { this.stats.underflow++; this.stats.underflowing = true; continue; }
      const next = (this.read + (this.length > 1 ? 1 : 0)) % CAPACITY;
      const l = (this.ring[this.read * 2] * (1 - this.phase) + this.ring[next * 2] * this.phase) / 32768;
      const r = (this.ring[this.read * 2 + 1] * (1 - this.phase) + this.ring[next * 2 + 1] * this.phase) / 32768;
      left[i] = this.muted ? 0 : l;
      right[i] = this.muted ? 0 : r;
      this.stats.peak = Math.max(this.stats.peak, Math.abs(left[i]), Math.abs(right[i]));
      this.stats.rendered++;
      this.phase += 44100 / sampleRate;
      const consume = Math.floor(this.phase);
      this.phase -= consume;
      const available = Math.min(this.length, consume);
      this.read = (this.read + available) % CAPACITY;
      this.length -= available;
      if (!this.length) this.phase = 0;
    }
    this.reportSpace();
    this.sinceReport += left.length;
    if (this.sinceReport >= 2048) {
      this.sinceReport = 0;
      this.stats.queued = this.length;
      this.port.postMessage({ stats: { ...this.stats } });
    }
    return true;
  }
}
registerProcessor("saturn-pcm", SaturnPcm);
