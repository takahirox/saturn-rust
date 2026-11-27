//! Original renderer-domain experiment. Commands own their input bytes; a
//! consumer never consults the producer's current RAM or framebuffer. This
//! domain can drain serially or emit bounded transferable chunks. Production
//! Machine execution keeps the existing renderer unless explicitly enabled.
use super::BusError;
use crate::devices::VDP2;
use crate::timing::{Raster, RasterEvent};
use crate::vdp1::{DisplaySurface, Vdp1};
use crate::vdp2::Vdp2;
use std::collections::VecDeque;
mod wire;

#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct FrontPatch {
    pub offset: usize,
    pub bytes: Vec<u8>,
}

/// VDP1 advances before the endpoint's raster event. HBlank display erase
/// follows scanout; a VBlankOut swap precedes new-field acquisition.
pub enum Endpoint {
    None,
    HBlankErase(Vec<FrontPatch>),
    /// The first HBlank-in after VBlank-OUT latched a framebuffer change:
    /// install the new front before composing the line, then erase it.
    FrameChange(DisplaySurface, Vec<FrontPatch>),
}

pub enum Command {
    Flush,
    Advance {
        clocks: u32,
        before: Vec<FrontPatch>,
        endpoint: Endpoint,
    },
    Vram {
        offset: usize,
        bytes: Vec<u8>,
    },
    Cram {
        offset: usize,
        bytes: Vec<u8>,
    },
    Register {
        address: u32,
        value: u16,
    },
    FrontMode(u16),
    FrontSurface(DisplaySurface),
    Pal(bool),
    Clock(bool),
    Reset {
        pal: bool,
    },
    /// Relaxed playback: render whole lines at scanout without slot timing.
    LineRendering(bool),
}

