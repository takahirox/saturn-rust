// A bounded WebAudio consumer of emulated stereo PCM. No guest state is changed.
export class AudioOutput {
  constructor(onStatus = () => {}) {
    this.onStatus = onStatus;
    this.context = null;
    this.node = null;
    this.muted = false;
    this.paused = true;
    this.pending = 0;
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
          if (data.stats) {
            Object.assign(this.stats, data.stats, { dropped: data.stats.dropped + this.producerDropped });
            this.status(this.paused ? "paused" : this.muted ? "muted" : this.context.state !== "running" ? "blocked" : data.stats.underflowing ? "underflow" : "playing");
          }
        };
        this.node.connect(this.context.destination);
        this.context.onstatechange = () => this.status(this.context.state !== "running" ? "blocked" : this.paused ? "paused" : this.muted ? "muted" : "ready");
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
    this.node?.port.postMessage({ paused, clear: paused });
    this.status(!this.node ? "disabled" : this.context.state !== "running" ? "blocked" : paused ? "paused" : this.muted ? "muted" : "ready");
  }
  setMuted(muted) {
    this.muted = muted;
    this.node?.port.postMessage({ muted });
    this.status(this.paused ? "paused" : muted ? "muted" : "ready");
  }
  deliver(pcm) {
    const frames = pcm.length >>> 1;
    if (!frames || !this.node || this.paused || this.context.state !== "running") return;
    // Bound transferable messages waiting for the audio thread, in addition to
    // the consumer's fixed ring. Saturation is visible in diagnostics.
    if (this.pending + frames > 8192) {
      this.producerDropped += frames;
      this.stats.dropped += frames;
      this.status();
      return;
    }
    this.pending += frames;
    const copy = new Int16Array(pcm);
    this.node.port.postMessage({ pcm: copy }, [copy.buffer]);
  }
}
