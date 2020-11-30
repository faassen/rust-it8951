use bincode::config::Options;
use rusb::{DeviceHandle, Error, GlobalContext, Result};
use serde::{Deserialize, Serialize};
use std::mem;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

// this implements sending SCSI commands over USB

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum Direction {
    IN,
    OUT,
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
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct CommandStatusWrapper {
    signature: [u8; 4],
    tag: u32,
    data_residue: u32,
    status: u8,
}

static TAG: AtomicU32 = AtomicU32::new(1);

pub fn read_command<T: serde::de::DeserializeOwned, O: bincode::config::Options>(
    device_handle: &mut DeviceHandle<GlobalContext>,
    endpoint_out: u8,
    endpoint_in: u8,
    command: &[u8; 16],
    bincode_options: O,
) -> Result<T> {
    let length = mem::size_of::<T>();
    // issue CBW block
    let cbw_data = &get_command_block_wrapper(command, length as u32, Direction::IN);
    device_handle.write_bulk(endpoint_out, &cbw_data, Duration::from_millis(1000))?;

    // now read the data
    let mut buf: Vec<u8> = Vec::with_capacity(length);
    buf.resize(length, 0);
    device_handle.read_bulk(endpoint_in, &mut buf, Duration::from_millis(1000))?;

    // issue CBS block
    send_status_block_wrapper(device_handle, endpoint_in)?;

    // transform data into required data
    let result: T = bincode_options
        .with_fixint_encoding()
        .deserialize(&buf)
        .unwrap();
    return Ok(result);
}

pub fn write_command<T: Serialize, O: bincode::config::Options>(
    device_handle: &mut DeviceHandle<GlobalContext>,
    endpoint_out: u8,
    endpoint_in: u8,
    command: &[u8; 16],
    value: T,
    data: &[u8],
    bincode_options: O,
) -> Result<()> {
    // transform the value into data
    let mut value_data: Vec<u8> = bincode_options
        .with_fixint_encoding()
        .with_big_endian()
        .serialize(&value)
        .unwrap();
    // combine this with any additional data
    let mut bulk_data: Vec<u8> = Vec::new();
    bulk_data.append(&mut value_data);
    bulk_data.extend_from_slice(data);

    // issue CBW block
    let cbw_data = &get_command_block_wrapper(command, bulk_data.len() as u32, Direction::OUT);
    device_handle.write_bulk(endpoint_out, &cbw_data, Duration::from_millis(1000))?;

    // now write the data for the value
    device_handle.write_bulk(endpoint_out, &bulk_data, Duration::from_millis(1000))?;

    // issue CBS block
    send_status_block_wrapper(device_handle, endpoint_in)?;

    return Ok(());
}

pub fn send_status_block_wrapper(
    device_handle: &mut DeviceHandle<GlobalContext>,
    endpoint_in: u8,
) -> Result<CommandStatusWrapper> {
    let mut csb_data: [u8; 13] = [0; 13];
    loop {
        match device_handle.read_bulk(endpoint_in, &mut csb_data, Duration::from_millis(1000)) {
            Ok(_size) => {
                return Ok(bincode::options()
                    .with_fixint_encoding()
                    .deserialize::<CommandStatusWrapper>(&csb_data)
                    .unwrap());
            }
            Err(error) => match error {
                Error::Pipe => {
                    device_handle.clear_halt(endpoint_in).unwrap();
                    continue;
                }
                _ => {
                    return Err(error);
                }
            },
        }
    }
}

pub fn get_command_block_wrapper(
    command_data: &[u8; 16],
    data_transfer_length: u32,
    direction: Direction,
) -> Vec<u8> {
    let flags: u8 = match direction {
        Direction::IN => 0x80,
        Direction::OUT => 0x00,
    };
    let tag = TAG.fetch_add(1, Ordering::SeqCst);
    let cwb = CommandBlockWrapper {
        signature: [0x55, 0x53, 0x42, 0x43],
        tag: tag,
        data_transfer_length: data_transfer_length,
        flags: flags,
        logical_unit_number: 0,
        command_length: 16,
        command_data: *command_data,
    };
    bincode::options()
        .with_little_endian()
        .with_fixint_encoding()
        .serialize(&cwb)
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mass_storage_command_data() {
        let data = get_command_block_wrapper(
            &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
            18,
            Direction::OUT,
        );
        assert_eq!(
            data,
            [
                85, 83, 66, 67, 1, 0, 0, 0, 18, 0, 0, 0, 0, 0, 16, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9,
                10, 11, 12, 13, 14, 15
            ]
        );
        // the second time the tag increases
        let data2 = get_command_block_wrapper(
            &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
            18,
            Direction::OUT,
        );
        assert_eq!(
            data2,
            [
                85, 83, 66, 67, 2, 0, 0, 0, 18, 0, 0, 0, 0, 0, 16, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9,
                10, 11, 12, 13, 14, 15
            ]
        )
    }
}
