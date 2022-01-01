use bincode::config::Options;
use image::GenericImageView;
use rusb::open_device_with_vid_pid;
use rusb::{DeviceHandle, GlobalContext, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::mem;
use std::str;
use std::time::Duration;

mod usb;

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

/// Display mode
#[repr(u32)]
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum Mode {
    /// Blank screen
    INIT = 0,
    DU,
    /// Partial update, greyscale
    GC16,
    GL16,
    GLR16,
    GLD16,
    DU4, // or swap order?
    A2,
    /// Don't know what mode it is, but [Waveshare 7.8inch e-Paper HAT](https://www.waveshare.net/wiki/7.8inch_e-Paper_HAT) reports this mode.
    __UNKNOWN1,
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// System information about epaper panel.
#[repr(C)]
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct SystemInfo {
    standard_cmd_no: u32,
    extended_cmd_no: u32,
    signature: u32,
    /// Command table version
    pub version: u32,
    /// Panel width
    pub width: u32,
    /// Panel height
    pub height: u32,
    update_buf_base: u32,
    image_buffer_base: u32,
    temperature_no: u32,
    /// Display mode
    pub mode: Mode,
    frame_count: [u32; 8],
    num_img_buf: u32,
    reserved: [u32; 9],
    // command_table_ptr: [u32; 1],
}

#[repr(C)]
#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct CInquiry {
    ignore_start: [u8; 8],
    pub vendor: [u8; 8],
    pub product: [u8; 16],
    pub revision: [u8; 4],
    ignore_end: [u8; 4],
}

/// Inquiry.
///
/// If it works, it's going to be uninteresting:
/// ```
/// vendor: Generic
/// product: Storage RamDisc
/// revision: 1.00
// ```
pub struct Inquiry {
    pub vendor: String,
    pub product: String,
    pub revision: String,
}

/// An area
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
    display_mode: Mode,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    wait_ready: u32,
}

fn open_it8951() -> Option<DeviceHandle<GlobalContext>> {
    // XXX this should be replaced by something not for debugging only
    // XXX but that should be is unclear to me
    open_device_with_vid_pid(0x48d, 0x8951)
}

/// Talk to the It8951 e-paper display via a USB connection
pub struct It8951 {
    connection: usb::ScsiOverUsbConnection,
    system_info: Option<SystemInfo>,
}

impl Drop for It8951 {
    fn drop(&mut self) {
        self.connection
            .device_handle
            .release_interface(0)
            .expect("release failed");
    }
}

impl It8951 {
    /// Establish a connection to the e-paper display via the USB port.
    pub fn connect() -> Result<It8951> {
        // XXX hardcoded timeout
        let timeout = Duration::from_millis(1000);
        let mut device_handle = open_it8951().expect("Cannot open it8951");
        if let Err(e) = device_handle.set_auto_detach_kernel_driver(true) {
            println!("auto detached failed, error is {}", e);
        }
        device_handle.claim_interface(0).expect("claim failed");
        let mut result = It8951 {
            connection: usb::ScsiOverUsbConnection {
                device_handle,
                endpoint_out: ENDPOINT_OUT,
                endpoint_in: ENDPOINT_IN,
                timeout,
            },
            system_info: None,
        };
        let system_info = result.get_sys()?;
        result.system_info = Some(system_info);
        Ok(result)
    }

    /// Make an inquiry.
    pub fn inquiry(&mut self) -> Result<Inquiry> {
        let c_inquiry: CInquiry = self
            .connection
            .read_command(&INQUIRY_CMD, bincode::options())?;
        Ok(Inquiry {
            vendor: str::from_utf8(&c_inquiry.vendor).unwrap().to_string(),
            product: str::from_utf8(&c_inquiry.product).unwrap().to_string(),
            revision: str::from_utf8(&c_inquiry.revision).unwrap().to_string(),
        })
    }

    fn get_sys(&mut self) -> Result<SystemInfo> {
        self.connection
            .read_command(&GET_SYS_CMD, bincode::options().with_big_endian())
    }

    /// system info about e-paper display.
    pub fn get_system_info(&self) -> Option<&SystemInfo> {
        self.system_info.as_ref()
    }

    fn ld_image_area(&mut self, area: Area, data: &[u8]) -> Result<()> {
        self.connection.write_command(
            &LD_IMAGE_AREA_CMD,
            area,
            data,
            bincode::options().with_big_endian(),
        )
    }

    fn dpy_area(&mut self, display_area: DisplayArea) -> Result<()> {
        self.connection.write_command(
            &DPY_AREA_CMD,
            display_area,
            &[],
            bincode::options().with_big_endian(),
        )
    }

    /// Update region of e-paper display.
    pub fn update_region(
        &mut self,
        image: &image::DynamicImage,
        x: u32,
        y: u32,
        mode: Mode,
    ) -> Result<()> {
        let data = image.as_bytes();
        let (width, height) = image.dimensions();

        let w: usize = width as usize;
        let h: usize = height as usize;
        let size = w * h;

        // we send the image in bands of MAX_TRANSFER
        let mut i: usize = 0;
        let mut row_height = (MAX_TRANSFER - mem::size_of::<Area>()) / w;
        let address = self.get_system_info().unwrap().image_buffer_base;
        while i < size {
            // we don't want to go beyond the end with the last band
            if (i / w) + row_height > h {
                row_height = h - (i / w);
            }
            self.ld_image_area(
                Area {
                    address,
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
            address,
            display_mode: mode,
            x,
            y,
            w: width,
            h: height,
            wait_ready: 1,
        })?;
        Ok(())
    }
}
