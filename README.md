## USB IT8951 epaper driver Rust

Waveshare sells a [range of epaper
displays](https://www.waveshare.com/product/displays/e-paper/epaper-1.htm),
some of which ship with a IT8951 display HAT. This IT8951 can be controlled via
SPI (typically through a Raspberry Pi) but also through USB, which I was
interested in so I could control it with my Linux desktop.

Thankfully, a large part of the problem had already been solved by [Martijn
Braam in C](https://blog.brixit.nl/epaper/), and his solution inspired mine.
One drawback of Martijn Braam's solution is that it requires root access as it
uses a specific Linux ioctl command to send data to the epaper display.

Martijn Braam links to a [IT8951 USB Programming
Guide](https://www.waveshare.com/w/upload/c/c9/IT8951_USB_ProgrammingGuide_v.0.4_20161114.pdf)
which gave me the clue that it should be possible to do this directly over USB
without root access. After quite some digging I've implemented a working
solution in Rust.

## Preparation

In order to make this work you need to create a udev rule that gives users
permission to talk to this device. To this end add a file `60-it8951.rules`
to the `/etc/udev/rules.d/etc/udev/rules.d` directory with the following
contents:

```
SUBSYSTEM=="usb", ATTRS{idVendor}=="048d", MODE="0666"
```

This gives applications access to talk to devices by vendor "048d", which is
the IT8951. You can then restart your system, or by write this to trigger
without reboot:

```
udevadm control --reload-rules && udevadm trigger
```

## Custom SCSI commands over SCSI over USB

The IT8951 implements custom SCSI commands to control the epaper display.
Normally SCSI is a disk protocol, but with these custom commands it can be
extended to arbitrary new commands. It's possible to send SCSI commands over
USB: you need to wrap them in a Command Block Wrapper and Command Status
Wrapper. Unlike the ioctl which can be used to talk to SCSI directly, USB may
be controlled by users, which is how this code works without requiring root
access.
