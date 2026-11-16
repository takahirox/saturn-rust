// Exclusive original-Rust Sound owner. Each MessageChannel task computes a bounded prefix.
import init, { SoundDeltaOwner } from "./pkg/saturn_rust.js";
const clock = words => BigInt(words[0]) | BigInt(words[1]) << 32n;
const pumpChannel = new MessageChannel();
let actor = null, owned = false, job = 0, end = 0n, first = true, finished = false;
const ready = init().then(() => { actor=new SoundDeltaOwner(); self.postMessage({type:"ready"}); })
  .catch(error => self.postMessage({type:"error",job:0,message:String(error?.message??error).slice(0,1024)}));
pumpChannel.port1.onmessage = ({data}) => {
  if (!owned || data !== job || finished) return;
  try {
    const before=actor.prefix(), now=clock(before.slice(2,4));
    const next=now+8192n < end ? now+8192n : end;
    let fault=null;
    try { actor.extend(next); } catch(error) { fault=String(error?.message??error).slice(0,1024); }
    const prefix=actor.prefix(), through=clock(prefix.slice(2,4)), stable=clock(prefix.slice(4,6));
    finished=Boolean(fault) || through>=end || stable<through;
    if (first || finished || (through-clock(prefix.slice(0,2)))%32768n===0n) {
      self.postMessage({type:"prefix",job,prefix,done:finished,fault}); first=false;
    }
    if (!finished) pumpChannel.port2.postMessage(job);
  } catch(error) { self.postMessage({type:"error",job,message:String(error?.message??error).slice(0,1024)}); }
};
self.onmessage = async ({data}) => {
  try {
    await ready;
    if (data.type==="reset") {
      if (owned || !Number.isSafeInteger(data.job)) throw new Error("sound reset requires canonical ownership");
      actor.free(); actor=new SoundDeltaOwner(); job=data.job; finished=false;
      self.postMessage({type:"reset",job});
    } else if (data.type==="load") {
      if (owned || !(data.state instanceof Uint8Array) || !Number.isSafeInteger(data.job)) throw new Error("sound ownership violation");
      actor.begin(data.state); owned=true; job=data.job; end=data.end; first=true; finished=false;
      pumpChannel.port2.postMessage(job);
    } else if (data.type==="finish") {
      if (!owned || data.job!==job) throw new Error("stale sound finish");
      finished=true;
      const state=actor.snapshot_at(data.at); owned=false;
      self.postMessage({type:"state",job,state},[state.buffer]);
    } else throw new Error("unknown sound owner command");
  } catch(error) { self.postMessage({type:"error",job:data.job,message:String(error?.message??error).slice(0,1024)}); }
};
