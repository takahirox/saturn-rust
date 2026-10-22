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
    pub fn set_pal(&mut self, pal: bool) {
        self.machine.set_pal(pal);
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
    /// A NUL-separated name table, lengths and concatenated local file bytes
    /// marshal a complete CUE selection in one call. No path is opened here.
    pub fn reset_cue(
        &mut self,
        cue: &str,
        names: &str,
        lengths: &[u32],
        bytes: &[u8],
    ) -> Result<(), JsValue> {
        let result = cue_media(cue, names, lengths, bytes);
        match result {
            Ok(media) => self.machine.reset_media(media).map_err(js_error),
            Err(error) => {
                let _ = self.machine.reset(Boot::HleDisc(&[]));
                Err(JsValue::from_str(&error.to_string()))
            }
        }
    }
    /// Attach caller-supplied mixed-track media without resetting the CPU or
    /// installing HLE. This is the CUE counterpart of mount_disc for BIOS mode.
    pub fn mount_cue(
        &mut self,
        cue: &str,
        names: &str,
        lengths: &[u32],
        bytes: &[u8],
    ) -> Result<(), JsValue> {
        let media = cue_media(cue, names, lengths, bytes)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        self.machine.mount_media(media);
        Ok(())
    }
    pub fn mount_disc(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        let media =
            crate::media::Media::iso(bytes).map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.machine.mount_media(media);
        Ok(())
    }
    pub fn advance_frame(&mut self) -> Result<(), JsValue> {
        self.machine.advance_frame().map_err(js_error)
    }
    pub fn framebuffer(&self) -> Result<Vec<u8>, JsValue> {
        self.machine.framebuffer().map_err(js_error)
    }
    pub fn audio_samples(&mut self, frames: u32) -> Vec<i16> {
        self.machine.drain_audio(frames as usize)
    }
    pub fn audio_generated(&self) -> String {
        self.machine.audio_generated().to_string()
    }
    pub fn audio_dropped(&self) -> String {
        self.machine.audio_dropped().to_string()
    }
    pub fn sound_registers(&self) -> Vec<u32> {
        self.machine.sound_state().d.to_vec()
    }
    pub fn sound_pc(&self) -> u32 {
        self.machine.sound_state().pc
    }
    pub fn sound_instruction_count(&self) -> String {
        self.machine.sound_state().instructions.to_string()
    }
    pub fn set_region(&mut self, code: u8) -> Result<(), JsValue> {
        let region = crate::smpc::Region::try_from(code).map_err(JsValue::from_str)?;
        self.machine.set_region(region);
        Ok(())
    }
    pub fn set_analog(&mut self, port: u8, enabled: bool, x: u8, y: u8, z: u8) {
        self.machine
            .set_analog(port as usize, enabled.then_some([x, y, z]));
    }
    pub fn set_3d_pad(&mut self, port: u8, enabled: bool, x: u8, y: u8, right: u8, left: u8) {
        self.machine
            .set_3d_pad(port as usize, enabled.then_some([x, y, right, left]));
    }
    pub fn sound_halted(&self) -> bool {
        self.machine.sound_state().halted
    }
    pub fn set_peripheral_th(&mut self, port: u8, high: bool) {
        self.machine.set_peripheral_th(port as usize, high);
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
    pub fn video_width(&self) -> u32 {
        self.machine.video_dimensions().0 as u32
    }
    pub fn video_height(&self) -> u32 {
        self.machine.video_dimensions().1 as u32
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
        if self.machine.reset_in_progress() {
            return "Resetting — waiting for hardware reset vectors".into();
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

// Resolve only the caller's in-memory selection; never open a CUE path.
fn cue_media(
    cue: &str,
    names: &str,
    lengths: &[u32],
    bytes: &[u8],
) -> Result<crate::media::Media, crate::disc::DiscError> {
    let names = names.split('\0');
    if lengths.len() > 99 || bytes.len() > 134_217_728 || names.clone().count() != lengths.len() {
        return Err(crate::disc::DiscError(
            "Invalid local CUE file table".into(),
        ));
    }
    let mut files = Vec::new();
    let mut offset = 0usize;
    for (name, length) in names.zip(lengths) {
        let end = offset
            .checked_add(*length as usize)
            .ok_or_else(|| crate::disc::DiscError("CUE file length overflow".into()))?;
        files.push((
            name,
            bytes
                .get(offset..end)
                .ok_or_else(|| crate::disc::DiscError("Truncated CUE file table".into()))?,
        ));
        offset = end;
    }
    if offset != bytes.len() {
        return Err(crate::disc::DiscError(
            "Unexpected bytes after CUE file table".into(),
        ));
    }
    crate::media::Media::cue(cue, &files)
}
