//! Independent HLE startup data and original guest-executed system services.
//! ST-162-R1 System Program §§1–2.2; no CPU instruction interception.
use crate::bus::{BIOS_SIZE, Bus, BusError, Memory};

pub(crate) const POWER_CLEAR: u32 = 0x0600_0C00;

pub(crate) fn install(bus: &mut Bus, aip_entry: u32, slave_stack: u32) -> Result<(), BusError> {
    let mut rom = vec![0; BIOS_SIZE];
    for (offset, value) in [
        (0, 0x100_u32),
        (4, 0x060F_E000),
        (8, 0x100),
        (12, 0x060F_E000),
    ] {
        rom[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }
    let mut reset = include_bytes!("hle/boot.bin").to_vec();
    // Configure the original reset program's literal pool from ST-040 IP
    // metadata. It still executes ordinary guest loads/jumps on both CPUs.
    for (before, after) in [(0x0600_2E20u32, aip_entry), (0x060F_E000, slave_stack)] {
        let offset = reset
            .windows(4)
            .position(|b| b == before.to_be_bytes())
            .unwrap();
        reset[offset..offset + 4].copy_from_slice(&after.to_be_bytes());
    }
    rom[0x100..0x100 + reset.len()].copy_from_slice(&reset);
    let system = include_bytes!("hle/system.bin");
    rom[0x4000..0x4000 + system.len()].copy_from_slice(system);
    let backup = include_bytes!("hle/backup.bin");
    rom[0x10000..0x10000 + backup.len()].copy_from_slice(backup);
    let player = include_bytes!("hle/player.bin");
    rom[0x18000..0x18000 + player.len()].copy_from_slice(player);
    bus.load_bios(&rom)?;
    for vector in 0..128 {
        let scu = (0x40..0x60).contains(&vector) && !matches!(vector, 0x4E | 0x4F);
        let handler = if scu {
            0x8000 + (vector - 0x40) * 32
        } else {
            0x4B00
        };
        bus.write32(0x0600_0000 + vector * 4, handler)?;
        bus.write32(0x0600_0400 + vector * 4, 0x4B00)?;
    }
    // Public system entry pointers; the referenced code executes on the caller's
    // processor, so VBR and architectural TAS are naturally CPU-correct.
    for (pointer, target) in [
        (0x26C, 0x18000), // SYS_EXECDMP, non-returning original CD player
        (0x280, 0x4800),  // SYS_CHGUIPR
        (0x300, 0x4200),  // SYS_SETUINT
        (0x304, 0x4300),  // SYS_GETUINT
        (0x310, 0x4000),  // SYS_SETSINT
        (0x314, 0x4100),  // SYS_GETSINT
        (0x320, 0x4D00),  // SYS_CHGSYSCK
        (0x330, 0x4600),  // SYS_TASSEM
        (0x334, 0x4700),  // SYS_CLRSEM
        (0x340, 0x4400),  // SYS_SETSCUIM
        (0x344, 0x4500),  // SYS_CHGSCUIM
        (0x358, 0x5000),  // BUP_Init
        (0x210, POWER_CLEAR),
    ] {
        bus.write32(0x0600_0000 + pointer, target)?;
    }
    bus.write32(0x0600_0348, 0xBFFF)?; // SYS_GETSCUIM is data, not a call.
    bus.write32(0x0600_0324, 0)?; // SYS_GETSYSCK is data, not a call.
    for (cause, &level) in crate::scu::INTERRUPT_LEVELS.iter().enumerate() {
        let mut mask = 0xBFFF_u16;
        for (other, &priority) in crate::scu::INTERRUPT_LEVELS.iter().enumerate() {
            if priority > level {
                mask &= !(1 << if other < 16 { other } else { 15 });
            }
        }
        bus.write32(0x0600_0B00 + cause as u32 * 4, 0x4B10)?;
        bus.write32(
            0x0600_0B80 + cause as u32 * 4,
            u32::from(level) << 20 | u32::from(mask),
        )?;
    }
    Ok(())
}
