//! Device constants and compatibility exports; implementations have separate modules.

pub const VRAM: u32 = 0x05E0_0000;
pub const VDP2: u32 = 0x05F8_0000;
pub const SMPC: u32 = 0x0010_0000;
pub const WIDTH: usize = 320;
pub const HEIGHT: usize = 224;

pub use crate::smpc::Smpc;

pub use crate::vdp2::Vdp2;

#[cfg(test)]
mod smpc_command_tests {
    use super::*;

    #[test]
    fn cpu_control_commands_require_sf_latch_output_and_complete_after_807_clocks() {
        for command in [0, 2, 3, 0x18] {
            let mut smpc = Smpc::default();
            assert_eq!(smpc.read(SMPC + 0x63).unwrap(), 0);
            assert!(smpc.write(SMPC + 0x1F, command).is_err());
            smpc.write(SMPC + 0x63, 1).unwrap();
            smpc.write(SMPC + 0x1F, command).unwrap();
            assert_eq!(smpc.read(SMPC + 0x5F).unwrap(), command);
            assert_eq!(smpc.read(SMPC + 0x63).unwrap(), 1);
            assert!(smpc.write(SMPC + 0x1F, 2).is_err()); // No overwrite while busy.
            assert!(smpc.write(SMPC + 0x63, 0).is_err()); // Guest cannot clear SF.
            assert_eq!(smpc.advance_clocks(806), None);
            assert_eq!(smpc.read(SMPC + 0x63).unwrap(), 1);
            assert_eq!(smpc.advance_clocks(1), Some(command));
            assert_eq!(smpc.read(SMPC + 0x63).unwrap(), 0);
            assert_eq!(smpc.advance_clocks(1000), None); // No duplicate completion.
        }
    }

    #[test]
    fn cd_power_commands_complete_after_forty_microseconds_on_emulated_clocks() {
        for command in [8, 9] {
            let mut s = Smpc::default();
            s.write(SMPC + 0x63, 1).unwrap();
            s.write(SMPC + 0x1F, command).unwrap();
            assert_eq!(s.advance_clocks(1074), None);
            assert_eq!(s.read(SMPC + 0x63), Ok(1));
            assert_eq!(s.advance_clocks(1), Some(command));
            assert_eq!(s.read(SMPC + 0x63), Ok(0));
        }
    }

    #[test]
    fn unsupported_commands_and_register_width_contract_are_explicit() {
        let mut smpc = Smpc::default();
        smpc.write(SMPC + 0x63, 1).unwrap();
        assert!(smpc.write(SMPC + 0x1F, 0xFF).is_err());
        assert_eq!(smpc.read(SMPC + 0x5F).unwrap(), 0);
        assert_eq!(smpc.read(SMPC + 0x63).unwrap(), 1);
        assert_eq!(smpc.advance_clocks(1000), None);
        assert!(smpc.read(SMPC + 0x1F).is_err()); // Write-only COMREG.
        assert!(smpc.write(SMPC + 0x5F, 0).is_err()); // Read-only OREG31.
    }
}
