import { PlaybackClient } from "./playback-client.js";

// The same Machine stays in its Worker during play, pause and debugging.
// Native method results are asynchronous; independent reads in one JS task
// share bounded batches without changing native ordering or error results.
export class WorkerEmulator {
  constructor({ onFrame = () => {}, onAudio = () => {}, onError = () => {}, directPresentation = true, persistSaves = false, onSaveStatus } = {}) {
    this.input = { buttons: 0, port2Buttons: 0, ports: [] };
    this.queued = [];
    this.commands = Promise.resolve();
    this.inputQueued = false;
    this.client = new PlaybackClient({ directPresentation, onAudio, onError, persistSaves, onSaveStatus,
      onFrame: packet => {
        this.acceptView({ ...packet.view, clocks: packet.clocks, frames: packet.fields, width: packet.width, height: packet.height }, packet.epoch);
        return onFrame(packet);
      },
    });
    this.ready = this.client.ready.then(() => {
      this.state = this.client.view;
      this.stateEpoch = this.client.epoch;
      for (const method of this.client.methods) {
        if (!(method in this)) this[method] = (...args) => this.invoke(method, args);
      }
    });
  }
  acceptView(view, epoch) {
    if (epoch !== this.client.epoch) return;
    if (epoch !== this.stateEpoch || view.clocks >= this.state.clocks) {
      this.state = { ...this.state, ...view };
      this.stateEpoch = epoch;
    }
  }
  invoke(method, args) {
    const promise = new Promise((resolve, reject) => {
      this.queued.push({ method, args, resolve, reject });
    });
    if (this.queued.length === 1) queueMicrotask(() => this.flush());
    return promise;
  }
  flush() {
    const pending = this.queued.splice(0);
    if (!pending.length) return;
    const execute = async () => {
      try {
        await this.ready;
        for (let offset = 0; offset < pending.length; offset += 4096) {
          const batch = pending.slice(offset, offset + 4096);
          this.flushInput();
          const response = await this.client.invoke(batch.map(({ method, args }) => ({ method, args })));
          this.acceptView(response.view, response.epoch);
          for (const [i, result] of response.values.entries()) {
            if (result.ok) batch[i].resolve(result.value);
            else batch[i].reject(new Error(result.error));
          }
        }
      } catch (error) { for (const item of pending) item.reject(error); }
    };
    // A later JS task cannot overtake the remaining chunks of an earlier batch.
    this.commands = this.commands.then(execute, execute);
  }
  queueInput() {
    if (this.inputQueued) return;
    this.inputQueued = true;
    queueMicrotask(() => this.flushInput());
  }
  flushInput() {
    if (!this.inputQueued) return;
    this.inputQueued = false;
    this.client.setInput(this.input);
  }
  set_buttons(value) { this.input.buttons = value; this.queueInput(); }
  set_port2_buttons(value) { this.input.port2Buttons = value; this.queueInput(); }
  set_analog(port, connected, x, y, throttle) {
    const state = this.input.ports[port] ??= {};
    delete state.pad3d;
    state.analog = { connected, x, y, throttle };
    this.queueInput();
  }
  set_3d_pad(port, connected, x, y, right, left) {
    const state = this.input.ports[port] ??= {};
    delete state.analog;
    state.pad3d = { connected, x, y, left, right };
    this.queueInput();
  }
  set_3d_pad_digital(port, digital) {
    (this.input.ports[port] ??= {}).digital = digital;
    this.queueInput();
  }
  async play() { await this.ready; this.flushInput(); return this.client.play(); }
  async pause({ checkpoint = true } = {}) {
    await this.ready;
    this.flushInput();
    const response = await this.client.call("view", { checkpoint });
    this.acceptView(response.view, response.epoch);
    return this.state;
  }
  async retrySaves() {
    const { view, epoch, resetRequired, transportError } = await this.client.retrySaves();
    this.acceptView(view, epoch);
    return { resetRequired, transportError };
  }
  async advanceField() {
    await this.ready;
    this.flushInput();
    await this.client.advance(1, { inspect: false });
    const response = await this.client.call("view");
    this.acceptView(response.view, response.epoch);
  }
  free() { this.client.close(); }
}
