use rust_it8951::usb;
use rust_it8951::{open_it8951, It8951};
use std::str;
use std::thread;
use std::time::Duration;

// XXX can I hardcode these?
// There is code to obtain these and check whether they are in and out
// endpoints.
const ENDPOINT_IN: u8 = 0x81;
const ENDPOINT_OUT: u8 = 0x02;

fn main() {
    let timeout = Duration::from_millis(1000);
    println!("Start");
    let mut device_handle = open_it8951().expect("Cannot open it8951");
    device_handle
        .set_auto_detach_kernel_driver(true)
        .expect("auto detached failed");
    device_handle.claim_interface(0).expect("claim failed");

    let mut it8951 = It8951 {
        connection: usb::ScsiOverUsbConnection {
            device_handle: &mut device_handle,
            endpoint_out: ENDPOINT_OUT,
            endpoint_in: ENDPOINT_IN,
            timeout: timeout,
        },
    };

    let inquiry_result = it8951.inquiry().unwrap();
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
    let system_info = it8951.get_sys().unwrap();
    println!("width: {}", system_info.width);
    println!("height: {}", system_info.height);
    println!("mode: {}", system_info.mode_no);
    println!("version: {}", system_info.version);

    println!("Display data");
    let img = image::open("kitten.jpg").unwrap();
    let grayscale_image = img.grayscale();

    // it8951.update_region(&system_info, &[], 0, 0, 0).unwrap();

    // 0 INIT: works - whole screen blanks
    // 1 DU:
    // 2: GC16: partial update, greyscale
    // 3: GL16
    // 4: GLR16
    // 5: GLD16
    // 6: DU4: 4 gray times
    // 7: A2: 2 bit pictures

    it8951
        .update_region(&system_info, &grayscale_image, 0, 0, 2)
        .unwrap();
    device_handle.release_interface(0).expect("release failed");
    println!("End");
}
