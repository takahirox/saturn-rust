//! Only marshaling lives here; no browser APIs or emulation logic.
use crate::{Boot, BootMode, Machine, cpu::CpuStatus};
use wasm_bindgen::prelude::*;

#[cfg(feature = "video-worker")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = globalThis, js_name = __saturnVideoChunk)]
    pub(crate) fn emit_video_chunk(bytes: &[u8]);
}

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
    pub fn reset_disc(&mut self, bytes: Vec<u8>) -> Result<(), JsValue> {
        let media = standalone_media(bytes);
        match media {
            Ok(media) => self.machine.reset_media(media).map_err(js_error),
            Err(error) => {
                let _ = self.machine.reset(Boot::HleDisc(&[]));
                Err(JsValue::from_str(&error.to_string()))
            }
        }
    }
    pub fn reset_loaded_disc(&mut self) -> Result<(), JsValue> {
        self.machine.reset_loaded_disc().map_err(js_error)
    }
    pub fn clear_media(&mut self) {
        self.machine.clear_media();
    }
    /// A NUL-separated name table, lengths and concatenated local file bytes
    /// marshal a complete CUE selection in one call. No path is opened here.
    pub fn reset_cue(
        &mut self,
        cue: &str,
        names: &str,
        lengths: &[u32],
        bytes: Vec<u8>,
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
        bytes: Vec<u8>,
    ) -> Result<(), JsValue> {
        let media = cue_media(cue, names, lengths, bytes)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        self.machine.mount_media(media);
        Ok(())
    }
    pub fn mount_disc(&mut self, bytes: Vec<u8>) -> Result<(), JsValue> {
        let media = standalone_media(bytes).map_err(|e| JsValue::from_str(&e.to_string()))?;
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
    pub fn set_3d_pad_digital(&mut self, port: u8, buttons: u16) {
        self.machine.set_3d_pad_digital(port as usize, buttons);
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
    /// Explicit bounded local fault context; does not read guest memory.
    pub fn smpc_fault_diagnostics(&self) -> Option<String> {
        self.machine.smpc_fault_diagnostics()
    }
    /// Explicit developer-only local trace; first 64 completed DSTP cycles.
    pub fn set_dstp_trace(&mut self, enabled: bool) {
        self.machine.set_dstp_trace(enabled);
    }
    pub fn dstp_trace(&self) -> Option<String> {
        self.machine.dstp_trace()
    }
    /// Read-only debugger access, also used to assert guest-written diagnostics.
    pub fn read_long(&self, address: u32) -> Result<u32, JsValue> {
        self.machine
            .debug_read32(address)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
    /// Word-width debugger access, including CD response registers.
    pub fn read_word(&self, address: u32) -> Result<u16, JsValue> {
        self.machine
            .debug_read16(address)
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
    bytes: Vec<u8>,
) -> Result<crate::media::Media, crate::disc::DiscError> {
    let names = names.split('\0');
    if lengths.len() > 99
        || bytes.len() > crate::disc::MAX_MEDIA_BYTES
        || names.clone().count() != lengths.len()
    {
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
        if end > bytes.len() {
            return Err(crate::disc::DiscError("Truncated CUE file table".into()));
        }
        files.push((name, offset..end));
        offset = end;
    }
    if offset != bytes.len() {
        return Err(crate::disc::DiscError(
            "Unexpected bytes after CUE file table".into(),
        ));
    }
    crate::media::Media::cue_owned(cue, &files, bytes)
}

// Dispatch by content as well as filename-independent native validation. A CHD
// extension never coerces a malformed CHD into a cooked ISO.
fn standalone_media(bytes: Vec<u8>) -> Result<crate::media::Media, crate::disc::DiscError> {
    if bytes.starts_with(b"MComprHD") {
        crate::media::Media::chd_owned(bytes)
    } else {
        crate::media::Media::iso_owned(bytes)
    }
}

/// Optional research harness, absent from default browser builds.
#[cfg(feature = "kernel-experiment")]
#[wasm_bindgen]
pub fn verify_owned_read_port() -> Vec<u32> {
    crate::kernel::verify_read_port_board()
}

#[cfg(feature = "kernel-experiment")]
#[wasm_bindgen]
pub fn verify_timed_board() -> Vec<u32> {
    crate::kernel::verify_timed_board()
}

#[cfg(feature = "kernel-experiment")]
#[wasm_bindgen]
pub fn verify_owned_store_board() -> Vec<u32> {
    crate::kernel::verify_store_board()
}

#[cfg(feature = "kernel-experiment")]
#[wasm_bindgen]
pub fn verify_active_wram_board() -> Vec<u32> {
    crate::kernel::verify_active_wram_board()
}

#[cfg(feature = "kernel-experiment")]
#[wasm_bindgen]
impl Emulator {
    pub fn advance_frame_probe(&mut self) -> Result<(), JsValue> {
        self.machine.advance_frame_probe().map_err(js_error)
    }
    pub fn enable_video_journal(&mut self, enabled: bool) -> bool {
        self.machine.enable_video_journal(enabled)
    }
    pub fn video_journal_enabled(&self) -> bool {
        self.machine.video_journal_enabled()
    }
    pub fn loop_probe_counts(&self) -> Vec<u32> {
        self.machine.loop_probe_counts()
    }
    pub fn loop_probe_periods(&self) -> Vec<u32> {
        self.machine.loop_probe_periods()
    }
}

#[cfg(feature = "video-worker")]
#[wasm_bindgen]
impl Emulator {
    pub fn enable_video_transport(&mut self, enabled: bool) -> bool {
        self.machine.enable_video_transport(enabled)
    }
    pub fn video_transport_enabled(&self) -> bool {
        self.machine.video_transport_enabled()
    }
    pub fn flush_video_output(&self) {
        self.machine.flush_video_output();
    }
    pub fn advance_video_field(&mut self) -> Result<(), JsValue> {
        self.machine.advance_video_field().map_err(js_error)
    }
}

/// Independent renderer instance for the optional owned-command Worker.
#[cfg(feature = "video-worker")]
#[wasm_bindgen]
pub struct VideoRenderer {
    inner: crate::bus::video_queue::Renderer,
}
#[cfg(feature = "video-worker")]
#[wasm_bindgen]
impl VideoRenderer {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: crate::bus::video_queue::Renderer::default(),
        }
    }
    pub fn replay(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        self.inner
            .replay_bytes(bytes)
            .map_err(|e| JsValue::from_str(&e))
    }
    pub fn frames(&self) -> u64 {
        self.inner.frames()
    }
    pub fn width(&self) -> u32 {
        self.inner.dimensions().0 as u32
    }
    pub fn height(&self) -> u32 {
        self.inner.dimensions().1 as u32
    }
    pub fn framebuffer(&self) -> Result<Vec<u8>, JsValue> {
        self.inner.framebuffer().map_err(|e| JsValue::from_str(&e))
    }
}
#[cfg(feature = "video-worker")]
impl Default for VideoRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// Original actual-board CPU experiment; not a game engine switch.
#[cfg(feature = "kernel-experiment")]
#[wasm_bindgen]
pub struct TimedKernelExperiment {
    inner: crate::kernel::TimedExperiment,
}

#[cfg(feature = "kernel-experiment")]
#[wasm_bindgen]
impl TimedKernelExperiment {
    #[wasm_bindgen(constructor)]
    pub fn new(memory: bool, dual: bool, bypass: bool) -> Self {
        Self {
            inner: crate::kernel::TimedExperiment::new(memory, dual, bypass),
        }
    }
    pub fn run_candidate_until(&mut self, end: u32) {
        self.inner.run_candidate_until(end);
    }
    pub fn run_reference(&mut self, fields: u32) {
        assert!((1..=10).contains(&fields));
        self.inner.run_reference(fields);
    }
    pub fn reference_clock(&self) -> u32 {
        self.inner.reference_clock()
    }
    pub fn verify(&mut self) {
        self.inner.verify();
    }
    pub fn digest(&self) -> Vec<u32> {
        self.inner.digest()
    }
}

/// Optional research harness, absent from default browser builds.
#[cfg(feature = "kernel-experiment")]
#[wasm_bindgen]
pub struct KernelExperiment {
    inner: crate::kernel::experiment::Experiment,
}
#[cfg(feature = "kernel-experiment")]
#[wasm_bindgen]
impl KernelExperiment {
    #[wasm_bindgen(constructor)]
    pub fn new(memory: bool, latency: u32) -> Self {
        Self {
            inner: crate::kernel::experiment::Experiment::new(memory, latency),
        }
    }
    pub fn run_kernel(&mut self, count: u32) -> Result<(), JsValue> {
        self.inner
            .run_kernel(count)
            .map_err(|error| JsValue::from_str(&error.to_string()))
    }
    pub fn run_reference(&mut self, count: u32) -> Result<(), JsValue> {
        self.inner
            .run_reference(count)
            .map_err(|error| JsValue::from_str(&error))
    }
    pub fn verify(&self) -> Result<(), JsValue> {
        self.inner
            .verify()
            .map_err(|error| JsValue::from_str(&error))
    }
    pub fn digest(&self) -> Vec<u32> {
        self.inner.digest()
    }
}

#[cfg(feature = "kernel-experiment")]
#[wasm_bindgen]
pub fn verify_loop_probe() -> Vec<u32> {
    crate::bus::verify_loop_probe()
}

#[cfg(feature = "kernel-experiment")]
#[wasm_bindgen]
impl Emulator {
    pub fn enable_sound_dependency_filter(&mut self, enabled: bool) {
        self.machine.enable_sound_dependency_filter(enabled);
    }
    pub fn enable_paired_irq_sampling(&mut self, enabled: bool) {
        self.machine.enable_paired_irq_sampling(enabled);
    }
    pub fn enable_dmac_pending_write_wait(&mut self, enabled: bool) {
        self.machine.enable_dmac_pending_write_wait(enabled);
    }
    pub fn enable_boundary_grant_reuse(&mut self, enabled: bool) {
        self.machine.bus.boundary_grant_reuse = enabled;
    }
    pub fn boundary_grant_reuse_enabled(&self) -> bool {
        self.machine.bus.boundary_grant_reuse
    }
    pub fn boundary_grant_reuse_counts(&self) -> Vec<u64> {
        self.machine.bus.boundary_grant_reuses.to_vec()
    }
    pub fn enable_cd_quiet_clock(&mut self, enabled: bool) {
        self.machine.bus.cd.set_quiet_clock_enabled(enabled);
    }
    pub fn cd_quiet_clock_enabled(&self) -> bool {
        self.machine.bus.cd.quiet_clock_enabled
    }
    pub fn cd_quiet_clock_steps(&self) -> u64 {
        self.machine.bus.cd.quiet_clock_steps
    }
    pub fn enable_direct_wram_access(&mut self, enabled: bool) {
        self.machine.bus.direct_wram_access = enabled;
    }
    pub fn direct_wram_access_enabled(&self) -> bool {
        self.machine.bus.direct_wram_access
    }
    pub fn direct_wram_access_counts(&self) -> Vec<u64> {
        vec![
            self.machine.bus.direct_wram_reads.get(),
            self.machine.bus.direct_wram_writes,
        ]
    }
    pub fn enable_borrowed_cycle_anchor(&mut self, enabled: bool) {
        self.machine.bus.cached_cycle_anchor_borrowed = enabled;
    }
    pub fn borrowed_cycle_anchor_enabled(&self) -> bool {
        self.machine.bus.cached_cycle_anchor_borrowed
    }
    pub fn borrowed_cycle_anchor_count(&self) -> u64 {
        self.machine.bus.cached_cycle_anchor_borrows
    }
    pub fn enable_cd_quiet_data(&mut self, enabled: bool) {
        self.machine.bus.set_cd_quiet_data(enabled);
    }
    pub fn cd_quiet_data_enabled(&self) -> bool {
        self.machine.bus.cd_quiet_data().0
    }
    pub fn cd_quiet_data_count(&self) -> u64 {
        self.machine.bus.cd_quiet_data().1
    }
    pub fn enable_smpc_quiet_interval(&mut self, enabled: bool) {
        self.machine.bus.set_smpc_quiet_interval(enabled);
    }
    pub fn smpc_quiet_interval_enabled(&self) -> bool {
        self.machine.bus.smpc_quiet_interval().0
    }
    pub fn smpc_quiet_interval_steps(&self) -> u64 {
        self.machine.bus.smpc_quiet_interval().1
    }
    pub fn enable_resident_branch_issues(&mut self, enabled: bool) {
        self.machine.bus.resident_branch_issues = enabled;
    }
    pub fn resident_branch_issues_enabled(&self) -> bool {
        self.machine.bus.resident_branch_issues
    }
    pub fn resident_branch_issue_count(&self) -> u64 {
        self.machine.bus.resident_branch_issue_count
    }
    pub fn dmac_pending_write_wait_enabled(&self) -> bool {
        self.machine.dmac_pending_write_wait_enabled()
    }
    pub fn dmac_pending_write_wait_counts(&self) -> Vec<u64> {
        self.machine.dmac_pending_write_wait_counts()
    }
    pub fn enable_local_journal(&mut self, enabled: bool) {
        self.machine.enable_local_journal(enabled);
    }
    pub fn enable_reversible_cached_loads(&mut self, enabled: bool) {
        self.machine.bus.reversible_cached_loads = enabled;
    }
    pub fn reversible_cached_loads_enabled(&self) -> bool {
        self.machine.bus.reversible_cached_loads
    }
    pub fn reversible_cached_load_counts(&self) -> Vec<u64> {
        self.machine.bus.reversible_cached_load_counts.to_vec()
    }
    pub fn local_journal_enabled(&self) -> bool {
        self.machine.local_journal_enabled()
    }
    pub fn local_journal_counts(&self) -> Vec<u64> {
        self.machine.local_journal_counts()
    }
    pub fn enable_prepared_cd_loads(&mut self, enabled: bool) {
        self.machine.bus.prepared_cd_load = enabled;
    }
    pub fn prepared_cd_loads_enabled(&self) -> bool {
        self.machine.bus.prepared_cd_load
    }
    pub fn prepared_cd_load_count(&self) -> u64 {
        self.machine.bus.prepared_cd_loads
    }
    pub fn paired_irq_reuses(&self) -> u64 {
        self.machine.paired_irq_reuses()
    }
    pub fn board_irq_reuses(&self) -> u64 {
        self.machine.board_irq_reuses()
    }
    pub fn loop_state_hashes(&self) -> Vec<u32> {
        self.machine.loop_state_hashes()
    }
    pub fn begin_sound_domain_trace(&mut self) -> Result<(), JsValue> {
        self.machine
            .begin_sound_domain_trace()
            .map_err(|e| JsValue::from_str(&e))
    }
    pub fn take_sound_domain_trace(&mut self) -> Vec<u32> {
        self.machine.take_sound_domain_trace()
    }
    pub fn sound_domain_snapshot(&self) -> Result<Vec<u8>, JsValue> {
        self.machine
            .sound_domain_snapshot()
            .map_err(JsValue::from_str)
    }

    pub fn restore_remote_sound_projection(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        self.machine
            .restore_remote_sound_projection(bytes)
            .map_err(JsValue::from_str)
    }
    pub fn begin_sound_projection(&mut self, clocks: u32) -> bool {
        self.machine.begin_sound_projection(clocks)
    }
    pub fn finish_sound_projection(&mut self) -> bool {
        self.machine.finish_sound_projection()
    }

    pub fn enable_sound_controller_independence(&mut self, enabled: bool) {
        self.machine.enable_sound_controller_independence(enabled);
    }
    pub fn enable_sound_cd_quiet(&mut self, enabled: bool) {
        self.machine.enable_sound_cd_quiet(enabled);
    }
    pub fn enable_sound_cd_queries(&mut self, enabled: bool) {
        self.machine.enable_sound_cd_queries(enabled);
    }
    pub fn enable_sound_cd_neutral_data(&mut self, enabled: bool) {
        self.machine.bus.cd.sound_data_commands = enabled;
    }
    pub fn sound_cd_neutral_data_enabled(&self) -> bool {
        self.machine.bus.cd.sound_data_commands
    }
    pub fn sound_cd_neutral_data_completions(&self) -> u64 {
        self.machine
            .bus
            .sound_rendezvous
            .cd_neutral_data_completions
    }
    pub fn sound_cd_queries_enabled(&self) -> bool {
        self.machine.sound_cd_queries_enabled()
    }
    pub fn sound_cd_query_counts(&self) -> Vec<u64> {
        self.machine.sound_cd_query_counts()
    }
    pub fn sound_cd_quiet_enabled(&self) -> bool {
        self.machine.sound_cd_quiet_enabled()
    }
    pub fn sound_cd_quiet_skips(&self) -> u64 {
        self.machine.sound_cd_quiet_skips()
    }
    pub fn sound_controller_independence_enabled(&self) -> bool {
        self.machine.sound_controller_independence_enabled()
    }
    pub fn enable_owned_texture_runs(&mut self, enabled: bool) {
        self.machine.enable_owned_texture_runs(enabled);
    }
    pub fn enable_private_bus_admission(&mut self, enabled: bool) {
        self.machine.enable_private_bus_admission(enabled);
    }
    pub fn private_bus_admission_counts(&self) -> Vec<u64> {
        self.machine.private_bus_admission_counts()
    }
    pub fn private_bus_admission_enabled(&self) -> bool {
        self.machine.private_bus_admission_enabled()
    }
    pub fn owned_texture_runs_enabled(&self) -> bool {
        self.machine.owned_texture_runs_enabled()
    }
    pub fn enable_vdp1_geometry_deadlines(&mut self, enabled: bool) {
        self.machine.enable_vdp1_geometry_deadlines(enabled);
    }
    pub fn vdp1_geometry_deadlines_enabled(&self) -> bool {
        self.machine.vdp1_geometry_deadlines_enabled()
    }
    pub fn vdp1_geometry_deadline_queries(&self) -> u64 {
        self.machine.vdp1_geometry_deadline_queries()
    }
    pub fn enable_display_erase_rows(&mut self, enabled: bool) {
        self.machine.enable_display_erase_rows(enabled);
    }
    pub fn display_erase_row_counts(&self) -> Vec<u64> {
        self.machine.display_erase_row_counts()
    }
    pub fn enable_short_cycle_learning(&mut self, enabled: bool) {
        self.machine.enable_short_cycle_learning(enabled);
    }
    pub fn short_cycle_learning_counts(&self) -> Vec<u64> {
        self.machine.short_cycle_learning_counts()
    }
    pub fn enable_pending_write_runtime(&mut self, enabled: bool) {
        self.machine.enable_pending_write_runtime(enabled);
    }
    pub fn pending_write_runtime_counts(&self) -> Vec<u64> {
        self.machine.pending_write_runtime_counts()
    }
    pub fn enable_live_write_runtime(&mut self, enabled: bool) {
        self.machine.enable_live_write_runtime(enabled);
    }
    pub fn live_write_runtime_counts(&self) -> Vec<u64> {
        self.machine.live_write_runtime_counts()
    }
    pub fn enable_live_write_probe(&mut self, enabled: bool) {
        self.machine.enable_live_write_probe(enabled);
    }
    pub fn live_write_probe_stats(&self) -> Vec<u64> {
        self.machine.live_write_probe_stats()
    }
    pub fn enable_loop_replay(&mut self, enabled: bool) {
        self.machine.enable_loop_replay(enabled);
    }
    pub fn enable_loop_training(&mut self, enabled: bool) {
        self.machine.enable_loop_training(enabled);
    }
    pub fn loop_replayed_instructions(&self) -> u64 {
        self.machine.loop_replayed_instructions()
    }
    pub fn loop_shortened_batches(&self) -> u64 {
        self.machine.loop_shortened_batches()
    }
    pub fn sound_gap_elisions(&self) -> u64 {
        self.machine.sound_gap_elisions()
    }
    pub fn loop_sound_elisions(&self) -> u64 {
        self.machine.loop_sound_elisions()
    }
    pub fn diagnose_loop_replay(&mut self, enabled: bool) {
        self.machine.diagnose_loop_replay(enabled);
    }
    pub fn loop_replay_mismatches(&self) -> Vec<u32> {
        self.machine.loop_replay_mismatches()
    }
    pub fn capture_loop_trace(&mut self) -> Result<Vec<u32>, JsValue> {
        self.machine.capture_loop_trace().map_err(js_error)
    }
}

#[cfg(feature = "kernel-experiment")]
#[derive(Default)]
#[wasm_bindgen]
pub struct SoundDomain {
    sound: crate::sound::Sound,
}

#[cfg(feature = "kernel-experiment")]
#[wasm_bindgen]
impl SoundDomain {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }
    pub fn replay_forecast(&mut self, words: &[u32]) -> Result<u32, JsValue> {
        self.sound
            .replay_domain_trace_forecast(words)
            .map_err(|e| JsValue::from_str(&e))
    }
    pub fn replay(&mut self, words: &[u32]) -> Result<(), JsValue> {
        self.sound
            .replay_domain_trace(words)
            .map_err(|e| JsValue::from_str(&e))
    }
    pub fn digest(&self) -> Vec<u32> {
        self.sound.domain_digest().to_vec()
    }
    pub fn snapshot(&self) -> Result<Vec<u8>, JsValue> {
        self.sound.domain_snapshot().map_err(JsValue::from_str)
    }
    pub fn load_state(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        self.sound
            .restore_domain_snapshot(bytes)
            .map_err(JsValue::from_str)
    }
}

