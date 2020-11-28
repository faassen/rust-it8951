use bincode::config::Options;
use rusb::{DeviceHandle, GlobalContext};
use serde::{Deserialize, Serialize};
use std::str;
use std::thread;
use std::time::Duration;

mod usb;

// XXX can I hardcode these?
// There is code to obtain these and check whether they are in and out
// endpoints.
const ENDPOINT_IN: u8 = 0x81;
const ENDPOINT_OUT: u8 = 0x02;

fn main() {
    println!("Start");
    let mut device_handle = usb::open_it8951().expect("Cannot open it8951");
    device_handle
        .set_auto_detach_kernel_driver(true)
        .expect("auto detached failed");
    device_handle.claim_interface(0).expect("claim failed");
    let inquiry_result = inquiry(&mut device_handle);
    println!(
        "vendor: {}",
        str::from_utf8(&inquiry_result.vendor).unwrap()
    );
    println!(
        "product: {}",
        str::from_utf8(&inquiry_result.product).unwrap()
    );
    println!(
        "revision: {}",
        str::from_utf8(&inquiry_result.revision).unwrap()
    );
    thread::sleep(Duration::from_millis(100));
    println!("We are now reading data");
    let sys_info = get_sys(&mut device_handle);
    println!("width: {}", sys_info.width);
    println!("height: {}", sys_info.height);
    println!("mode: {}", sys_info.mode_no);

    device_handle.release_interface(0).expect("release failed");
    println!("End");
}

#[repr(C)]
#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct SystemInfo {
    standard_cmd_no: u32,
    extended_cmd_no: u32,
    signature: u32,
    version: u32,
    width: u32,
    height: u32,
    update_buf_base: u32,
    image_buffer_base: u32,
    temperature_no: u32,
    mode_no: u32,
    frame_count: [u32; 8],
    num_img_buf: u32,
    reserved: [u32; 9],
    // command_table_ptr: [u32; 1],
}

#[repr(C)]
#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Inquiry {
    ignore_start: [u8; 8],
    vendor: [u8; 8],
    product: [u8; 16],
    revision: [u8; 4],
    ignore_end: [u8; 4],
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

fn inquiry(device_handle: &mut DeviceHandle<GlobalContext>) -> Inquiry {
    return usb::read_command(
        device_handle,
        ENDPOINT_OUT,
        ENDPOINT_IN,
        &INQUIRY_CMD,
        40,
        bincode::options(),
    )
    .unwrap();
}

const GET_SYS_CMD: [u8; 16] = [
    0xfe, 0, 0x38, 0x39, 0x35, 0x31, 0x80, 0, 0x01, 0, 0x02, 0, 0, 0, 0, 0,
];

fn get_sys(device_handle: &mut DeviceHandle<GlobalContext>) -> SystemInfo {
    return usb::read_command(
        device_handle,
        ENDPOINT_OUT,
        ENDPOINT_IN,
        &GET_SYS_CMD,
        112,
        bincode::options().with_big_endian(),
    )
    .unwrap();
}

const LD_IMAGE_AREA_CMD: [u8; 16] = [
    0xfe, 0x00, 0x00, 0x00, 0x00, 0x00, 0xa2, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

// const DISPLAY_AREA_CMD: [u8; 16] = [
//     0xfe, 0x00, 0x00, 0x00, 0x00, 0x00, 0x94, 0, 0, 0, 0, 0, 0, 0, 0, 0,
// ];

fn ld_image_area(f: i32, addr: i32, x: i32, y: i32, w: i32, h: i32, data: &[u8]) {
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