pub struct Renderer {
    raster: Raster,
    line_rendering: bool,
    video: Vdp2,
    sprites: Vdp1,
}
impl Default for Renderer {
    fn default() -> Self {
        Self {
            raster: Raster::default(),
            line_rendering: false,
            video: Vdp2::clocked(),
            sprites: Vdp1::default(),
        }
    }
}
impl Renderer {
    pub fn replay_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        wire::replay(self, bytes)
    }
    pub fn frames(&self) -> u64 {
        self.raster.fields
    }
    pub fn dimensions(&self) -> (usize, usize) {
        self.video.output_dimensions()
    }
    pub fn framebuffer(&self) -> Result<Vec<u8>, String> {
        self.video.scanout(&self.sprites)
    }

    fn acquire(&mut self) {
        if !self.video.display_active() {
            return;
        }
        if let Some((field, line, phase, slot)) = self.raster.display_slot() {
            self.video.acquire_display(field, line, phase, slot);
        }
        if self.video.rotation_display_active()
            && let Some((field, line, phase, x)) = self.raster.rotation_display_slot()
        {
            self.video
                .acquire_rotation_display(&self.sprites, field, line, phase, x);
        }
    }
    fn flush(&mut self) {
        if self.raster.line < self.raster.display_lines() {
            let end = self.raster.visible_pixels(self.video.dimensions().0);
            self.video
                .scan_to(&self.sprites, usize::from(self.raster.line), end);
        }
    }
    fn patch(&mut self, patches: Vec<FrontPatch>) {
        for patch in patches {
            self.sprites
                .patch_display_surface(patch.offset, &patch.bytes);
        }
    }
    fn advance(&mut self, mut clocks: u32, before: Vec<FrontPatch>, endpoint: Endpoint) {
        assert!(clocks > 0, "an endpoint needs a positive logical span");
        let slots = !self.line_rendering;
        if slots {
            self.acquire();
        }
        let mut before = Some(before);
        let mut endpoint = Some(endpoint);
        while clocks != 0 {
            let display = if slots && self.video.display_active() {
                self.raster
                    .clocks_to_display_slot()
                    .min(if self.video.rotation_display_active() {
                        self.raster.clocks_to_rotation_slot()
                    } else {
                        u32::MAX
                    })
            } else {
                u32::MAX
            };
            let delta = clocks.min(display).min(self.raster.clocks_to_event());
            let final_step = clocks == delta;
            if final_step {
                self.patch(before.take().unwrap());
            }
            let event = self.raster.advance(delta);
            match event {
                Some(RasterEvent::HBlankIn) => {
                    let erase = if final_step {
                        match endpoint.take().unwrap() {
                            Endpoint::None => Vec::new(),
                            Endpoint::HBlankErase(patches) => patches,
                            Endpoint::FrameChange(surface, patches) => {
                                self.sprites.install_display_surface(surface);
                                patches
                            }
                        }
                    } else {
                        Vec::new()
                    };
                    if self.raster.line < self.raster.display_lines() {
                        self.video
                            .scanline(&self.sprites, usize::from(self.raster.line));
                    }
                    self.video.hblank = true;
                    self.patch(erase);
                }
                Some(RasterEvent::VBlankIn) => {
                    self.video.finish_field();
                    self.video.hblank = false;
                    self.video.vblank = true;
                }
                Some(RasterEvent::VBlankOut) => {
                    self.video.hblank = self.raster.line != 0;
                    self.video.vblank = false;
                    self.video.scan_field(self.raster.odd());
                    if self.raster.line == 0 {
                        self.video.begin_scanline(0);
                    }
                }
                Some(RasterEvent::LineStart) => {
                    self.video.hblank = false;
                    if self.raster.line < self.raster.display_lines() {
                        self.video.begin_scanline(usize::from(self.raster.line));
                    }
                }
                None => (),
            }
            if final_step {
                assert!(
                    matches!(endpoint.take(), None | Some(Endpoint::None)),
                    "a front action must match its original raster edge"
                );
            }
            let (h, v) = self.raster.counters();
            self.video.counters(h, v);
            self.video.scan_field(self.raster.odd());
            if slots {
                self.acquire();
            }
            clocks -= delta;
        }
    }
    pub fn apply(&mut self, command: Command) -> Result<(), BusError> {
        match command {
            Command::Flush => self.flush(),
            Command::Advance {
                clocks,
                before,
                endpoint,
            } => self.advance(clocks, before, endpoint),
            Command::Vram { offset, bytes } => {
                self.flush();
                self.video.vram[offset..offset + bytes.len()].copy_from_slice(&bytes);
            }
            Command::Cram { offset, bytes } => {
                self.flush();
                self.video.cram[offset..offset + bytes.len()].copy_from_slice(&bytes);
            }
            Command::Register { address, value } => {
                self.flush();
                self.video.write16(address, value)?;
                if address == VDP2 {
                    self.raster.configure(value);
                }
            }
            Command::FrontMode(mode) => {
                self.flush();
                self.sprites.write16(crate::vdp1::REGISTERS, mode)?;
            }
            Command::FrontSurface(surface) => {
                self.flush();
                self.sprites.install_display_surface(surface);
            }
            Command::Pal(pal) => {
                self.raster.pal = pal;
                self.video.pal = pal;
            }
            Command::Clock(fast) => {
                self.raster.select_clock(fast);
                self.raster.configure(0);
                self.video = Vdp2::clocked();
                self.video.set_clocked(!self.line_rendering);
                self.video.pal = self.raster.pal;
                self.sprites = Vdp1::default();
            }
            Command::LineRendering(enabled) => {
                self.flush();
                self.line_rendering = enabled;
                self.video.set_clocked(!enabled);
            }
            Command::Reset { pal } => {
                *self = Self::default();
                self.raster.pal = pal;
                self.video.pal = pal;
            }
        }
        Ok(())
    }
}

/// Owned FIFO with adjacent observer-free clock spans combined. No command
/// can overtake a memory write, surface change, endpoint action or reset.
#[derive(Default)]
pub struct Journal {
    commands: VecDeque<Command>,
}

