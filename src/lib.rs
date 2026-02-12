use bincode::config::Options;
use image::GenericImageView;
use rusb::DeviceHandle;
use rusb::{Context, Result, UsbContext};
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
const VENDOR_ID: u16 = 0x48d;
const PRODUCT_ID: u16 = 0x8951;
const PMIC_CONTROL: [u8; 16] = [
    0xfe, 0x00, 0x00, 0x00, 0x00, 0x00, 0xa3, 0x00, /* Set VCom 15:8 */
    0x00, /* Set VCom 7:0 */
    0x00, /* Do set VCom? 0 - no, 1 - yes */
    0x00, /* Do Power on/off 0 - no, 1 - Do */
    0x00, /* Power 1 - on/ 0 - off */
    0x00, 0x00, 0x00, 0x00,
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

fn open_it8951() -> Result<DeviceHandle<Context>> {
    let context = Context::new()?;
    let devices = context.devices()?;

    let device = devices
        .iter()
        .find(|device| match device.device_descriptor() {
            Ok(descriptor) => {
                descriptor.vendor_id() == VENDOR_ID && descriptor.product_id() == PRODUCT_ID
            }
            Err(e) => {
                eprintln!("Could not find device descriptor: {e:?}");
                false
            }
        });

    if let Some(device) = device {
        let handle = device.open()?;
        Ok(handle)
    } else {
        Err(rusb::Error::NoDevice)
    }
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
        let mut device_handle = open_it8951()?;
        if let Err(e) = device_handle.set_auto_detach_kernel_driver(true) {
            println!("auto detached failed, error is {e}");
        }
        device_handle.claim_interface(0)?;
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

    /// Power On/Off display
    pub fn set_power(&mut self, on: bool) -> Result<()> {
        let mut pmic_control = PMIC_CONTROL;
        // Set do power on to yes
        pmic_control[10] = 1;
        // Power on or off?
        pmic_control[11] = on as u8;

        self.connection.write_command_no_data(&pmic_control)?;

        Ok(())
    }

    /// Sets VCOM to 2500 (according to USB Programming Guide)
    /// <https://www.waveshare.com/w/upload/c/c9/IT8951_USB_ProgrammingGuide_v.0.4_20161114.pdf>
    pub fn set_vcom(&mut self) -> Result<()> {
        let mut pmic_control = PMIC_CONTROL;
        // Set VCom set to yes
        pmic_control[9] = 1;
        // Set VCom value to 2500
        pmic_control[7] = 0x09;
        pmic_control[8] = 0xc4;

        self.connection.write_command_no_data(&pmic_control)?;

        Ok(())
    }

    /// Sets VCOM to 2500 (according to USB Programming Guide) and sets power state of display
    /// <https://www.waveshare.com/w/upload/c/c9/IT8951_USB_ProgrammingGuide_v.0.4_20161114.pdf>
    pub fn set_power_vcom(&mut self, power_on: bool) -> Result<()> {
        let mut pmic_control = PMIC_CONTROL;
        // Set VCom set to yes
        pmic_control[9] = 1;
        // Set VCom value to 2500
        pmic_control[7] = 0x09;
        pmic_control[8] = 0xc4;

        // Set do power on to yes
        pmic_control[10] = 1;
        // Power on or off?
        pmic_control[11] = power_on as u8;

        self.connection.write_command_no_data(&pmic_control)?;

        Ok(())
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

    /// Load region of e-paper display
    pub fn load_region(&mut self, image: &image::DynamicImage, x: u32, y: u32) -> Result<()> {
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

        Ok(())
    }

    /// Show region of e-paper display.
    pub fn display_region(
        &mut self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        mode: Mode,
    ) -> Result<()> {
        // we send the image in bands of MAX_TRANSFER
        let address = self.get_system_info().unwrap().image_buffer_base;

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
