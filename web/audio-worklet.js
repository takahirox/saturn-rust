// Stereo 44.1 kHz guest PCM -> host sample rate. Fixed ring, no shared host data.
class SaturnPcm extends AudioWorkletProcessor {
  constructor() {
    super();
    this.ring = new Int16Array(8192 * 2);
    this.read = 0;
    this.length = 0;
    this.phase = 0;
    this.paused = true;
    this.muted = false;
    this.stats = { received: 0, rendered: 0, underflow: 0, dropped: 0, peak: 0, queued: 0, underflowing: false };
    this.sinceReport = 0;
    this.port.onmessage = ({ data }) => {
      if (data.paused !== undefined) this.paused = data.paused;
      if (data.muted !== undefined) this.muted = data.muted;
      if (data.clear) { this.length = 0; this.read = 0; this.phase = 0; }
      if (data.pcm) {
        const pcm = data.pcm;
        const frames = pcm.length >>> 1;
        this.stats.received += frames;
        for (let i = 0; i < frames; i++) {
          if (this.length === 8192) {
            this.read = (this.read + 1) % 8192;
            this.length--;
            this.stats.dropped++;
          }
          const write = (this.read + this.length) % 8192;
          this.ring[write * 2] = pcm[i * 2];
          this.ring[write * 2 + 1] = pcm[i * 2 + 1];
          this.length++;
        }
        this.port.postMessage({ ack: frames });
      }
    };
  }
  process(_inputs, outputs) {
    const [left, right] = outputs[0];
    this.stats.underflowing = false;
    for (let i = 0; i < left.length; i++) {
      if (this.paused) continue;
      if (!this.length) { this.stats.underflow++; this.stats.underflowing = true; continue; }
      const next = (this.read + (this.length > 1 ? 1 : 0)) % 8192;
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
      this.read = (this.read + available) % 8192;
      this.length -= available;
      if (!this.length) this.phase = 0;
    }
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