#[cfg(feature = "kernel-experiment")]
#[wasm_bindgen]
pub struct SoundForecaster {
    forecast: crate::sound::forecast::Forecast,
}
#[cfg(feature = "kernel-experiment")]
#[wasm_bindgen]
impl SoundForecaster {
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: &[u8]) -> Result<SoundForecaster, JsValue> {
        let sound = crate::sound::Sound::from_domain_snapshot(bytes).map_err(JsValue::from_str)?;
        Ok(Self {
            forecast: crate::sound::forecast::Forecast::new(sound),
        })
    }
    pub fn extend(&mut self, through: u64) -> Result<(), JsValue> {
        self.forecast
            .extend(through)
            .map_err(|error| JsValue::from_str(&error.to_string()))
    }
    pub fn prefix(&self) -> Vec<u32> {
        let origin = self.forecast.origin();
        let through = self.forecast.through();
        let stable = self.forecast.stable_frontier();
        vec![
            origin as u32,
            (origin >> 32) as u32,
            through as u32,
            (through >> 32) as u32,
            stable as u32,
            (stable >> 32) as u32,
            u32::from(self.forecast.initial_irq()),
        ]
    }
    pub fn snapshot_at(&mut self, at: u64) -> Result<Vec<u8>, JsValue> {
        self.forecast
            .snapshot_at(at)
            .map_err(|error| JsValue::from_str(&error))
    }
}

