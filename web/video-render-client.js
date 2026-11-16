// Optional renderer transport. Call waitForCapacity before advancing a field,
// activate while running that producer, then submit its ordered field marker.
export class VideoRenderClient {
  constructor({ maxFrames = 2, maxBytes = 64 * 1024 * 1024, presentationPort = null } = {}) {
    if (!Number.isInteger(maxFrames) || maxFrames < 1 || maxFrames > 8
        || !Number.isSafeInteger(maxBytes) || maxBytes < 1048576) {
      throw new Error("Invalid video queue capacity");
    }
    this.directPresentation = Boolean(presentationPort);
    this.maxFrames = maxFrames;
    this.maxBytes = maxBytes;
    this.chunkSequence = 0;
    this.frameSequence = 0;
    this.receivedSequence = 0;
    this.postedBytes = 0;
    this.acknowledgedBytes = 0;
    this.peakPendingFrames = 0;
    this.peakPendingBytes = 0;
    this.pending = new Map();
    this.error = null;
    this.closed = false;
    this.ready = new Promise((resolve, reject) => {
      this.resolveReady = resolve;
      this.rejectReady = reject;
    });
    this.ready.catch(() => {});
    this.worker = new Worker(new URL("./video-worker.js", import.meta.url), { type: "module" });
    if (presentationPort) this.worker.postMessage({ type: "presentation-port", port: presentationPort }, [presentationPort]);
    this.worker.onerror = event => this.fail(new Error(event.message || "Video Worker failed"));
    this.worker.onmessage = ({ data }) => {
      if (this.closed || this.error) return;
      try {
        if (data.type === "ready") { this.resolveReady(); return; }
        if (data.type === "error") throw new Error(data.message);
        const item = this.pending.get(data.sequence);
        if (data.type !== "frame" || data.sequence !== this.receivedSequence++ || !item
            || data.frames !== item.fields || data.chunks !== item.chunks
            || data.bytes < this.acknowledgedBytes || data.bytes > this.postedBytes) {
          throw new Error("Video receipt does not match its field");
        }
        if (item.capture && !item.direct && (!(data.pixels instanceof Uint8Array)
            || data.pixels.byteLength !== data.width * data.height * 4)) {
          throw new Error("Incomplete video field");
        }
        this.acknowledgedBytes = data.bytes;
        this.pending.delete(data.sequence);
        item.resolve(data);
      } catch (error) { this.fail(error); }
    };
  }

  fail(error) {
    if (this.error) return;
    this.error = error;
    this.rejectReady(error);
    for (const item of this.pending.values()) item.reject(error);
    this.pending.clear();
    this.worker?.terminate();
  }

  activate() {
    if (this.closed || this.error) throw this.error ?? new Error("Video Worker is closed");
    globalThis.__saturnVideoChunk = bytes => {
      if (this.closed || this.error) throw this.error ?? new Error("Video Worker is closed");
      if (this.postedBytes - this.acknowledgedBytes + bytes.byteLength > this.maxBytes) {
        throw new Error("Video command queue capacity exceeded");
      }
      // The import receives a borrowed Wasm view. Copy before transferring;
      // neither Wasm memory nor a producer-owned reusable buffer is detached.
      const owned = bytes.slice();
      this.postedBytes += owned.byteLength;
      this.peakPendingBytes = Math.max(this.peakPendingBytes, this.postedBytes - this.acknowledgedBytes);
      this.worker.postMessage({ type: "chunk", sequence: this.chunkSequence++, bytes: owned }, [owned.buffer]);
    };
  }

  async waitForCapacity() {
    await this.ready;
    while (this.pending.size >= this.maxFrames) await this.pending.values().next().value.promise;
    if (this.closed || this.error) throw this.error ?? new Error("Video Worker is closed");
  }

  submit(fields, { capture = true, presentation = null } = {}) {
    if (this.closed || this.error) throw this.error ?? new Error("Video Worker is closed");
    if (this.pending.size >= this.maxFrames) throw new Error("Wait for video capacity before advancing");
    const sequence = this.frameSequence++;
    let resolve, reject;
    const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
    promise.catch(() => {});
    this.pending.set(sequence, { promise, resolve, reject, fields, chunks: this.chunkSequence, capture,
      direct: this.directPresentation && presentation !== null });
    this.peakPendingFrames = Math.max(this.peakPendingFrames, this.pending.size);
    this.worker.postMessage({ type: "frame", sequence, fields, capture, presentation });
    return promise;
  }

  close() {
    if (this.closed) return;
    this.closed = true;
    this.fail(new Error("Video Worker closed"));
    this.worker.terminate();
  }
}