pub(super) struct Domain {
    journal: Journal,
    renderer: Renderer,
    external: bool,
    pending_clocks: u32,
    #[cfg(not(target_arch = "wasm32"))]
    outgoing: VecDeque<Vec<u8>>,
}
impl Domain {
    pub(super) fn new(video: &Vdp2, raster: &Raster, sprites: &Vdp1) -> Option<Self> {
        let mut renderer = Renderer {
            raster: raster.clone(),
            line_rendering: false,
            video: video.renderer_seed()?,
            sprites: Vdp1::default(),
        };
        renderer
            .sprites
            .install_display_surface(sprites.display_surface());
        Some(Self {
            journal: Journal::default(),
            renderer,
            external: false,
            pending_clocks: 0,
            #[cfg(not(target_arch = "wasm32"))]
            outgoing: VecDeque::new(),
        })
    }
    pub(super) fn push(&mut self, command: Command) {
        if self.external
            && let Command::Advance { clocks, .. } = &command
        {
            assert!(*clocks <= wire::MAX_CLOCKS, "bounded raster segment");
            if self.pending_clocks > wire::MAX_CLOCKS - clocks {
                self.drain();
            }
            self.pending_clocks += clocks;
        }
        self.journal.push(command);
        if self.journal.len() >= 4096 {
            self.drain();
        }
    }
    pub(super) fn drain(&mut self) {
        if self.external {
            if self.journal.is_empty() {
                return;
            }
            let bytes = wire::encode(&mut self.journal);
            self.pending_clocks = 0;
            #[cfg(target_arch = "wasm32")]
            crate::wasm::emit_video_chunk(&bytes);
            #[cfg(not(target_arch = "wasm32"))]
            self.outgoing.push_back(bytes);
        } else {
            self.journal
                .drain_into(&mut self.renderer)
                .expect("producer validated video command");
        }
    }
}

