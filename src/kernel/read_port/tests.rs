use super::*;

fn word(address: u32) -> u32 {
    0x8123_4567 ^ address.rotate_left(9)
}
fn value(address: u32, width: usize) -> u32 {
    (0..width).fold(0, |v, i| {
        let at = address + i as u32;
        let byte = word(at & !3) >> ((3 - (at & 3)) * 8) & 255;
        v << 8 | byte
    })
}
fn finish(port: &mut ReadPort, mut poll: ReadPoll) -> (u32, Vec<ReadTransfer>) {
    let mut transfers = Vec::new();
    while poll == ReadPoll::Pending {
        let transfer = port.take_transfer().unwrap();
        assert!(
            port.take_transfer().is_none(),
            "each request is handed off once"
        );
        poll = port
            .complete(transfer.ticket, value(transfer.address, transfer.width))
            .unwrap();
        transfers.push(transfer);
    }
    let ReadPoll::Ready(value) = poll else {
        panic!("live read was discarded")
    };
    (value, transfers)
}

#[test]
fn all_widths_offsets_owners_and_allocation_controls_match_the_actual_cache() {
    for owner in [Processor::Master, Processor::Slave] {
        for control in [1, 9, 3, 5] {
            for fetch in [false, true] {
                for width in if fetch { vec![2] } else { vec![1, 2, 4] } {
                    for offset in (0..16).step_by(width) {
                        let address = HIGH_WRAM + 0x1000 + offset as u32;
                        let request = if fetch {
                            Read::Fetch16(address)
                        } else {
                            Read::Data {
                                address,
                                width: width as u32,
                            }
                        };
                        let mut port = ReadPort::new(owner);
                        port.set_control(control).unwrap();
                        let poll = port.request(request).unwrap();
                        let (actual, transfers) = finish(&mut port, poll);
                        assert_eq!(actual, value(address, width));
                        let mut reference = Cache::default();
                        reference.set_control(control);
                        let (at, bus_width, instruction) = request.parts().unwrap();
                        let mut expected_addresses = Vec::new();
                        let mut read = |a| {
                            expected_addresses.push(a);
                            Ok(word(a).to_be_bytes())
                        };
                        match bus_width {
                            1 => {
                                reference.read::<1>(at, instruction, &mut read).unwrap();
                            }
                            2 => {
                                reference.read::<2>(at, instruction, &mut read).unwrap();
                            }
                            _ => {
                                reference.read::<4>(at, instruction, &mut read).unwrap();
                            }
                        }
                        port.cache.assert_same_contents(&reference);
                        if expected_addresses.is_empty() {
                            assert_eq!(transfers.len(), 1);
                            assert_eq!(transfers[0].address, at);
                            assert_eq!(transfers[0].width, bus_width);
                        } else {
                            assert_eq!(
                                transfers.iter().map(|t| t.address).collect::<Vec<_>>(),
                                expected_addresses
                            );
                            assert!(transfers.iter().all(|t| t.width == 4));
                            assert_eq!(
                                transfers
                                    .iter()
                                    .map(|t| (t.first, t.last))
                                    .collect::<Vec<_>>(),
                                [(true, false), (false, false), (false, false), (false, true)]
                            );
                        }
                        assert!(transfers.iter().all(|t| t.instruction == fetch));
                    }
                }
            }
        }
    }
}

#[test]
fn refill_words_publish_only_after_the_last_real_completion() {
    let base = HIGH_WRAM + 0x1000;
    let mut port = ReadPort::new(Processor::Master);
    port.set_control(1).unwrap();
    assert_eq!(
        port.request(Read::Data {
            address: base + 8,
            width: 4
        }),
        Ok(ReadPoll::Pending)
    );
    for (index, address) in [base + 12, base, base + 4, base + 8]
        .into_iter()
        .enumerate()
    {
        assert!(
            !port.cache.read_hit(base),
            "a partial line must remain invisible"
        );
        let transfer = port.take_transfer().unwrap();
        assert_eq!(transfer.address, address);
        let poll = port.complete(transfer.ticket, word(address)).unwrap();
        assert_eq!(
            poll,
            if index == 3 {
                ReadPoll::Ready(word(base + 8))
            } else {
                ReadPoll::Pending
            }
        );
    }
    assert_eq!(
        port.request(Read::Data {
            address: base,
            width: 4
        }),
        Ok(ReadPoll::Ready(word(base)))
    );
    assert!(port.take_transfer().is_none());
}

