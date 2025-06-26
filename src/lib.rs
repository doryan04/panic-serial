//! Prints panic information via a serial port, then goes into an infinite loop.
//!
//! Status: experimental; biased towards Arduino
//!
//! This crate implements a panic handler which prints panic information on a serial port (or other type of output - see below).
//!
//! ## Why?
//!
//! Seeing panic messages (or at least their location) is essential to make sense of what went wrong.
//!
//! I don't want to live without it.
//!
//! ## What is printed?
//!
//! There are three levels of detail at which panics can be printed, depending on how much space you are willing to waste in your firmware.
//! The level of detail is chosen by selecting **feature flags**:
//! - `location`: prints location information.
//!   Example:
//!   ```
//!   Panic at src/main.rs:91:9
//!   ```
//! - `message`: prints the actual full panic message. This uses `core::fmt` under the hood, so expect an increase in firmware size.
//!   Example:
//!   ```
//!   attempt to subtract with overflow
//!   ```
//! - `full` == `location` & `message`: Combined location and message.
//!    Example:
//!    ```
//!    Panic at src/main.rs:91:9: attempt to subtract with overflow
//!    ```
//! - (no features): if no features are chosen, a static message is printed.
//!    Example:
//!    ```
//!    PANIC !
//!    ```
//!    This option is easiest on firmware size.
//!
//! ## Usage
//!
//! An example project for Arduino Uno based on these instructions can be found here: <https://github.com/nilclass/panic-serial-example>.
//!
//! 1. Remove any existing panic handler. For example if you are currently using `panic_halt`, remove that dependency & it's usage.
//! 2. Add `panic-serial` dependency to your project:
//!    ```sh
//!    # Check "What is printed" section above for features to choose
//!    cargo add panic-serial --features full
//!    ```
//! 3. Within your `main.rs` (or elsewhere at top level) invoke the `impl_panic_handler` macro:
//!    ```
//!    panic_serial::impl_panic_handler!(
//!      // This is the type of the UART port to use for printing the message:
//!      arduino_hal::usart::Usart<
//!        arduino_hal::pac::USART0,
//!        arduino_hal::port::Pin<arduino_hal::port::mode::Input, arduino_hal::hal::port::PD0>,
//!        arduino_hal::port::Pin<arduino_hal::port::mode::Output, arduino_hal::hal::port::PD1>
//!      >
//!    );
//!    ```
//!   This will do two things:
//!   - define the actual panic handler
//!   - define a function called `share_serial_port_with_panic`, which we'll use in the next step
//! 4. Call `share_serial_port_with_panic` within `main`:
//!    ```
//!    #[arduino_hal::entry]
//!    fn main() -> ! {
//!      // ...
//!      let serial = arduino_hal::default_serial!(dp, pins, 57600);
//!      // this gives ownership of the serial port to panic-serial. We receive a mutable reference to it though, so we can keep using it.
//!      let serial = share_serial_port_with_panic(serial);
//!      // continue using serial:
//!      ufmt::uwriteln!(serial, "Hello there!\r").unwrap();
//!
//!      // ...
//!    }
//!    ```
//!
//! ## How does it work?
//!
//! The `impl_panic_handler` macro defines a mutable static `PANIC_PORT: Option<$your_type>`.
//! When you call `share_serial_port_with_panic`, that option gets filled, and you get back `PANIC_PORT.as_mut().unwrap()`.
//!
//! If a panic happens, the panic handler either just loops (if you never called `share_serial_port_with_panic`), or prints
//! the panic info to the given port.
//! It does this in two steps:
//! 1. call `port.flush()`
//! 2. use `ufmt` (or `core::fmt`) to print the fragments.
//!
//! Technically this works with *anything* that implements `ufmt::uWrite` and has a `flush()` method.
//!
//! ## How unsafe is this?
//!
//! When you find out, please tell me.
//!

#![no_std]
#![feature(panic_info_message)]

use ufmt::uWrite;
use core::panic::PanicInfo;
use core::fmt::Write;

struct WriteWrapper<'a, W: uWrite>(&'a mut W);

impl<'a, W: uWrite> Write for WriteWrapper<'a, W> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.0.write_str(s).map_err(|_| core::fmt::Error)
    }
}

/// Called internally by the panic handler.
pub fn _print_panic<W: uWrite>(w: &mut W, info: &PanicInfo) {
    let location_feature = cfg!(feature="location");
    let message_feature = cfg!(feature="message");

    if location_feature {
        if let Some(location) = info.location() {
            _ = ufmt::uwrite!(w, "Panic at {}:{}:{}", location.file(), location.line(), location.column());
            _ = w.write_str(if message_feature { ": " } else { "\r\n" });
        }
    }

    if message_feature {
        if let Some(str) = info.message().as_str() {
            _ = core::fmt::write(&mut WriteWrapper(w), format_args!("{}", str));
            _ = w.write_str("\r\n");
        }
    }

    if !message_feature && !location_feature {
        _ = ufmt::uwriteln!(w, "PANIC !\r");
    }
}

/// Implements the panic handler. You need to call this for the package to work.
///
/// This macro defines the panic handler, as well as a function called `share_serial_port_with_panic`.
/// That function takes an argument of the given `$type` and returns a `&'static mut $type`.
///
#[macro_export]
macro_rules! impl_panic_handler {
    ($type:ty) => {
        static mut PANIC_PORT: Option<$type> = None;

        #[inline(never)]
        #[panic_handler]
        fn panic(info: &::core::panic::PanicInfo) -> ! {
            if let Some(panic_port) = unsafe { PANIC_PORT.as_mut() } {
                _ = panic_port.flush();
                ::panic_serial::_print_panic(panic_port, info);
            }
            loop {
                ::core::sync::atomic::compiler_fence(::core::sync::atomic::Ordering::SeqCst);
            }
        }

        pub fn share_serial_port_with_panic(port: $type) -> &'static mut $type {
            unsafe {
                PANIC_PORT = Some(port);
                PANIC_PORT.as_mut().unwrap()
            }
        }
    };
}
