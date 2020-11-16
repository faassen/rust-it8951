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

static TAG: AtomicU32 = AtomicU32::new(0);

pub fn open_it8951() -> Option<DeviceHandle<GlobalContext>> {
    open_device_with_vid_pid(0x1B3F, 0x30FE)
}

pub fn send_mass_storage_command(
    device_handle: DeviceHandle<GlobalContext>,
    endpoint: u8,
    command_data: [u8; 16],
    data_transfer_length: u32,
    direction: Direction,
) -> Result<usize> {
    let flags: u8 = match direction {
        Direction::IN => 0x80,
        Direction::OUT => 0x00,
        Direction::NONE => 0x00,
    };
    let cwb = CommandBlockWrapper {
        signature: [0x55, 0x53, 0x42, 0x43],
        tag: TAG.fetch_add(1, Ordering::SeqCst),
        data_transfer_length: data_transfer_length,
        flags: flags,
        logical_unit_number: 0,
        command_length: 16,
        command_data: command_data,
    };
    let data_buffer: Vec<u8> = bincode::DefaultOptions::new().serialize(&cwb).unwrap();

    return device_handle.write_bulk(endpoint, &data_buffer, Duration::from_millis(1000));
}