#[cfg(feature = "sound-worker")]
#[derive(Default)]
#[wasm_bindgen]
pub struct SoundDeltaOwner {
    owner: crate::sound::delta::DeltaOwner,
}
#[cfg(feature = "sound-worker")]
#[wasm_bindgen]
impl SoundDeltaOwner {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }
    pub fn begin(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        self.owner.begin(bytes).map_err(JsValue::from_str)
    }
    pub fn extend(&mut self, to: u64) -> Result<(), JsValue> {
        self.owner
            .extend(to)
            .map_err(|error| JsValue::from_str(&error))
    }
    pub fn prefix(&self) -> Result<Vec<u32>, JsValue> {
        self.owner.prefix().map_err(JsValue::from_str)
    }
    pub fn snapshot_at(&mut self, at: u64) -> Result<Vec<u8>, JsValue> {
        self.owner
            .finish(at)
            .map_err(|error| JsValue::from_str(&error))
    }
}

#[cfg(feature = "sound-worker")]
#[wasm_bindgen]
impl Emulator {
    pub fn advance_sound_rendezvous_field(&mut self) -> Result<bool, JsValue> {
        self.machine
            .advance_sound_rendezvous_field()
            .map_err(js_error)
    }
    pub fn reset_sound_transport(&mut self) -> Result<(), JsValue> {
        self.machine
            .reset_sound_transport()
            .map_err(JsValue::from_str)
    }
    pub fn abort_sound_transport(&mut self) -> Result<(), JsValue> {
        self.machine.abort_sound_transport().map_err(js_error)
    }

