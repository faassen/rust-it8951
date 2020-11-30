use bincode::config::Options;
use image::GenericImageView;
use rusb::open_device_with_vid_pid;
use rusb::{DeviceHandle, GlobalContext, Result};
use serde::{Deserialize, Serialize};
use std::mem;
use std::str;
use std::thread;
use std::time::Duration;

mod usb;

// XXX can I hardcode these?
// There is code to obtain these and check whether they are in and out
// endpoints.
const ENDPOINT_IN: u8 = 0x81;
const ENDPOINT_OUT: u8 = 0x02;

// maximum transfer size is 60k bytes for IT8951 USB
const MAX_TRANSFER: usize = 60 * 1024;

fn main() {
    println!("Start");
    let mut device_handle = open_it8951().expect("Cannot open it8951");
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
    let system_info = get_sys(&mut device_handle);
    println!("width: {}", system_info.width);
    println!("height: {}", system_info.height);
    println!("mode: {}", system_info.mode_no);

    println!("Display data");
    let img = image::open("cat.jpg").unwrap();
    let grayscale_image = img.grayscale();
    let data = grayscale_image.as_bytes();
    let (w, h) = img.dimensions();
    let image = Image { data, w, h };
    update_region(&mut device_handle, &system_info, &image, 0, 0, 2).unwrap();
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
struct Area {
    address: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}
#[repr(C)]
#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct DisplayArea {
    address: u32,
    display_mode: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    wait_ready: u32,
}

pub fn open_it8951() -> Option<DeviceHandle<GlobalContext>> {
    // XXX this should be replaced by something not for debugging only
    // XXX but that should be is unclear to me
    open_device_with_vid_pid(0x48d, 0x8951)
}

const INQUIRY_CMD: [u8; 16] = [0x12, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

fn inquiry(device_handle: &mut DeviceHandle<GlobalContext>) -> Inquiry {
    return usb::read_command(
        device_handle,
        ENDPOINT_OUT,
        ENDPOINT_IN,
        &INQUIRY_CMD,
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
        bincode::options().with_big_endian(),
    )
    .unwrap();
}

const LD_IMAGE_AREA_CMD: [u8; 16] = [
    0xfe, 0x00, 0x00, 0x00, 0x00, 0x00, 0xa2, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

fn ld_image_area(
    device_handle: &mut DeviceHandle<GlobalContext>,
    area: Area,
    data: &[u8],
) -> Result<()> {
    return usb::write_command(
        device_handle,
        ENDPOINT_OUT,
        ENDPOINT_IN,
        &LD_IMAGE_AREA_CMD,
        area,
        data,
        bincode::options(),
    );
}

const DPY_AREA_CMD: [u8; 16] = [
    0xfe, 0x00, 0x00, 0x00, 0x00, 0x00, 0x94, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

fn dpy_area(
    device_handle: &mut DeviceHandle<GlobalContext>,
    display_area: DisplayArea,
) -> Result<()> {
    return usb::write_command(
        device_handle,
        ENDPOINT_OUT,
        ENDPOINT_IN,
        &DPY_AREA_CMD,
        display_area,
        &[],
        bincode::options(),
    );
}

struct Image<'a> {
    data: &'a [u8],
    w: u32,
    h: u32,
}

fn update_region(
    device_handle: &mut DeviceHandle<GlobalContext>,
    info: &SystemInfo,
    image: &Image,
    x: u32,
    y: u32,
    mode: u32,
) -> Result<()> {
    let w: usize = image.w as usize;
    let h: usize = image.h as usize;
    let size = w * h;

    // we send the image in bands of MAX_TRANSFER
    let mut i: usize = 0;
    let mut row_height = (MAX_TRANSFER - mem::size_of::<Area>()) / w;
    while i < size {
        // we don't want to go beyond the end with the last band
        if (i / w) + row_height > h {
            row_height = h - (i / w);
        }
        ld_image_area(
            device_handle,
            Area {
                address: info.image_buffer_base,
                x,
                y: y + (i / w) as u32,
                w: image.w,
                h: row_height as u32,
            },
            &image.data[i..i + w * row_height],
        )?;
        i += row_height * w;
    }
    dpy_area(
        device_handle,
        DisplayArea {
            address: info.image_buffer_base,
            display_mode: mode,
            x,
            y,
            w: image.w,
            h: image.h,
            wait_ready: 1,
        },
    )?;
    return Ok(());
}
