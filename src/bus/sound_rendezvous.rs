//! Cooperative Sound input boundaries, exclusive-owner prefixes and recovery.
//! Main executes every shared effect only with canonical Sound state.
use super::*;
use std::collections::VecDeque;

#[derive(Clone, Copy)]
pub(crate) struct Boundary {
    pub kind: u32,
    pub id: u64,
    pub at: u64,
    pub address: u32,
    pub width: u32,
    pub write: Option<u32>,
}
#[derive(Default)]
pub(crate) struct Rendezvous {
    pub independent_controller: bool,
    pub enabled: bool,
    #[cfg(any(test, feature = "kernel-experiment"))]
    pub strict_inputs: bool,
    pub pending: VecDeque<Boundary>,
    pub target: Option<u64>,
    pub steps: u32,
    pub cooperative: bool,
    pub next_id: u64,
    pub active_id: u64,
    exposed: u64,
    forecast: Option<crate::sound::forecast::Forecast>,
    spare: Option<crate::sound::Sound>,
    remote_end: Option<u64>,
    delta: Option<crate::sound::delta::Delta>,
    delta_inflight: bool,
    pub last_input: u64,
    #[cfg(test)]
    pub projected_idle_batches: u64,
    #[cfg(test)]
    pub projected_idle_max: u32,
}
impl Rendezvous {
    pub(super) fn production() -> Self {
        Self {
            independent_controller: true,
            ..Self::default()
        }
    }
    pub fn expose(&mut self, boundary: Boundary) {
        if self.enabled && boundary.id > self.exposed {
            assert!(self.pending.len() < 64, "bounded sound rendezvous queue");
            self.exposed = boundary.id;
            self.pending.push_back(boundary);
        }
    }
}
impl Bus {
    fn strict_sound_inputs(&self) -> bool {
        #[cfg(any(test, feature = "kernel-experiment"))]
        {
            self.sound_rendezvous.strict_inputs
        }
        #[cfg(not(any(test, feature = "kernel-experiment")))]
        {
            false
        }
    }
    pub(crate) fn sound_input_address(&self, address: u32, write: Option<u32>) -> bool {
        let strict = self.strict_sound_inputs();
        (crate::sound::SOUND_RAM..crate::sound::SCSP + 0x1000).contains(&address)
            || ((0x0580_0000..0x0590_0000).contains(&address)
                && (strict || self.cd.sound_can_receive_cd_audio()))
            // Retain real controls and unknown/faulting command writes. Pure
            // controller data/INTBACK writes commute with owned Sound, including
            // an already admitted beat. Main still executes every SMPC edge.
            || ((SMPC..SMPC + 0x80).contains(&address)
                && (strict || write.is_some_and(|value| {
                    !self.sound_rendezvous.independent_controller
                        || !self.smpc.sound_independent_controller_write(address, value)
                })))
    }
    fn sound_smpc_deadline(&self) -> u32 {
        if self.sound_rendezvous.independent_controller && !self.strict_sound_inputs() {
            self.smpc.sound_controller_deadline()
        } else {
            self.smpc.clocks_to_event()
        }
    }
    /// Stay strictly before every possible new bus grant/device input and
    /// before the verified Sound frontier. The next event uses one real tick.
    pub(crate) fn projected_idle_span(&self, requested: u32) -> u32 {
        if !self.sound.projected() {
            return requested.min(1);
        }
        let fixed = self
            .sound
            .cached_cpu_deadline()
            .min(self.cd.clocks_to_event())
            .min(self.smpc.clocks_to_event())
            .saturating_sub(1);
        let main = self
            .system_clock
            .preview(fixed)
            .saturating_add(self.pending_main_clocks);
        requested
            .min(main)
            .min(
                self.main_deadline()
                    .min(self.bus_transaction_deadline())
                    .saturating_sub(1),
            )
            .max(1)
    }
    pub(crate) fn sound_rendezvous_blocked(&self) -> bool {
        !self.sound_rendezvous.pending.is_empty() || self.sound.projection_at_frontier()
    }
    pub(crate) fn sound_control_boundary(&mut self, command: u8) {
        if !self.sound_rendezvous.enabled {
            return;
        }
        self.sound_rendezvous.last_input = self.sound.domain_reference_time();
        self.sound_rendezvous.next_id += 1;
        self.sound_rendezvous.expose(Boundary {
            kind: 1,
            id: self.sound_rendezvous.next_id,
            at: self.sound.domain_reference_time(),
            address: u32::from(command),
            width: 0,
            write: None,
        });
    }
}
impl crate::Machine {
    #[cfg(any(test, feature = "kernel-experiment"))]
    pub fn enable_sound_controller_independence(&mut self, enabled: bool) {
        self.bus.sound_rendezvous.independent_controller = enabled;
    }
    #[cfg(any(test, feature = "kernel-experiment"))]
    pub fn sound_controller_independence_enabled(&self) -> bool {
        self.bus.sound_rendezvous.independent_controller
    }
    #[cfg(any(test, feature = "kernel-experiment"))]
    pub fn enable_sound_dependency_filter(&mut self, enabled: bool) {
        self.bus.sound_rendezvous.strict_inputs = !enabled;
    }
    fn sound_projection_range(&self, clocks: u32) -> Option<(u64, u64)> {
        if self.mode != Some(crate::BootMode::ExternalBios)
            || self.bus.sound.projected()
            || self.bus.sound_rendezvous.forecast.is_some()
            || !self.bus.sound_rendezvous.pending.is_empty()
            || !self.bus.sound.main_port_idle()
            || self.bus.sound_ram_request().is_some()
            || self.bus.sound_input_inflight()
            || !self.bus.cd.audio_quiet()
        {
            return None;
        }
        let clocks = clocks
            .min(524288)
            .min(
                self.bus
                    .cd
                    .sound_source_deadline(self.bus.strict_sound_inputs())
                    .saturating_sub(1),
            )
            .min(self.bus.sound_smpc_deadline().saturating_sub(1));
        let origin = self.bus.sound.domain_reference_time();
        (clocks >= 512).then_some((origin, origin + u64::from(clocks)))
    }
    /// Same admission for an external exclusive Sound owner. No emulation runs.
    /// Prefer the existing whole-field path for disabled sound CPUs and HLE.
    /// This is a cost policy; synthesis still runs accurately on that path.
    pub fn sound_worker_field_eligible(&self) -> bool {
        self.mode == Some(crate::BootMode::ExternalBios) && self.bus.sound.enabled
    }
    pub fn remote_sound_projection_range(&self, clocks: u32) -> Vec<u32> {
        if self
            .bus
            .sound
            .domain_reference_time()
            .saturating_sub(self.bus.sound_rendezvous.last_input)
            < 512
        {
            return Vec::new();
        }
        self.sound_projection_range(clocks)
            .map_or_else(Vec::new, |(origin, end)| {
                vec![
                    origin as u32,
                    (origin >> 32) as u32,
                    end as u32,
                    (end >> 32) as u32,
                    u32::from(self.bus.sound.main_interrupt()),
                ]
            })
    }
    pub fn accept_remote_sound_projection(
        &mut self,
        origin: u64,
        end: u64,
        through: u64,
        irq: bool,
    ) -> Result<(), &'static str> {
        let span = end
            .checked_sub(origin)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or("invalid remote sound range")?;
        if self.sound_projection_range(span) != Some((origin, end))
            || through <= origin
            || through > end
            || irq != self.bus.sound.main_interrupt()
        {
            return Err("remote sound prefix does not match main state");
        }
        self.bus
            .sound
            .install_projection(origin, through, irq, self.bus.sound.enabled);
        self.bus.sound_rendezvous.remote_end = Some(end);
        Ok(())
    }
    pub fn extend_remote_sound_projection(&mut self, through: u64) -> Result<(), &'static str> {
        if !self
            .bus
            .sound_rendezvous
            .remote_end
            .is_some_and(|end| through <= end)
            || !self.bus.sound.extend_projection(through)
        {
            return Err("remote sound extension outside admitted range");
        }
        Ok(())
    }
    pub fn remote_sound_projection_time(&self) -> u64 {
        self.bus.sound.domain_reference_time()
    }
    /// Reset host transport only after its real Sound state is canonical.
    pub fn reset_sound_transport(&mut self) -> Result<(), &'static str> {
        if self.bus.sound.projected()
            || self.bus.sound_rendezvous.delta_inflight
            || self.bus.sound_rendezvous.forecast.is_some()
        {
            return Err("sound transport is still active");
        }
        self.bus.sound_rendezvous.delta = None;
        Ok(())
    }
    /// The main board retained the real Sound state at the remote origin.
    /// No shared input has executed since then, so failed transport can be
    /// recovered by recomputing that known prefix without rolling back Main.
    pub fn abort_sound_transport(&mut self) -> Result<(), crate::MachineError> {
        if self.bus.sound_rendezvous.remote_end.is_some() {
            self.bus.sound.recover_remote_projection()?;
            self.bus
                .sound
                .main_ram_request(self.bus.sound_ram_request());
        } else {
            assert!(
                !self.bus.sound.projected(),
                "local forecast needs its own owner"
            );
        }
        self.bus.sound_rendezvous.remote_end = None;
        self.bus.sound_rendezvous.delta_inflight = false;
        self.bus.sound_rendezvous.delta = None;
        Ok(())
    }
    pub fn export_sound_delta(&mut self) -> Result<Vec<u8>, &'static str> {
        if self.bus.sound_rendezvous.delta_inflight {
            return Err("sound delta already exported");
        }
        let bytes = self
            .bus
            .sound_rendezvous
            .delta
            .get_or_insert_with(Default::default)
            .export(&mut self.bus.sound)?;
        self.bus.sound_rendezvous.delta_inflight = true;
        Ok(bytes)
    }
    pub fn restore_remote_sound_delta(&mut self, bytes: &[u8]) -> Result<(), &'static str> {
        if !self.bus.sound_rendezvous.delta_inflight
            || (self.bus.sound_rendezvous.remote_end.is_some() != self.bus.sound.projected())
        {
            return Err("main has no pending sound delta return");
        }
        let at = self.bus.sound.domain_reference_time();
        self.bus
            .sound_rendezvous
            .delta
            .as_mut()
            .ok_or("sound delta session missing")?
            .import(&mut self.bus.sound, bytes, Some(at))?;
        self.bus
            .sound
            .main_ram_request(self.bus.sound_ram_request());
        self.bus.sound_rendezvous.remote_end = None;
        self.bus.sound_rendezvous.delta_inflight = false;
        Ok(())
    }
    pub fn restore_remote_sound_projection(&mut self, bytes: &[u8]) -> Result<(), &'static str> {
        if self.bus.sound_rendezvous.remote_end.is_none() || !self.bus.sound.projected() {
            return Err("main has no remote sound owner");
        }
        let mut restored = crate::sound::Sound::from_domain_snapshot(bytes)?;
        if restored.domain_reference_time() != self.bus.sound.domain_reference_time() {
            return Err("remote sound state is not at main committed time");
        }
        restored.main_ram_request(self.bus.sound_ram_request());
        self.bus.sound = restored;
        self.bus.sound_rendezvous.remote_end = None;
        Ok(())
    }
    /// Local proof of the main-board/forecast boundary. This still computes
    /// Sound sequentially; it does not claim Worker throughput improvement.
    pub fn begin_sound_projection(&mut self, clocks: u32) -> bool {
        let Some((origin, end)) = self.sound_projection_range(clocks) else {
            return false;
        };
        let irq = self.bus.sound.main_interrupt();
        let enabled = self.bus.sound.enabled;
        let spare = self.bus.sound_rendezvous.spare.take().unwrap_or_default();
        let sound = std::mem::replace(&mut self.bus.sound, spare);
        let mut forecast = crate::sound::forecast::Forecast::new(sound);
        if forecast.extend(end).is_err() {
            forecast.rewind(origin).expect("retained forecast origin");
            self.bus.sound_rendezvous.spare = Some(std::mem::replace(
                &mut self.bus.sound,
                forecast.into_sound(),
            ));
            return false;
        }
        let through = forecast
            .irq_timeline()
            .get(1)
            .map_or(end, |&(at, _)| at - 1);
        if through == origin {
            forecast.rewind(origin).expect("retained forecast origin");
            self.bus.sound_rendezvous.spare = Some(std::mem::replace(
                &mut self.bus.sound,
                forecast.into_sound(),
            ));
            return false;
        }
        self.bus
            .sound
            .install_projection(origin, through, irq, enabled);
        self.bus.sound_rendezvous.forecast = Some(forecast);
        true
    }
    pub fn finish_sound_projection(&mut self) -> bool {
        let Some(mut forecast) = self.bus.sound_rendezvous.forecast.take() else {
            return false;
        };
        forecast
            .rewind(self.bus.sound.domain_reference_time())
            .expect("main stayed within verified sound prefix");
        self.bus.sound_rendezvous.spare = Some(std::mem::replace(
            &mut self.bus.sound,
            forecast.into_sound(),
        ));
        self.bus
            .sound
            .main_ram_request(self.bus.sound_ram_request());
        true
    }
    /// Permit a short synchronous prefix before reconsidering offload.
    /// This host yield is not a hardware input and requires no acknowledgement.
    pub fn advance_sound_projection_field(&mut self) -> Result<bool, crate::MachineError> {
        self.bus.sound_rendezvous.cooperative = !self.bus.sound.projected();
        let result = self.advance_sound_rendezvous_field();
        self.bus.sound_rendezvous.cooperative = false;
        result
    }
    /// Finish one exact field, yielding at newly admitted shared sound work.
    /// Explicit acknowledgement is required before continuing a yielded field.
    pub fn advance_sound_rendezvous_field(&mut self) -> Result<bool, crate::MachineError> {
        let next = self.frames() + 1;
        let target = *self.bus.sound_rendezvous.target.get_or_insert(next);
        self.bus.sound_rendezvous.enabled = true;
        let steps = self.bus.sound_rendezvous.steps;
        let done = self.advance_frame_bounded::<true, true>(target, steps)?;
        if done {
            self.finish_sound_projection();
            self.bus.sound_rendezvous.enabled = false;
            self.bus.sound_rendezvous.target = None;
            self.bus.sound_rendezvous.steps = 0;
        }
        Ok(done)
    }
    pub fn sound_rendezvous_boundary(&self) -> Vec<u32> {
        self.bus
            .sound_rendezvous
            .pending
            .front()
            .map_or_else(Vec::new, |b| {
                vec![
                    b.kind,
                    b.id as u32,
                    (b.id >> 32) as u32,
                    b.at as u32,
                    (b.at >> 32) as u32,
                    b.address,
                    b.width,
                    b.write.unwrap_or(0),
                    u32::from(b.write.is_some()),
                ]
            })
    }
    pub fn acknowledge_sound_rendezvous(&mut self) -> bool {
        self.bus.sound_rendezvous.pending.pop_front().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Boot, Machine};
    fn image(source: &str) -> Vec<u8> {
        let mut bytes = vec![0; BIOS_SIZE];
        for line in source.lines().filter(|line| !line.starts_with('#')) {
            let mut words = line.split_whitespace();
            let Some(at) = words.next() else { continue };
            let at = usize::from_str_radix(at, 16).unwrap();
            for (i, word) in words.enumerate() {
                bytes[at + i * 2..at + i * 2 + 2]
                    .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
            }
        }
        bytes
    }
    fn same(reference: &mut Machine, paused: &mut Machine) {
        assert_eq!(reference.cpu, paused.cpu);
        assert_eq!(reference.slave, paused.slave);
        assert_eq!(reference.clocks, paused.clocks);
        assert_eq!(reference.cpu_ready, paused.cpu_ready);
        assert_eq!(reference.loop_state_hashes(), paused.loop_state_hashes());
        assert!(
            reference.bus.same_cpu_journals(&paused.bus),
            "CPU replay journal changed"
        );
        assert_eq!(reference.bus.scu.interrupt(), paused.bus.scu.interrupt());
        assert_eq!(
            reference.bus.scu.read32(crate::scu::SCU + 0xA4),
            paused.bus.scu.read32(crate::scu::SCU + 0xA4)
        );
        for source in 0..32 {
            assert_eq!(
                reference.bus.scu.device_signal_pending(source),
                paused.bus.scu.device_signal_pending(source)
            );
        }
        assert_eq!(
            reference.bus.sound.domain_digest(),
            paused.bus.sound.domain_digest()
        );
        assert_eq!(
            reference.framebuffer().unwrap(),
            paused.framebuffer().unwrap()
        );
        assert_eq!(reference.drain_audio(8192), paused.drain_audio(8192));
    }
    #[test]
    fn controller_independence_restores_owned_sound_on_rejected_main_accesses() {
        let mut rejected_while_owned = 0;
        for source in [
            include_str!("../../tests/fixtures/smpc-fault-parameters.hex"),
            include_str!("../../tests/fixtures/smpc-fault-busy-set.hex"),
            include_str!("../../tests/fixtures/smpc-fault-invalid-sf.hex"),
        ] {
            for (pal, fast) in [(false, false), (false, true), (true, false), (true, true)] {
                let mut rom = image(source);
                let entry = u32::from_be_bytes(rom[..4].try_into().unwrap());
                rom[..4].copy_from_slice(&0x10000u32.to_be_bytes());
                let mut words = vec![0x0009u16; 1024];
                words.extend([
                    0xD101,
                    0x412B,
                    0xE100,
                    0x0009,
                    (entry >> 16) as u16,
                    entry as u16,
                ]);
                for (i, word) in words.into_iter().enumerate() {
                    rom[0x10000 + 2 * i..0x10002 + 2 * i].copy_from_slice(&word.to_be_bytes());
                }
                let (mut reference, mut projected) = (Machine::new(), Machine::new());
                for m in [&mut reference, &mut projected] {
                    m.reset(Boot::ExternalBios(&rom)).unwrap();
                    m.set_pal(pal);
                    m.bus.change_system_clock(fast);
                    m.bus.sound.ram[..4].copy_from_slice(&0x7000u32.to_be_bytes());
                    m.bus.sound.ram[4..8].copy_from_slice(&0x100u32.to_be_bytes());
                    m.bus.sound.ram[0x100..0x104].copy_from_slice(&[0x4E, 0x71, 0x60, 0xFC]);
                    m.bus.write8(SMPC + 0x63, 1).unwrap();
                    m.bus.write8(SMPC + 0x1F, 6).unwrap();
                }
                projected.enable_sound_controller_independence(true);
                let expected = reference.advance_video_field().unwrap_err();
                let actual = loop {
                    let owned = projected.begin_sound_projection(32768);
                    let result = projected.advance_sound_projection_field();
                    let rejected_owned = owned && result.is_err();
                    projected.finish_sound_projection();
                    if rejected_owned {
                        assert!(projected.bus.sound.enabled);
                        assert!(projected.bus.sound.cpu.state.instructions > 0);
                        rejected_while_owned += 1;
                    }
                    match result {
                        Err(error) => break error,
                        Ok(done) => {
                            assert!(!done, "original rejected-access guest must fail");
                        }
                    }
                    projected.acknowledge_sound_rendezvous();
                };
                assert_eq!(format!("{expected:?}"), format!("{actual:?}"));
                same(&mut reference, &mut projected);
            }
        }
        assert!(
            rejected_while_owned > 0,
            "must reject actual Main writes while the Sound CPU is owned"
        );
    }
    #[test]
    fn controller_filter_keeps_sound_ports_controls_and_strict_negative_control() {
        let mut m = Machine::new();
        for independent in [false, true] {
            m.enable_sound_controller_independence(independent);
            for strict in [false, true] {
                m.enable_sound_dependency_filter(!strict);
                assert_eq!(
                    m.bus.sound_input_address(SMPC + 0x63, Some(1)),
                    !independent || strict
                );
                assert_eq!(
                    m.bus.sound_input_address(SMPC + 0x1F, Some(0x10)),
                    !independent || strict
                );
                for command in [0, 2, 3, 6, 7, 8, 9, 0x0D, 0x0E, 0x0F, 0xFF] {
                    assert!(m.bus.sound_input_address(SMPC + 0x1F, Some(command)));
                }
                assert!(m.bus.sound_input_address(SMPC + 0x63, Some(0)));
                assert!(
                    m.bus
                        .sound_input_address(crate::sound::SOUND_RAM + 0x404, None)
                );
                assert!(
                    m.bus
                        .sound_input_address(crate::sound::SCSP + 0x42A, Some(0))
                );
            }
        }
    }
    #[test]
    fn dependency_filter_retains_writes_and_all_audio_track_accesses() {
        let mut machine = Machine::new();
        for strict in [false, true] {
            machine.enable_sound_dependency_filter(!strict);
            assert_eq!(machine.bus.sound_input_address(SMPC + 0x63, None), strict);
            assert!(machine.bus.sound_input_address(SMPC + 0x1F, Some(6)));
            assert!(
                machine
                    .bus
                    .sound_input_address(crate::sound::SCSP + 0x400, None)
            );
            assert_eq!(
                machine.bus.sound_input_address(crate::cd::REGISTERS, None),
                strict
            );
        }
        let data = vec![0; 2352 * 3];
        for mode in ["MODE1/2352", "AUDIO"] {
            let cue = format!("FILE \"a\" BINARY\nTRACK 01 {mode}\nINDEX 01 00:00:00");
            machine.mount_media(crate::media::Media::cue(&cue, &[("a", &data)]).unwrap());
            machine.enable_sound_dependency_filter(true);
            for write in [None, Some(0x100)] {
                assert_eq!(
                    machine.bus.sound_input_address(crate::cd::REGISTERS, write),
                    mode == "AUDIO"
                );
            }
        }
    }
    #[test]
    fn ram_and_register_requests_pause_without_clock_motion_or_repeated_admission() {
        for target in [crate::sound::SOUND_RAM + 0x1000, crate::sound::SCSP + 0x400] {
            for fast in [false, true] {
                let mut rom = image(include_str!("../../examples/runtime-loop.hex"));
                rom[0x224..0x228].copy_from_slice(&target.to_be_bytes());
                let (mut reference, mut paused) = (Machine::new(), Machine::new());
                for m in [&mut reference, &mut paused] {
                    m.reset(Boot::ExternalBios(&rom)).unwrap();
                    m.bus.change_system_clock(fast);
                }
                let mut boundaries = 0;
                let mut prior_id = 0;
                for _ in 0..2 {
                    reference.advance_video_field().unwrap();
                    while !paused.advance_sound_rendezvous_field().unwrap() {
                        let b = paused.sound_rendezvous_boundary();
                        assert_eq!(b.len(), 9);
                        let id = u64::from(b[1]) | u64::from(b[2]) << 32;
                        let at = u64::from(b[3]) | u64::from(b[4]) << 32;
                        assert_eq!(
                            at,
                            paused.bus.sound.domain_reference_time(),
                            "yield must precede the next sound clock"
                        );
                        assert!(id > prior_id);
                        prior_id = id;
                        if boundaries < 3 {
                            let clock = paused.clocks;
                            let cpu = paused.cpu.clone();
                            let digest = paused.bus.sound.domain_digest();
                            assert!(!paused.advance_sound_rendezvous_field().unwrap());
                            assert_eq!(paused.clocks, clock);
                            assert_eq!(paused.cpu, cpu);
                            assert_eq!(paused.bus.sound.domain_digest(), digest);
                            let state = paused.sound_domain_snapshot().unwrap();
                            paused.bus.sound.restore_domain_snapshot(&state).unwrap();
                            assert_eq!(paused.bus.sound.domain_digest(), digest);
                        }
                        assert!(paused.acknowledge_sound_rendezvous());
                        boundaries += 1;
                        assert!(boundaries < 200000);
                    }
                    same(&mut reference, &mut paused);
                }
                assert!(boundaries > 100, "must cover repeated physical bus beats");
            }
        }
    }
    #[test]
    fn busy_sound_video_guest_and_smpc_control_keep_field_results() {
        let rom = image(include_str!(
            "../../examples/runtime-poll-sound-busy-scene.hex"
        ));
        let (mut reference, mut paused) = (Machine::new(), Machine::new());
        for m in [&mut reference, &mut paused] {
            m.reset(Boot::ExternalBios(&rom)).unwrap();
        }
        for _ in 0..2 {
            reference.advance_video_field().unwrap();
            while !paused.advance_sound_rendezvous_field().unwrap() {
                assert!(paused.acknowledge_sound_rendezvous());
            }
            same(&mut reference, &mut paused);
        }
        paused.bus.sound_rendezvous.enabled = true;
        for command in [0x06, 0x07, 0x0E, 0x0F, 0x0D] {
            paused.complete_hardware_boundary(Some(command)).unwrap();
            let boundary = paused.sound_rendezvous_boundary();
            assert_eq!(boundary[0], 1);
            assert_eq!(boundary[5], u32::from(command));
            assert!(paused.acknowledge_sound_rendezvous());
        }
    }
    #[test]
    fn idle_cd_reports_need_no_sound_handoff_but_new_play_commands_still_do() {
        let rom = image(include_str!("../../tests/fixtures/cdda.hex"));
        let mut audio = vec![0; 2352 * 3];
        for frame in audio.as_chunks_mut::<4>().0 {
            frame[..2].copy_from_slice(&12000i16.to_le_bytes());
            frame[2..].copy_from_slice(&(-6000i16).to_le_bytes());
        }
        let media = crate::media::Media::cue(
            "FILE \"a\" BINARY\nTRACK 01 AUDIO\nINDEX 01 00:00:00",
            &[("a", &audio)],
        )
        .unwrap();
        let mut idle_elisions = 0;
        let mut nonzero = false;
        for fast in [false, true] {
            let (mut reference, mut projected) = (Machine::new(), Machine::new());
            for m in [&mut reference, &mut projected] {
                m.reset(Boot::ExternalBios(&rom)).unwrap();
                m.mount_media(media.clone());
                m.bus.change_system_clock(fast);
            }
            for _ in 0..4 {
                reference.advance_video_field().unwrap();
                loop {
                    let idle = !projected.bus.cd.active();
                    let cd_deadline = projected.bus.cd.clocks_to_event();
                    if projected.begin_sound_projection(524288)
                        && idle
                        && projected.bus.sound.cached_cpu_deadline() > cd_deadline
                    {
                        idle_elisions += 1;
                    }
                    if projected.advance_sound_projection_field().unwrap() {
                        break;
                    }
                    projected.finish_sound_projection();
                    projected.acknowledge_sound_rendezvous();
                }
                let expected = reference.drain_audio(8192);
                nonzero |= expected.iter().any(|&sample| sample != 0);
                assert_eq!(projected.drain_audio(8192), expected);
                assert_eq!(projected.bus.cd.interrupt(), reference.bus.cd.interrupt());
                assert_eq!(
                    projected.bus.cd.peek16(crate::cd::REGISTERS + 8),
                    reference.bus.cd.peek16(crate::cd::REGISTERS + 8)
                );
                same(&mut reference, &mut projected);
            }
        }
        assert!(idle_elisions > 0);
        assert!(
            nonzero,
            "original guest starts and routes real original CD-DA samples"
        );
    }

    #[test]
    fn delta_owner_returns_exact_sound_across_main_inputs_and_field_drains() {
        check_delta_owner_controller(false);
    }
    #[test]
    fn controller_independence_keeps_shared_inputs_device_edges_and_transport_recovery() {
        check_delta_owner_controller(true);
    }
    fn check_delta_owner_controller(independent: bool) {
        let mut controller_crossings = 0;
        let mut jobs = 0;
        let mut extensions = 0;
        let mut lost_before_accept = 0;
        let mut lost_during_execution = 0;
        let mut lost_at_field_end = 0;
        let mut idle_batches = 0;
        let mut idle_max = 0;
        for source in [
            include_str!("../../examples/runtime-poll-sound-busy-scene.hex"),
            include_str!("../../tests/fixtures/scsp-cpu-memory.hex"),
            include_str!("../../tests/fixtures/scsp-clock.hex"),
            include_str!("../../tests/fixtures/scu-fringe.hex"),
            include_str!("../../tests/fixtures/frt-compare.hex"),
            include_str!("../../tests/fixtures/smpc-status.hex"),
            include_str!("../../tests/fixtures/smpc-sf-activity.hex"),
            include_str!("../../tests/fixtures/smpc-boundaries.hex"),
        ] {
            for (pal, fast) in [(false, false), (false, true), (true, false), (true, true)] {
                if pal && !independent {
                    continue;
                }
                let rom = image(source);
                let (mut reference, mut remote) = (Machine::new(), Machine::new());
                for m in [&mut reference, &mut remote] {
                    m.reset(Boot::ExternalBios(&rom)).unwrap();
                    m.set_pal(pal);
                    m.bus.change_system_clock(fast);
                    m.enable_sound_controller_independence(independent);
                }
                let mut owner = crate::sound::delta::DeltaOwner::default();
                let mut active = false;
                let mut frontier = 0;
                for _ in 0..3 {
                    reference.advance_video_field().unwrap();
                    loop {
                        if !active {
                            let range = remote.remote_sound_projection_range(32768);
                            if !range.is_empty() {
                                let origin = u64::from(range[0]) | u64::from(range[1]) << 32;
                                let end = u64::from(range[2]) | u64::from(range[3]) << 32;
                                if jobs % 5 == 0 {
                                    let before = remote.sound_domain_snapshot().unwrap();
                                    remote.export_sound_delta().unwrap();
                                    assert!(remote.reset_sound_transport().is_err());
                                    remote.abort_sound_transport().unwrap();
                                    assert_eq!(remote.sound_domain_snapshot().unwrap(), before);
                                    owner = crate::sound::delta::DeltaOwner::default();
                                    lost_before_accept += 1;
                                }
                                let bytes = remote.export_sound_delta().unwrap();
                                assert!(remote.export_sound_delta().is_err());
                                owner.begin(&bytes).unwrap();
                                owner.extend(end).unwrap();
                                let prefix = owner.prefix().unwrap();
                                let stable = u64::from(prefix[4]) | u64::from(prefix[5]) << 32;
                                if stable > origin {
                                    frontier = stable.min(origin + 8192);
                                    remote
                                        .accept_remote_sound_projection(
                                            origin,
                                            end,
                                            frontier,
                                            prefix[6] != 0,
                                        )
                                        .unwrap();
                                    if independent
                                        && frontier - origin
                                            > u64::from(remote.bus.smpc.clocks_to_event())
                                    {
                                        controller_crossings += 1;
                                    }
                                    active = true;
                                    jobs += 1;
                                } else {
                                    let state = owner.finish(origin).unwrap();
                                    remote.restore_remote_sound_delta(&state).unwrap();
                                }
                            }
                        }
                        let done = remote.advance_sound_projection_field().unwrap();
                        if active {
                            let at = remote.remote_sound_projection_time();
                            let prefix = owner.prefix().unwrap();
                            let stable = u64::from(prefix[4]) | u64::from(prefix[5]) << 32;
                            if !done
                                && remote.sound_rendezvous_boundary().is_empty()
                                && stable > frontier
                            {
                                assert_eq!(at, frontier);
                                if independent
                                    && stable - at > u64::from(remote.bus.smpc.clocks_to_event())
                                {
                                    controller_crossings += 1;
                                }
                                remote.extend_remote_sound_projection(stable).unwrap();
                                frontier = stable;
                                extensions += 1;
                                continue;
                            }
                            if done || jobs % 3 == 0 {
                                assert!(remote.reset_sound_transport().is_err());
                                remote.abort_sound_transport().unwrap();
                                assert_eq!(remote.remote_sound_projection_time(), at);
                                owner = crate::sound::delta::DeltaOwner::default();
                                if done {
                                    lost_at_field_end += 1;
                                } else {
                                    lost_during_execution += 1;
                                }
                            } else {
                                let state = owner.finish(at).unwrap();
                                assert!(owner.finish(at).is_err());
                                remote.restore_remote_sound_delta(&state).unwrap();
                                assert!(remote.restore_remote_sound_delta(&state).is_err());
                            }
                            active = false;
                        }
                        if done {
                            break;
                        }
                        remote.acknowledge_sound_rendezvous();
                    }
                    same(&mut reference, &mut remote);
                }
                if source == include_str!("../../tests/fixtures/frt-compare.hex") {
                    assert!(reference.cpu.state().exceptions > 0);
                    assert!(
                        reference
                            .cpu
                            .state()
                            .last_exception
                            .as_ref()
                            .is_some_and(|exception| exception.vector == 32),
                        "original FRT guest must wake from SLEEP through its interrupt handler"
                    );
                }
                idle_batches += remote.bus.sound_rendezvous.projected_idle_batches;
                idle_max = idle_max.max(remote.bus.sound_rendezvous.projected_idle_max);
            }
        }
        assert!(idle_batches > 0, "must exercise projected idle batches");
        assert!(
            idle_max > crate::CLOCKS_PER_STEP,
            "must exercise event-bounded waits beyond the ordinary step budget"
        );
        assert!(jobs > 10 && extensions > 10);
        if independent {
            assert!(
                controller_crossings > 0,
                "must retain ownership across real controller device deadlines"
            );
        }
        assert!(lost_before_accept > 0 && lost_during_execution > 0 && lost_at_field_end > 0);
    }

    #[test]
    fn remote_sound_owner_extends_prefixes_and_rejects_stale_returns() {
        let mut jobs = 0;
        let mut extensions = 0;
        let mut stale_returns = 0;
        for source in [
            include_str!("../../examples/runtime-loop.hex"),
            include_str!("../../examples/runtime-poll-sound-busy-scene.hex"),
            include_str!("../../tests/fixtures/scsp-cpu-memory.hex"),
            include_str!("../../tests/fixtures/scsp-clock.hex"),
            include_str!("../../tests/fixtures/scu-fringe.hex"),
            include_str!("../../tests/fixtures/frt-compare.hex"),
        ] {
            for fast in [false, true] {
                let rom = image(source);
                let (mut reference, mut remote) = (Machine::new(), Machine::new());
                for m in [&mut reference, &mut remote] {
                    m.reset(Boot::ExternalBios(&rom)).unwrap();
                    m.bus.change_system_clock(fast);
                }
                let mut owner: Option<(crate::sound::forecast::Forecast, Vec<u8>, u64, u64)> = None;
                for _ in 0..2 {
                    reference.advance_video_field().unwrap();
                    loop {
                        if owner.is_none() {
                            let range = remote.remote_sound_projection_range(32768);
                            if !range.is_empty() {
                                let origin = u64::from(range[0]) | u64::from(range[1]) << 32;
                                let end = u64::from(range[2]) | u64::from(range[3]) << 32;
                                let bytes = remote.sound_domain_snapshot().unwrap();
                                let sound =
                                    crate::sound::Sound::from_domain_snapshot(&bytes).unwrap();
                                let mut f = crate::sound::forecast::Forecast::new(sound);
                                f.extend(end).unwrap();
                                let through = f.stable_frontier().min(origin + 8192);
                                if through > origin {
                                    assert!(
                                        remote
                                            .accept_remote_sound_projection(
                                                origin + 1,
                                                end,
                                                through,
                                                f.initial_irq()
                                            )
                                            .is_err()
                                    );
                                    assert!(
                                        remote
                                            .accept_remote_sound_projection(
                                                origin,
                                                end,
                                                through,
                                                !f.initial_irq()
                                            )
                                            .is_err()
                                    );
                                    assert_eq!(remote.sound_domain_snapshot().unwrap(), bytes);
                                    remote
                                        .accept_remote_sound_projection(
                                            origin,
                                            end,
                                            through,
                                            f.initial_irq(),
                                        )
                                        .unwrap();
                                    assert!(
                                        remote.extend_remote_sound_projection(end + 1).is_err()
                                    );
                                    owner = Some((f, bytes, origin, through));
                                    jobs += 1;
                                }
                            }
                        }
                        let done = remote.advance_sound_projection_field().unwrap();
                        if let Some((mut f, stale, origin, through)) = owner.take() {
                            let at = remote.remote_sound_projection_time();
                            if !done
                                && remote.sound_rendezvous_boundary().is_empty()
                                && f.stable_frontier() > through
                            {
                                assert_eq!(at, through);
                                let next = f.stable_frontier();
                                remote.extend_remote_sound_projection(next).unwrap();
                                owner = Some((f, stale, origin, next));
                                extensions += 1;
                                continue;
                            }
                            if at != origin {
                                assert!(remote.restore_remote_sound_projection(&stale).is_err());
                                assert_eq!(remote.remote_sound_projection_time(), at);
                                assert!(remote.sound_domain_snapshot().is_err());
                                stale_returns += 1;
                            }
                            let state = f.snapshot_at(at).unwrap();
                            remote.restore_remote_sound_projection(&state).unwrap();
                            assert!(
                                remote.restore_remote_sound_projection(&state).is_err(),
                                "ownership returned exactly once"
                            );
                        }
                        if done {
                            break;
                        }
                        remote.acknowledge_sound_rendezvous();
                    }
                    same(&mut reference, &mut remote);
                }
            }
        }
        assert!(jobs > 10 && extensions > 10 && stale_returns > 10);
    }

    #[test]
    fn new_smpc_sound_command_invalidates_an_existing_forecast() {
        let rom = image(
            "0000 0000 0200 060F F000\n0200 D107 E001 2100 D107 E001 2100 D106 E006 2100 AFFE 0009\n0220 FFFF FE92 0010 0063 0010 001F",
        );
        for fast in [false, true] {
            let (mut reference, mut projected) = (Machine::new(), Machine::new());
            for m in [&mut reference, &mut projected] {
                m.reset(Boot::ExternalBios(&rom)).unwrap();
                m.bus.change_system_clock(fast);
            }
            reference.advance_video_field().unwrap();
            let mut projected_before_command = false;
            loop {
                if projected.begin_sound_projection(32768) && !projected.bus.sound.enabled {
                    projected_before_command = true;
                }
                if projected.advance_sound_projection_field().unwrap() {
                    break;
                }
                projected.finish_sound_projection();
                projected.acknowledge_sound_rendezvous();
            }
            assert!(projected_before_command);
            assert!(
                reference.bus.sound.enabled,
                "original guest completed SNDon"
            );
            same(&mut reference, &mut projected);
        }
    }

    #[test]
    fn projection_frontier_precedes_sound_cpu_irq_changes() {
        let rom = image(include_str!("../../examples/runtime-loop.hex"));
        let (mut reference, mut projected) = (Machine::new(), Machine::new());
        for m in [&mut reference, &mut projected] {
            m.reset(Boot::ExternalBios(&rom)).unwrap();
            m.bus.sound = crate::sound::wire::tests::fixture(false, 0);
            for (i, word) in [
                0x33FCu16, 0x0020, 0x0010, 0x042A, 0x33FC, 0x0020, 0x0010, 0x042C, 0x33FC, 0x0020,
                0x0010, 0x042E, 0x60EE,
            ]
            .into_iter()
            .enumerate()
            {
                m.bus.sound.ram[0x100 + i * 2..0x102 + i * 2].copy_from_slice(&word.to_be_bytes());
            }
        }
        reference.advance_video_field().unwrap();
        let mut shortened = 0;
        loop {
            if projected.begin_sound_projection(512) {
                let f = projected.bus.sound_rendezvous.forecast.as_ref().unwrap();
                if let Some(&(irq_at, _)) = f.irq_timeline().get(1) {
                    assert_eq!(
                        projected.bus.sound.cached_cpu_deadline() as u64 + f.origin(),
                        irq_at - 1
                    );
                    shortened += 1;
                }
            }
            if projected.advance_sound_projection_field().unwrap() {
                break;
            }
            projected.finish_sound_projection();
            projected.acknowledge_sound_rendezvous();
        }
        assert!(shortened > 10, "exercise both raised and cleared main IRQs");
        same(&mut reference, &mut projected);
    }

    #[test]
    fn projected_main_board_stays_within_computed_sound_prefixes() {
        let mut projections = 0;
        let mut shared_inputs = 0;
        let mut frontiers = 0;
        for source in [
            include_str!("../../examples/runtime-loop.hex"),
            include_str!("../../examples/runtime-poll-sound-busy-scene.hex"),
            include_str!("../../tests/fixtures/scsp-cpu-memory.hex"),
            include_str!("../../tests/fixtures/scsp-clock.hex"),
            include_str!("../../tests/fixtures/scu-fringe.hex"),
            include_str!("../../tests/fixtures/frt-compare.hex"),
        ] {
            for fast in [false, true] {
                let rom = image(source);
                let (mut reference, mut projected) = (Machine::new(), Machine::new());
                for m in [&mut reference, &mut projected] {
                    m.reset(Boot::ExternalBios(&rom)).unwrap();
                    m.bus.change_system_clock(fast);
                }
                for _ in 0..4 {
                    reference.advance_video_field().unwrap();
                    let mut yields = 0;
                    loop {
                        if projected.begin_sound_projection(32768) {
                            projections += 1;
                            assert!(projected.sound_domain_snapshot().is_err());
                        }
                        let done = projected.advance_sound_projection_field().unwrap();
                        if done {
                            break;
                        }
                        let frontier = projected.bus.sound.projection_at_frontier();
                        projected.finish_sound_projection();
                        if projected.acknowledge_sound_rendezvous() {
                            shared_inputs += 1;
                        } else {
                            // An unprojected prefix may also yield for a bounded offload retry.
                            if frontier {
                                frontiers += 1;
                            }
                        }
                        yields += 1;
                        assert!(yields < 100000);
                    }
                    same(&mut reference, &mut projected);
                }
            }
        }
        assert!(projections > 10);
        assert!(shared_inputs > 10);
        assert!(frontiers > 10);
    }

    #[test]
    fn original_sound_cpu_arbitration_and_clock_guests_resume_at_shared_boundaries() {
        for source in [
            include_str!("../../tests/fixtures/scsp-cpu-memory.hex"),
            include_str!("../../tests/fixtures/scsp-clock.hex"),
            include_str!("../../tests/fixtures/scu-fringe.hex"),
        ] {
            let rom = image(source);
            let (mut reference, mut paused) = (Machine::new(), Machine::new());
            for m in [&mut reference, &mut paused] {
                m.reset(Boot::ExternalBios(&rom)).unwrap();
            }
            let mut boundaries = 0;
            for _ in 0..4 {
                reference.advance_video_field().unwrap();
                while !paused.advance_sound_rendezvous_field().unwrap() {
                    assert!(paused.acknowledge_sound_rendezvous());
                    boundaries += 1;
                    assert!(boundaries < 100000);
                }
                same(&mut reference, &mut paused);
            }
            assert!(boundaries > 0);
        }
    }
}
