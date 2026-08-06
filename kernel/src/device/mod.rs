// SPDX-License-Identifier: MPL-2.0

mod full;
mod null;
mod pty;
mod random;
mod shm;
pub mod tty;
mod urandom;
mod zero;

#[cfg(all(target_arch = "x86_64", feature = "cvm_guest"))]
mod tdxguest;

use alloc::format;

pub use pty::{new_pty_pair, PtyMaster, PtySlave};
pub use random::Random;
pub use urandom::Urandom;

use crate::{
    fs::{
        device::{add_node, Device, DeviceId, DeviceType},
        utils::CStr256,
    },
    prelude::*,
};

/// Init the device node in fs, must be called after mounting rootfs.
pub fn init_in_first_process(ctx: &Context) -> Result<()> {
    ostd::console::early_print(format_args!("[device.ifp] start\n"));

    // Diagnostic: test heap allocation in the first user-process context.
    ostd::console::early_print(format_args!("[device.ifp] testmap new\n"));
    let mut test_map: hashbrown::HashMap<CStr256, usize> = hashbrown::HashMap::new();
    ostd::console::early_print(format_args!("[device.ifp] testmap insert\n"));
    test_map.insert(CStr256::from("x"), 1usize);
    ostd::console::early_print(format_args!("[device.ifp] testmap done\n"));

    ostd::console::early_print(format_args!("[device.ifp] testvec new\n"));
    let mut test_vec: alloc::vec::Vec<(CStr256, usize)> = alloc::vec::Vec::new();
    ostd::console::early_print(format_args!("[device.ifp] testvec push\n"));
    test_vec.push((CStr256::from("x"), 1usize));
    ostd::console::early_print(format_args!("[device.ifp] testvec done\n"));

    let fs = ctx.thread_local.borrow_fs();
    let fs_resolver = fs.resolver().read();

    let null = Arc::new(null::Null);
    add_node(null, "null", &fs_resolver)?;
    ostd::console::early_print(format_args!("[device.ifp] null\n"));
    let zero = Arc::new(zero::Zero);
    add_node(zero, "zero", &fs_resolver)?;
    ostd::console::early_print(format_args!("[device.ifp] zero\n"));

    tty::init();
    ostd::console::early_print(format_args!("[device.ifp] tty init\n"));

    let tty = Arc::new(tty::TtyDevice);
    add_node(tty, "tty", &fs_resolver)?;
    ostd::console::early_print(format_args!("[device.ifp] tty\n"));

    if let Some(console) = tty::system_console_opt() {
        add_node(console.clone(), "console", &fs_resolver)?;
        ostd::console::early_print(format_args!("[device.ifp] console\n"));
    } else {
    }

    for (index, tty) in tty::iter_n_tty().enumerate() {
        add_node(tty.clone(), &format!("tty{}", index), &fs_resolver)?;
    }
    ostd::console::early_print(format_args!("[device.ifp] iter_tty\n"));

    #[cfg(target_arch = "x86_64")]
    ostd::if_tdx_enabled!({
        add_node(Arc::new(tdxguest::TdxGuest), "tdx_guest", &fs_resolver)?;
    });

    let random = Arc::new(random::Random);
    add_node(random, "random", &fs_resolver)?;
    ostd::console::early_print(format_args!("[device.ifp] random\n"));

    let urandom = Arc::new(urandom::Urandom);
    add_node(urandom, "urandom", &fs_resolver)?;
    ostd::console::early_print(format_args!("[device.ifp] urandom\n"));

    let full = Arc::new(full::Full);
    add_node(full, "full", &fs_resolver)?;
    ostd::console::early_print(format_args!("[device.ifp] full\n"));

    pty::init_in_first_process(&fs_resolver)?;
    ostd::console::early_print(format_args!("[device.ifp] pty\n"));

    shm::init_in_first_process(&fs_resolver)?;
    ostd::console::early_print(format_args!("[device.ifp] shm\n"));

    Ok(())
}

// TODO: Implement a more scalable solution for ID-to-device mapping.
// Instead of hardcoding every device numbers in this function,
// a registration mechanism should be used to allow each driver to
// allocate device IDs either statically or dynamically.
pub fn get_device(devid: DeviceId) -> Result<Arc<dyn Device>> {
    let major = devid.major();
    let minor = devid.minor();

    match (major, minor) {
        (1, 3) => Ok(Arc::new(null::Null)),
        (1, 5) => Ok(Arc::new(zero::Zero)),
        (5, 0) => Ok(Arc::new(tty::TtyDevice)),
        (1, 8) => Ok(Arc::new(random::Random)),
        (1, 9) => Ok(Arc::new(urandom::Urandom)),
        _ => return_errno_with_message!(Errno::EINVAL, "the device ID is invalid or unsupported"),
    }
}