impl super::Bus {
    pub(crate) fn enable_video_transport(&mut self, enabled: bool) -> bool {
        if !enabled {
            return self.enable_video_journal(false);
        }
        if self.main_clocks != 0 || self.raster.fields != 0 {
            return false;
        }
        let Some(mut domain) = Domain::new(&self.vdp2, &self.raster, &self.vdp1) else {
            return false;
        };
        domain.external = true;
        domain.journal.push(Command::Reset {
            pal: self.raster.pal,
        });
        domain
            .journal
            .push(Command::Clock(self.raster.renderer_clock_fast()));
        for command in self.vdp2.renderer_setup_commands() {
            domain.journal.push(command);
        }
        domain
            .journal
            .push(Command::FrontSurface(self.vdp1.display_surface()));
        if self.fast_video {
            domain.journal.push(Command::LineRendering(true));
        }
        self.video_domain = Some(Box::new(std::cell::RefCell::new(domain)));
        self.vdp1.enable_display_journal(true);
        self.display_deadline = 0;
        true
    }
    pub(crate) fn video_transport_enabled(&self) -> bool {
        self.video_domain
            .as_ref()
            .is_some_and(|d| d.borrow().external)
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn take_video_chunks(&mut self) -> Vec<Vec<u8>> {
        self.video_domain.as_mut().map_or_else(Vec::new, |d| {
            let domain = d.get_mut();
            domain.drain();
            domain.outgoing.drain(..).collect()
        })
    }
    pub(crate) fn enable_video_journal(&mut self, enabled: bool) -> bool {
        if self.main_clocks != 0 || self.raster.fields != 0 {
            return false;
        }
        if enabled && self.video_domain.is_none() {
            let Some(domain) = Domain::new(&self.vdp2, &self.raster, &self.vdp1) else {
                return false;
            };
            self.video_domain = Some(Box::new(std::cell::RefCell::new(domain)));
        } else if !enabled {
            self.video_domain = None;
        }
        self.vdp1.enable_display_journal(enabled);
        self.display_deadline = 0;
        true
    }
    pub(crate) fn video_journal_enabled(&self) -> bool {
        self.video_domain.is_some()
    }
    pub(super) fn record_video_command(&mut self, command: Command) {
        if let Some(domain) = &mut self.video_domain {
            domain.get_mut().push(command);
        }
    }
    pub(super) fn record_video_advance(
        &mut self,
        clocks: u32,
        before: Vec<FrontPatch>,
        event: Option<RasterEvent>,
    ) {
        if self.video_domain.is_none() {
            return;
        }
        let after = self.vdp1.take_display_journal();
        let endpoint = match event {
            Some(RasterEvent::HBlankIn) => match self.vdp1.take_changed_front() {
                Some(surface) => Endpoint::FrameChange(surface, after),
                None => Endpoint::HBlankErase(after),
            },
            _ => {
                assert!(
                    after.is_empty(),
                    "unexpected front write after raster event"
                );
                Endpoint::None
            }
        };
        self.record_video_command(Command::Advance {
            clocks,
            before,
            endpoint,
        });
    }
    pub(crate) fn drain_video_journal(&self) {
        if let Some(domain) = &self.video_domain {
            domain.borrow_mut().drain();
        }
    }
    pub(crate) fn queued_framebuffer(&self) -> Option<Result<Vec<u8>, String>> {
        self.video_domain.as_ref().map(|domain| {
            let mut domain = domain.borrow_mut();
            domain.drain();
            if domain.external {
                return Err("external renderer owns video output".into());
            }
            domain.renderer.framebuffer()
        })
    }
    pub(crate) fn queued_dimensions(&self) -> Option<(usize, usize)> {
        self.video_domain.as_ref().map(|domain| {
            let mut domain = domain.borrow_mut();
            domain.drain();
            if domain.external {
                return (0, 0);
            }
            domain.renderer.dimensions()
        })
    }
}

impl crate::Machine {
    pub fn enable_video_transport(&mut self, enabled: bool) -> bool {
        self.bus.enable_video_transport(enabled)
    }
    pub fn video_transport_enabled(&self) -> bool {
        self.bus.video_transport_enabled()
    }
    /// Publish the exact current video prefix for paused debugger queries.
    pub fn flush_video_output(&self) {
        self.bus.drain_video_journal();
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn take_video_chunks(&mut self) -> Vec<Vec<u8>> {
        self.bus.take_video_chunks()
    }
    /// Experimental renderer pipeline boundary: return at the raster field
    /// event even while a CPU is sleeping or still waiting on its next issue.
    /// Unlike a debugger step, this does not wait for another CPU boundary.
    pub fn advance_video_field(&mut self) -> Result<(), crate::MachineError> {
        self.advance_frame_mode::<true>()
    }
    pub fn enable_video_journal(&mut self, enabled: bool) -> bool {
        self.bus.enable_video_journal(enabled)
    }
    pub fn video_journal_enabled(&self) -> bool {
        self.bus.video_journal_enabled()
    }
}
impl Journal {
    pub fn len(&self) -> usize {
        self.commands.len()
    }
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
    pub fn push(&mut self, command: Command) {
        if let Command::Advance {
            clocks,
            before,
            endpoint: Endpoint::None,
        } = &command
            && before.is_empty()
            && let Some(Command::Advance {
                clocks: old,
                before: old_before,
                endpoint: Endpoint::None,
            }) = self.commands.back_mut()
            && old_before.is_empty()
            && let Some(combined) = old.checked_add(*clocks)
        {
            *old = combined;
            return;
        }
        self.commands.push_back(command);
    }
    pub fn drain_into(&mut self, renderer: &mut Renderer) -> Result<(), BusError> {
        while let Some(command) = self.commands.pop_front() {
            renderer.apply(command)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
