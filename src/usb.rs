use bincode::config::Options;
use rusb::{
    open_device_with_vid_pid, Context, Device, DeviceDescriptor, DeviceHandle,
    Direction as UsbDirection, GlobalContext, Result, TransferType, UsbContext,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum Direction {
    IN,
    OUT,
    NONE,
}

#[repr(C)]
#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct CommandBlockWrapper {
    signature: [u8; 4],
    tag: u32,
    data_transfer_length: u32,
    flags: u8,
    logical_unit_number: u8,
    command_length: u8,
    command_data: [u8; 16],
}

#[repr(C)]
struct CommandStatusWrapper {
    signature: [u8; 4],
    tag: u32,
    data_residue: u32,
    status: u8,
}

static TAG: AtomicU32 = AtomicU32::new(1);

pub fn open_it8951() -> Option<DeviceHandle<GlobalContext>> {
    open_device_with_vid_pid(0x48d, 0x8951)
}

pub fn get_mass_storage_command_data(
    command_data: [u8; 16],
    data_transfer_length: u32,
    direction: Direction,
) -> Vec<u8> {
    let flags: u8 = match direction {
        Direction::IN => 0x80,
        Direction::OUT => 0x00,
        Direction::NONE => 0x00,
    };
    let tag = TAG.fetch_add(1, Ordering::SeqCst);
    let cwb = CommandBlockWrapper {
        signature: [0x55, 0x53, 0x42, 0x43],
        tag: tag,
        data_transfer_length: data_transfer_length,
        flags: flags,
        logical_unit_number: 0,
        command_length: 16,
        command_data: command_data,
    };
    bincode::options()
        .with_little_endian()
        .with_fixint_encoding()
        .serialize(&cwb)
        .unwrap()
}

pub fn send_mass_storage_command(
    device_handle: &DeviceHandle<GlobalContext>,
    endpoint: u8,
    command_data: [u8; 16],
    data_transfer_length: u32,
    direction: Direction,
) -> Result<usize> {
    let data = &get_mass_storage_command_data(command_data, data_transfer_length, direction);
    // println!(
    //     "data: {:?} len: {}, endpoint: {}",
    //     data,
    //     data.len(),
    //     endpoint
    // );
    return device_handle.write_bulk(endpoint, data, Duration::from_millis(1000));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mass_storage_command_data() {
        let data = get_mass_storage_command_data(
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
            18,
            Direction::OUT,
        );
        assert_eq!(
            data,
            [
                85, 83, 66, 67, 0, 0, 0, 0, 18, 0, 0, 0, 0, 0, 16, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9,
                10, 11, 12, 13, 14, 15
            ]
        );
        // the second time the tag increases
        let data2 = get_mass_storage_command_data(
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
            18,
            Direction::OUT,
        );
        assert_eq!(
            data2,
            [
                85, 83, 66, 67, 1, 0, 0, 0, 18, 0, 0, 0, 0, 0, 16, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9,
                10, 11, 12, 13, 14, 15
            ]
        )
    }
}
