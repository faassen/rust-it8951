use bincode::config::Options;
use serde::{Deserialize, Serialize};

fn main() {
    println!("Hello, world!");
}

#[repr(C)]
struct IT8951_inquiry {
    dontcare: [u8; 8],
    vendor_id: [u8; 8],
    product_id: [u8; 16],
    product_ver: [u8; 4],
}

#[repr(C)]
struct IT8951_deviceinfo {
    ui_standard_cmd_no: u32,
    ui_extended_cmd_no: u32,
    ui_signature: u32,
    ui_version: u32,
    width: u32,
    height: u32,
    update_buffer_addr: u32,
    image_buffer_addr: u32,
    temperature_segment: u32,
    ui_mode: u32,
    frame_count: [u32; 8],
    buffer_count: u32,
    reversed: [u32; 9],
    // void *command_table
}

#[repr(C)]
#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct IT8951_area {
    address: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}
#[repr(C)]
struct IT8951_display_area {
    address: i32,
    wavemode: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    wait_ready: i32,
}

const LOAD_IMAGE_CMD: [u8; 16] = [
    0xfe, 0x00, 0x00, 0x00, 0x00, 0x00, 0xa2, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

fn load_image_area(f: i32, addr: i32, x: i32, y: i32, w: i32, h: i32, data: &[u8]) {
    let area = IT8951_area {
        address: addr, // note that this is assumed to be little endian
        x: x,
        y: y,
        w: w,
        h: h,
    };
    let length = w * h;

    let mut data_buffer: Vec<u8> = bincode::DefaultOptions::new()
        .with_big_endian()
        .serialize(&area)
        .unwrap();
    data_buffer.extend(data);
}
