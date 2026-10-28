//! Original MIT negative oracle; run in the archived inherited tree as documented.
use saturn_rust::{bus::Memory, media::Media, Machine};
#[test]
fn inherited_main_must_retrieve_nonempty_packs() {
    let media = Media::cue(
        include_str!("../../../tests/fixtures/cd-rw.cue"),
        &[
            (
                "cd-rw.iso",
                include_bytes!("../../../tests/fixtures/cd-rw.iso"),
            ),
            (
                "cd-rw-audio.bin",
                include_bytes!("../../../tests/fixtures/cd-rw-audio.bin"),
            ),
            (
                "cd-rw.srw",
                include_bytes!("../../../tests/fixtures/cd-rw.srw"),
            ),
        ],
    )
    .unwrap();
    let mut m = Machine::new();
    m.reset_media(media).unwrap();
    for _ in 0..400 {
        m.run_steps(10000).unwrap();
        if m.bus().read32(0x060103fc) == Ok(0x600d) {
            break;
        }
    }
    assert_eq!(m.bus().read32(0x060103fc), Ok(0x600d));
    assert_eq!(
        m.bus().read32(0x06010308),
        Ok(4),
        "SH-2 must retrieve four real R-W packs"
    );
}
