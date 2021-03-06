# hf2
Implements [Microsofts HID Flashing Format (HF2)](https://github.com/microsoft/uf2/blob/86e101e3a282553756161fe12206c7a609975e70/hf2.md) to upload firmware to UF2 bootloaders. UF2 is factory programmed extensively by [Microsoft MakeCode](https://www.microsoft.com/en-us/makecode) and [Adafruit](https://www.adafruit.com) hardware.

## prerequisites
Utilizes the [hidapi-sys crate](https://crates.io/crates/hidapi) which uses [libusb](https://github.com/libusb/hidapi).

### linux
Youll need libusb depending on your distro you might do `sudo apt-get install libudev-dev libusb-1.0-0-dev`.

If you'd like to not use sudo, you'll need udev rules. With your board plugged in and in bootloader mode, use `lsusb` to find your vendorid, seen here as 239a
```
Bus 001 Device 087: ID 239a:001b Adafruit Industries Feather M0
```
Then put your vendorid below and save to something like /etc/udev/rules.d/99-adafruit-boards.rules
```
ATTRS{idVendor}=="239a", ENV{ID_MM_DEVICE_IGNORE}="1"
SUBSYSTEM=="usb", ATTRS{idVendor}=="239a", MODE="0666"
SUBSYSTEM=="tty", ATTRS{idVendor}=="239a", MODE="0666"
```
Then reboot or run
```
sudo udevadm control --reload-rules
sudo udevadm trigger
```

### mac
On mac, as of Catalina you will get a permissions prompt and must follow directions to allow "Input Monitoring" for the Terminal application. 

## install
`cargo install cargo-hf2`

## use
```
let chk: ChksumPagesResponse = ChksumPages {
    0x4000,
    1,
}.send(&d)?;
println!(chk.chksums);
```

## troubleshooting

If it cant find a device, make sure your device is in a bootloader mode ready to receive firmware.
```
thread 'main' panicked at 'Are you sure device is plugged in and in bootloader mode?: OpenHidDeviceError', src/libcore/result.rs:1165:5
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace.
```
On the PyGamer, 2 button presses enables a blue and green screen that says PyGamer and also generally creates a flash drive which you should be able to see (though this doesn't use that method).

If you find another error, be sure to run with debug to see where in the process it failed and include those logs when reporting
```
RUST_LOG=debug cargo run
```
