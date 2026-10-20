//! Only marshaling lives here; no browser APIs or emulation logic.
use crate::{Boot, BootMode, Machine, cpu::CpuStatus};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
#[derive(Default)]
pub struct Emulator {
    machine: Machine,
}

#[wasm_bindgen]
impl Emulator {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            machine: Machine::new(),
        }
    }
    pub fn reset_hle(&mut self) -> Result<(), JsValue> {
        self.machine.reset(Boot::HlePrototype).map_err(js_error)
    }
    pub fn reset_bios(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        self.machine
            .reset(Boot::ExternalBios(bytes))
            .map_err(js_error)
    }
    pub fn step(&mut self) -> Result<(), JsValue> {
        self.machine.step().map_err(js_error)
    }
    pub fn run_steps(&mut self, count: u32) -> Result<(), JsValue> {
        self.machine.run_steps(count).map_err(js_error)
    }
    pub fn registers(&self) -> Vec<u32> {
        self.machine.cpu_state().r.to_vec()
    }
    /// PC, SR, PR, GBR, VBR, MACH, MACL, in that order.
    pub fn special_registers(&self) -> Vec<u32> {
        let s = self.machine.cpu_state();
        vec![s.pc, s.sr, s.pr, s.gbr, s.vbr, s.mach, s.macl]
    }
    pub fn instruction_count(&self) -> String {
        self.machine.cpu_state().instructions.to_string()
    }
    pub fn delay_target(&self) -> Option<u32> {
        self.machine.cpu_state().delay_target
    }
    pub fn mode(&self) -> String {
        match self.machine.mode() {
            Some(BootMode::HlePrototype) => "HLE bootstrap prototype",
            Some(BootMode::ExternalBios) => "User-provided BIOS",
            None => "Not selected",
        }
        .into()
    }
    pub fn status(&self) -> String {
        match &self.machine.cpu_state().status {
            CpuStatus::Reset => "Reset — waiting for valid boot input".into(),
            CpuStatus::Running => "Ready".into(),
            CpuStatus::Halted => "Halted".into(),
            CpuStatus::Faulted(error) => format!("Faulted: {error}"),
        }
    }
}

fn js_error(error: crate::MachineError) -> JsValue {
    JsValue::from_str(&error.to_string())
}
