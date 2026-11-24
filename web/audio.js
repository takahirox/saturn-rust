// A bounded WebAudio consumer of emulated stereo PCM. No guest state is changed.
const CAPACITY = 8192;
export class AudioOutput {
  constructor(onStatus = () => {}) {
    this.onStatus = onStatus;
    this.context = null;
    this.node = null;
    this.muted = false;
    this.paused = true;
    this.pending = 0;
    this.queuedUpperBound = 0;
    this.capacityWait = null;
    this.capacityWaits = 0;
    this.producerDropped = 0;
    this.stats = { state: "disabled", received: 0, rendered: 0, underflow: 0, dropped: 0, peak: 0, queued: 0 };
  }
  status(state = this.stats.state) {
    this.stats.state = state;
    this.onStatus({ ...this.stats });
  }
  async enable() {
    try {
      if (!this.context) {
        const AudioContext = globalThis.AudioContext || globalThis.webkitAudioContext;
        if (!AudioContext) throw new Error("WebAudio is unavailable");
        this.context = new AudioContext({ sampleRate: 44100, latencyHint: "interactive" });
        await this.context.audioWorklet.addModule(new URL("./audio-worklet.js", import.meta.url));
        this.node = new AudioWorkletNode(this.context, "saturn-pcm", { numberOfInputs: 0, numberOfOutputs: 1, outputChannelCount: [2] });
        this.node.port.onmessage = ({ data }) => {
          if (data.ack) this.pending = Math.max(0, this.pending - data.ack);
          if (Number.isSafeInteger(data.queued)) this.queuedUpperBound = data.queued;
          if (data.stats) {
            this.queuedUpperBound = data.stats.queued;
            Object.assign(this.stats, data.stats, { dropped: data.stats.dropped + this.producerDropped });
            this.status(this.paused ? "paused" : this.muted ? "muted" : this.context.state !== "running" ? "blocked" : data.stats.underflowing ? "underflow" : "playing");
          }
          if (this.capacityWait && this.hasCapacity(this.capacityWait.frames)) {
            this.finishCapacityWait(true);
          }
        };
        this.node.connect(this.context.destination);
        this.context.onstatechange = () => {
          if (this.context.state !== "running") this.finishCapacityWait(false);
          this.status(this.context.state !== "running" ? "blocked" : this.paused ? "paused" : this.muted ? "muted" : "ready");
        };
      }
      await this.context.resume();
      this.node.port.postMessage({ paused: this.paused, muted: this.muted });
      this.status(this.context.state === "running" ? (this.paused ? "paused" : "ready") : "blocked");
    } catch (error) {
      this.status("blocked");
      throw error;
    }
  }
  setPaused(paused) {
    this.paused = paused;
    if (paused) this.finishCapacityWait(false);
    this.node?.port.postMessage({ paused, clear: paused });
    this.status(!this.node ? "disabled" : this.context.state !== "running" ? "blocked" : paused ? "paused" : this.muted ? "muted" : "ready");
  }
  setMuted(muted) {
    this.muted = muted;
    this.node?.port.postMessage({ muted });
    this.status(this.paused ? "paused" : muted ? "muted" : "ready");
  }
  // Queue the first PCM and resume in one Worklet message. No empty
  // rendering quantum can occur between an unpause and its initial data.
  start(pcm, stretch = false) { return this.deliver(pcm, true, stretch); }
  hasCapacity(frames) {
    // The last reported queue depth is an upper bound until more PCM arrives.
    // Pending packets account for every write the Worklet has yet to report.
    return this.queuedUpperBound + this.pending + frames <= CAPACITY;
  }
  finishCapacityWait(available) {
    const waiter = this.capacityWait;
    this.capacityWait = null;
    waiter?.resolve(available);
  }
  waitForCapacity(frames) {
    if (this.capacityWait) return this.capacityWait.ready;
    let resolve;
    const ready = new Promise(done => { resolve = done; });
    this.capacityWait = { frames, ready, resolve };
    this.capacityWaits++;
    this.node.port.postMessage({ waitForSpace: frames });
    return ready;
  }
  async waitAndDeliver(pcm, frames) {
    while (!this.hasCapacity(frames)) {
      if (this.paused || this.context.state !== "running") return false;
      if (!await this.waitForCapacity(frames)) return false;
    }
    return this.deliver(pcm);
  }
  deliverPaced(pcm) {
    const frames = pcm.length >>> 1;
    if (!frames || frames > CAPACITY || !this.node || this.paused
        || this.context.state !== "running" || this.hasCapacity(frames)) {
      return this.deliver(pcm);
    }
    return this.waitAndDeliver(pcm, frames);
  }
  deliver(pcm, resume = false, stretch = false) {
    const frames = pcm.length >>> 1;
    if (!frames || !this.node || (this.paused && !resume) || this.context.state !== "running") return false;
    // Bound transferable messages waiting for the audio thread, in addition to
    // the consumer's fixed ring. Saturation is visible in diagnostics.
    if (this.pending + frames > CAPACITY) {
      this.producerDropped += frames;
      this.stats.dropped += frames;
      this.status();
      return false;
    }
    this.pending += frames;
    const copy = new Int16Array(pcm);
    if (resume) this.paused = false;
    this.node.port.postMessage(resume ? { pcm: copy, paused: false, stretch } : { pcm: copy }, [copy.buffer]);
    if (resume) this.status("ready");
    return true;
  }
}
