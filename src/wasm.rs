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
    pub fn reset_disc(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        self.machine.reset(Boot::HleDisc(bytes)).map_err(js_error)
    }
    pub fn advance_frame(&mut self) -> Result<(), JsValue> {
        self.machine.advance_frame().map_err(js_error)
    }
    pub fn framebuffer(&self) -> Result<Vec<u8>, JsValue> {
        self.machine.framebuffer().map_err(js_error)
    }
    pub fn set_buttons(&mut self, buttons: u16) {
        self.machine.set_buttons(buttons);
    }
    pub fn set_port2_buttons(&mut self, buttons: u16) {
        self.machine.set_port2_buttons(buttons);
    }
    pub fn export_backup_ram(&self) -> Vec<u8> {
        self.machine.backup_ram().to_vec()
    }
    pub fn import_backup_ram(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        self.machine.import_backup_ram(bytes).map_err(js_error)
    }
    /// Read-only debugger access, also used to assert guest-written diagnostics.
    pub fn read_long(&self, address: u32) -> Result<u32, JsValue> {
        use crate::bus::Memory;
        self.machine
            .bus()
            .read32(address)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
    pub fn frames(&self) -> String {
        self.machine.frames().to_string()
    }
    pub fn peripheral_clocks(&self) -> String {
        self.machine.clocks().to_string()
    }
    pub fn slave_registers(&self) -> Vec<u32> {
        self.machine.slave_state().r.to_vec()
    }
    pub fn slave_instruction_count(&self) -> String {
        self.machine.slave_state().instructions.to_string()
    }
    pub fn slave_pc(&self) -> u32 {
        self.machine.slave_state().pc
    }
    pub fn slave_exception_count(&self) -> String {
        self.machine.slave_state().exceptions.to_string()
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
    pub fn exception_count(&self) -> String {
        self.machine.cpu_state().exceptions.to_string()
    }
    pub fn address_error_pending(&self) -> bool {
        self.machine.cpu_state().address_error_pending
    }
    pub fn hardware_wake_possible(&self) -> bool {
        self.machine.can_wake_master()
    }
    pub fn last_exception(&self) -> String {
        self.machine
            .cpu_state()
            .last_exception
            .as_ref()
            .map_or_else(
                || "none".into(),
                |e| format!("Vector {} · saved PC 0x{:08X}", e.vector, e.saved_pc),
            )
    }
    pub fn delay_target(&self) -> Option<u32> {
        self.machine.cpu_state().delay_target
    }
    pub fn mode(&self) -> String {
        match self.machine.mode() {
            Some(BootMode::HlePrototype) => "HLE bootstrap prototype",
            Some(BootMode::HleDisc) => "BIOS-less HLE disc",
            Some(BootMode::ExternalBios) => "User-provided BIOS",
            None => "Not selected",
        }
        .into()
    }
    pub fn status(&self) -> String {
        if let Some(error) = self.machine.fault() {
            return format!("Faulted: {error}");
        }
        match &self.machine.cpu_state().status {
            CpuStatus::Reset => "Reset — waiting for valid boot input".into(),
            CpuStatus::Running => "Ready".into(),
            CpuStatus::Sleeping => "Sleeping — waiting for an interrupt".into(),
            CpuStatus::Halted => "Halted".into(),
            CpuStatus::Faulted(error) => format!("Faulted: {error}"),
        }
    }
}

fn js_error(error: crate::MachineError) -> JsValue {
    JsValue::from_str(&error.to_string())
}
