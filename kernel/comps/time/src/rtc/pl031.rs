// SPDX-License-Identifier: MPL-2.0

//! PL031 RTC driver for AArch64 QEMU virt machine.
//!
//! The PL031 is an ARM PrimeCell Real Time Clock. On QEMU virt AArch64,
//! it is accessible via MMIO at an address found in the device tree.

use ostd::{arch::boot::DEVICE_TREE, io::IoMem, mm::VmIoOnce};
use chrono::{DateTime, Datelike, Timelike};

use super::Driver;
use crate::SystemTime;

/// PL031 RTC register offset for the data register (current time as Unix timestamp)
const RTC_DR: usize = 0x00;

pub struct RtcPl031 {
    io_mem: IoMem,
}

impl Driver for RtcPl031 {
    fn try_new() -> Option<Self> {
        // Look for a PL031-compatible node in the device tree
        let dt = DEVICE_TREE.get()?;
        // Only match "arm,pl031" – generic "arm,primecell" is shared by many
        // other peripherals (e.g. PL011 UART) and must not be probed as RTC.
        let node = dt.find_compatible(&["arm,pl031"])?;

        let region = node.reg()?.next()?;
        let start = region.starting_address as usize;
        let size = region.size.unwrap_or(0x1000);
        let io_mem = IoMem::acquire(start..start + size).ok()?;
        Some(RtcPl031 { io_mem })
    }

    fn read_rtc(&self) -> SystemTime {
        // Read the 32-bit seconds-since-epoch value from RTC_DR
        let seconds: u32 = self.io_mem.read_once(RTC_DR).unwrap_or(0);
        let dt = DateTime::from_timestamp(seconds as i64, 0)
            .unwrap_or_default()
            .naive_utc();
        let (is_ad, year) = dt.year_ce();
        if !is_ad {
            return SystemTime {
                year: 1970,
                month: 1,
                day: 1,
                hour: 0,
                minute: 0,
                second: 0,
                nanos: 0,
            };
        }
        SystemTime {
            year: year as u16,
            month: dt.month() as u8,
            day: dt.day() as u8,
            hour: dt.hour() as u8,
            minute: dt.minute() as u8,
            second: dt.second() as u8,
            nanos: 0,
        }
    }
}
