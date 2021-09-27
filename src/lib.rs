use bincode::config::Options;
use image::GenericImageView;
use rusb::open_device_with_vid_pid;
use rusb::{DeviceHandle, GlobalContext, Result};
use serde::{Deserialize, Serialize};
use std::mem;
use std::str;
use std::thread;
use std::time::Duration;

pub mod usb;

// XXX can I hardcode these?
// There is code to obtain these and check whether they are in and out
// endpoints.
const ENDPOINT_IN: u8 = 0x81;
const ENDPOINT_OUT: u8 = 0x02;

// maximum transfer size is 60k bytes for IT8951 USB
const MAX_TRANSFER: usize = 60 * 1024;

const INQUIRY_CMD: [u8; 16] = [0x12, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const GET_SYS_CMD: [u8; 16] = [
    0xfe, 0, 0x38, 0x39, 0x35, 0x31, 0x80, 0, 0x01, 0, 0x02, 0, 0, 0, 0, 0,
];
const LD_IMAGE_AREA_CMD: [u8; 16] = [
    0xfe, 0x00, 0x00, 0x00, 0x00, 0x00, 0xa2, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];
const DPY_AREA_CMD: [u8; 16] = [
    0xfe, 0x00, 0x00, 0x00, 0x00, 0x00, 0x94, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

pub enum Mode {
    INIT = 0,
    DU,
    GC16,
    GL16,
    GLR16,
    GLD16,
    A2,
    DU4,
}

#[repr(C)]
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct SystemInfo {
    standard_cmd_no: u32,
    extended_cmd_no: u32,
    signature: u32,
    pub version: u32,
    pub width: u32,
    pub height: u32,
    update_buf_base: u32,
    image_buffer_base: u32,
    temperature_no: u32,
    pub mode_no: u32,
    frame_count: [u32; 8],
    num_img_buf: u32,
    reserved: [u32; 9],
    // command_table_ptr: [u32; 1],
}

#[repr(C)]
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Inquiry {
    ignore_start: [u8; 8],
    pub vendor: [u8; 8],
    pub product: [u8; 16],
    pub revision: [u8; 4],
    ignore_end: [u8; 4],
}

// maybe this should contain i32 not u32?
#[repr(C)]
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Area {
    address: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

// maybe this should contain i32 not u32?
#[repr(C)]
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct DisplayArea {
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

pub struct It8951<'a> {
    pub connection: usb::ScsiOverUsbConnection<'a>,
}

impl<'a> It8951<'a> {
    pub fn inquiry(&mut self) -> Result<Inquiry> {
        return self
            .connection
            .read_command(&INQUIRY_CMD, bincode::options());
    }

    pub fn get_sys(&mut self) -> Result<SystemInfo> {
        return self
            .connection
            .read_command(&GET_SYS_CMD, bincode::options().with_big_endian());
    }

    pub fn ld_image_area(&mut self, area: Area, data: &[u8]) -> Result<()> {
        return self.connection.write_command(
            &LD_IMAGE_AREA_CMD,
            area,
            data,
            bincode::options().with_big_endian(),
        );
    }

    pub fn dpy_area(&mut self, display_area: DisplayArea) -> Result<()> {
        return self.connection.write_command(
            &DPY_AREA_CMD,
            display_area,
            &[],
            bincode::options().with_big_endian(),
        );
    }

    pub fn update_region(
        &mut self,
        info: &SystemInfo,
        image: &image::DynamicImage,
        x: u32,
        y: u32,
        mode: u32,
    ) -> Result<()> {
        let data = image.as_bytes();
        let (width, height) = image.dimensions();

        let w: usize = width as usize;
        let h: usize = height as usize;
        let size = w * h;

        // we send the image in bands of MAX_TRANSFER
        let mut i: usize = 0;
        let mut row_height = (MAX_TRANSFER - mem::size_of::<Area>()) / w;
        while i < size {
            // we don't want to go beyond the end with the last band
            if (i / w) + row_height > h {
                row_height = h - (i / w);
            }
            self.ld_image_area(
                Area {
                    address: info.image_buffer_base,
                    x,
                    y: y + (i / w) as u32,
                    w: width,
                    h: row_height as u32,
                },
                &data[i..i + w * row_height],
            )?;
            i += row_height * w;
        }
        self.dpy_area(DisplayArea {
            address: info.image_buffer_base,
            display_mode: mode,
            x,
            y,
            w: width,
            h: height,
            wait_ready: 1,
        })?;
        return Ok(());
    }
}