    pub fn sound_worker_field_eligible(&self) -> bool {
        self.machine.sound_worker_field_eligible()
    }
    pub fn export_sound_delta(&mut self) -> Result<Vec<u8>, JsValue> {
        self.machine.export_sound_delta().map_err(JsValue::from_str)
    }
    pub fn restore_remote_sound_delta(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        self.machine
            .restore_remote_sound_delta(bytes)
            .map_err(JsValue::from_str)
    }
    pub fn remote_sound_projection_range(&self, clocks: u32) -> Vec<u32> {
        self.machine.remote_sound_projection_range(clocks)
    }
    pub fn accept_remote_sound_projection(
        &mut self,
        origin: u64,
        end: u64,
        through: u64,
        irq: bool,
    ) -> Result<(), JsValue> {
        self.machine
            .accept_remote_sound_projection(origin, end, through, irq)
            .map_err(JsValue::from_str)
    }
    pub fn extend_remote_sound_projection(&mut self, through: u64) -> Result<(), JsValue> {
        self.machine
            .extend_remote_sound_projection(through)
            .map_err(JsValue::from_str)
    }
    pub fn remote_sound_projection_time(&self) -> u64 {
        self.machine.remote_sound_projection_time()
    }
    pub fn advance_sound_projection_field(&mut self) -> Result<bool, JsValue> {
        self.machine
            .advance_sound_projection_field()
            .map_err(js_error)
    }
    pub fn sound_rendezvous_boundary(&self) -> Vec<u32> {
        self.machine.sound_rendezvous_boundary()
    }
    pub fn acknowledge_sound_rendezvous(&mut self) -> bool {
        self.machine.acknowledge_sound_rendezvous()
    }
}
