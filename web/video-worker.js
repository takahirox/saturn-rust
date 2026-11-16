// Optional original-Rust renderer. This Worker never owns guest firmware,
// CPU execution or authoritative device state; it consumes owned video data.
import init, { VideoRenderer } from "./pkg/saturn_rust.js";

let renderer;
let nextChunk = 0;
let nextFrame = 0;
let receivedBytes = 0;
function reportError(error) {
  self.postMessage({ type: "error", message: String(error?.message ?? error) });
  self.close();
}

let chain = init().then(() => {
  renderer = new VideoRenderer();
  self.postMessage({ type: "ready" });
}).catch(reportError);

self.onmessage = ({ data }) => {
  chain = chain.then(() => {
    if (data.type === "chunk") {
      if (data.sequence !== nextChunk++ || !(data.bytes instanceof Uint8Array)) {
        throw new Error("Video chunk order changed");
      }
      receivedBytes += data.bytes.byteLength;
      renderer.replay(data.bytes);
      return;
    }
    if (data.type !== "frame" || data.sequence !== nextFrame++) {
      throw new Error("Video frame order changed");
    }
    const frames = Number(renderer.frames());
    if (frames !== data.fields) throw new Error("Video field boundary changed");
    const width = renderer.width(), height = renderer.height();
    const pixels = data.capture ? renderer.framebuffer() : undefined;
    self.postMessage({ type: "frame", sequence: data.sequence, frames, width, height,
      chunks: nextChunk, bytes: receivedBytes, pixels }, pixels ? [pixels.buffer] : []);
  }).catch(reportError);
};
