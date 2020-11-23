use bincode::config::Options;
use rusb::{DeviceHandle, GlobalContext};
use serde::{Deserialize, Serialize};
use std::time::Duration;

mod usb;

fn main() {
    println!("Start");
    let mut device_handle = usb::open_it8951().expect("Cannot open it8951");
    device_handle
        .set_auto_detach_kernel_driver(true)
        .expect("auto detached failed");
    device_handle.claim_interface(0).expect("claim failed");
    inquiry(&device_handle);
    device_handle.release_interface(0).expect("release failed");
    println!("End");
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

const INQUIRY_CMD: [u8; 16] = [0x12, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

struct InquiryResult {
    vendor: [u8; 8],
    product: [u8; 16],
    revision: [u8; 4],
}

// XXX can I hardcode these?
// There is code to obtain these and check whether they are in and out
// endpoints
const ENDPOINT_IN: u8 = 0x81;
const ENDPOINT_OUT: u8 = 0x02;

fn inquiry(device_handle: &DeviceHandle<GlobalContext>) -> InquiryResult {
    usb::send_mass_storage_command(
        device_handle,
        ENDPOINT_OUT,
        INQUIRY_CMD,
        36,
        usb::Direction::IN,
    )
    .expect("Cannot send inquiry command");
    let mut buf: [u8; 256] = [0; 256];
    let size = device_handle
        .read_bulk(ENDPOINT_IN, &mut buf, Duration::from_millis(1000))
        .expect("failed to bulk read");
    println!("size {}", size);
    InquiryResult {
        vendor: [0, 0, 0, 0, 0, 0, 0, 0],
        product: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        revision: [0, 0, 0, 0],
    }
}

const LOAD_IMAGE_CMD: [u8; 16] = [
    0xfe, 0x00, 0x00, 0x00, 0x00, 0x00, 0xa2, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

const DISPLAY_AREA_CMD: [u8; 16] = [
    0xfe, 0x00, 0x00, 0x00, 0x00, 0x00, 0x94, 0, 0, 0, 0, 0, 0, 0, 0, 0,
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