#[test]
fn fetch_halfword_cache_through_and_purge_keep_distinct_sources() {
    let base = LOW_WRAM + 0x1000;
    let mut port = ReadPort::new(Processor::Slave);
    port.set_control(1).unwrap();
    let poll = port.request(Read::Fetch16(base)).unwrap();
    let (_, transfers) = finish(&mut port, poll);
    assert_eq!(transfers.len(), 4);
    let replacement = port.cache.replacement_state();
    assert_eq!(
        port.request(Read::Fetch16(base + 2)),
        Ok(ReadPoll::Ready(word(base) & 65535))
    );
    assert_eq!(port.cache.replacement_state(), replacement);
    assert!(port.take_transfer().is_none());
    // Bypass reads observe newly completed RAM, while the private cached copy
    // remains unchanged. Retained halves are also independent of later RAM.
    assert_eq!(
        port.request(Read::Data {
            address: base | 0x2000_0000,
            width: 4
        }),
        Ok(ReadPoll::Pending)
    );
    let transfer = port.take_transfer().unwrap();
    assert_eq!(
        (
            transfer.address,
            transfer.width,
            transfer.first,
            transfer.last
        ),
        (base, 4, true, true)
    );
    assert_eq!(
        port.complete(transfer.ticket, 0xBEEF1234),
        Ok(ReadPoll::Ready(0xBEEF1234))
    );
    assert_eq!(
        port.request(Read::Data {
            address: base,
            width: 4
        }),
        Ok(ReadPoll::Ready(word(base)))
    );
    port.discard_prefetch();
    port.set_control(0x11).unwrap();
    assert_eq!(port.request(Read::Fetch16(base)), Ok(ReadPoll::Pending));
    assert!(!port.cache.read_hit(base));
}

#[test]
fn reset_and_duplicate_replies_cannot_install_a_line_or_complete_a_new_read() {
    let base = HIGH_WRAM + 0x1000;
    let mut port = ReadPort::new(Processor::Master);
    port.set_control(1).unwrap();
    port.request(Read::Fetch16(base)).unwrap();
    let old = port.take_transfer().unwrap();
    assert_eq!(port.request(Read::Fetch16(base + 2)), Err(Error::Protocol));
    assert_eq!(port.set_control(0), Err(Error::Protocol));
    port.reset();
    assert_eq!(
        port.complete(old.ticket, 0x12345678),
        Ok(ReadPoll::Discarded)
    );
    port.set_control(1).unwrap();
    let poll = port.request(Read::Fetch16(base)).unwrap();
    assert_eq!(
        port.complete(old.ticket, 0x87654321),
        Ok(ReadPoll::Discarded)
    );
    let (actual, transfers) = finish(&mut port, poll);
    assert_eq!(actual, word(base) >> 16);
    assert_eq!(transfers.len(), 4);
    assert!(transfers.iter().all(|t| t.ticket != old.ticket));
    assert_eq!(
        port.complete(transfers[3].ticket, 0),
        Ok(ReadPoll::Discarded)
    );
    let mut peer = ReadPort::new(Processor::Slave);
    assert_eq!(peer.complete(transfers[3].ticket, 0), Err(Error::Protocol));
}

#[test]
fn unsupported_devices_and_alignment_fail_before_a_request_or_cache_change() {
    let mut port = ReadPort::new(Processor::Master);
    port.set_control(1).unwrap();
    for (address, width) in [
        (HIGH_WRAM + 1, 2),
        (HIGH_WRAM + 2, 4),
        (HIGH_WRAM, 0),
        (0x05F80000, 2),
        (0x60000000, 4),
    ] {
        assert_eq!(
            port.request(Read::Data { address, width }),
            Err(Error::Address { address, width })
        );
        assert!(port.take_transfer().is_none());
        assert!(port.pending.is_none());
    }
}

#[test]
fn changed_transfer_payload_is_rejected_before_the_board_observes_it() {
    let mut bus = Bus::default();
    let mut port = ReadPort::new(Processor::Master);
    port.set_control(1).unwrap();
    port.request(Read::Fetch16(HIGH_WRAM)).unwrap();
    let original = port.take_transfer().unwrap();
    for field in 0..5 {
        let mut changed = original;
        match field {
            0 => changed.address = crate::devices::SMPC,
            1 => changed.width = 1,
            2 => changed.instruction = !changed.instruction,
            3 => changed.first = !changed.first,
            _ => changed.last = !changed.last,
        }
        assert_eq!(
            port.resume(&mut bus, changed),
            Err(ReadError::State(Error::Protocol))
        );
        assert_eq!(bus.main_clocks, 0);
        assert_eq!(bus.bus_transaction_deadline(), u32::MAX);
    }
    assert_eq!(port.resume(&mut bus, original), Ok(ReadPoll::Pending));
}

#[test]
fn production_arbiter_bsc_width_endian_and_chunking_match_without_cpu_replay() {
    let digest = super::verify_board();
    assert_eq!(digest[0], 904);
}
