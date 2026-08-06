// SPDX-License-Identifier: MPL-2.0

//! Aster-nix is the Asterinas kernel, a safe, efficient unix-like
//! operating system kernel built on top of OSTD and OSDK.

#![no_std]
#![no_main]
#![deny(unsafe_code)]
#![feature(btree_cursors)]
#![feature(btree_extract_if)]
#![feature(debug_closure_helpers)]
#![feature(extend_one)]
#![feature(extract_if)]
#![feature(fn_traits)]
#![feature(format_args_nl)]
#![feature(int_roundings)]
#![feature(integer_sign_cast)]
#![feature(let_chains)]
#![feature(linked_list_cursors)]
#![feature(linked_list_remove)]
#![feature(linked_list_retain)]
#![feature(negative_impls)]
#![feature(panic_can_unwind)]
#![feature(register_tool)]
#![feature(min_specialization)]
#![feature(step_trait)]
#![feature(trait_alias)]
#![feature(trait_upcasting)]
#![feature(associated_type_defaults)]
#![register_tool(component_access_control)]

use component::InitStage;
use kcmdline::KCmdlineArg;
use ostd::{
    arch::qemu::{exit_qemu, QemuExitCode},
    boot::boot_info,
    cpu::CpuId,
};
use process::{spawn_init_process, Process};
use sched::SchedPolicy;

use crate::{fs::fs_resolver::FsResolver, prelude::*, thread::kernel_thread::ThreadOptions};

extern crate alloc;
extern crate lru;
#[macro_use]
extern crate controlled;
#[macro_use]
extern crate getset;

#[cfg(target_arch = "x86_64")]
#[path = "arch/x86/mod.rs"]
mod arch;
#[cfg(target_arch = "riscv64")]
#[path = "arch/riscv/mod.rs"]
mod arch;
#[cfg(target_arch = "loongarch64")]
#[path = "arch/loongarch/mod.rs"]
mod arch;
#[cfg(target_arch = "aarch64")]
#[path = "arch/aarch64/mod.rs"]
mod arch;

mod context;
mod cpu;
mod device;
mod driver;
mod error;
mod events;
mod fs;
mod ipc;
mod kcmdline;
mod net;
mod prelude;
mod process;
mod sched;
mod syscall;
mod thread;
mod time;
mod util;
// TODO: Add vDSO support for other architectures.
#[cfg(any(target_arch = "x86_64", target_arch = "riscv64"))]
mod vdso;
mod vm;

#[ostd::main]
#[controlled]
fn main() {
    ostd::arch::boot::pl011_puts_safe(b"[KM.main] start\n");
    ostd::arch::boot::pl011_puts_safe(b"[KM.main] before component::init_all\n");
    ostd::arch::boot::pl011_puts_safe(b"[mac.3] before call\n");
    component::init_all(InitStage::Bootstrap, component::parse_metadata!()).unwrap();
    ostd::arch::boot::pl011_puts_safe(b"[mac.4] after call\n");
    ostd::arch::boot::pl011_puts_safe(b"[KM.main] after component::init_all\n");

    init();
    ostd::arch::boot::pl011_puts_safe(b"[KM.main] after init\n");

    // Spawn all AP idle threads.
    ostd::arch::boot::pl011_puts_safe(b"[KM.main] before register_ap_entry\n");
    ostd::boot::smp::register_ap_entry(ap_init);
    ostd::arch::boot::pl011_puts_safe(b"[KM.main] after register_ap_entry\n");

    init_on_each_cpu();
    ostd::arch::boot::pl011_puts_safe(b"[KM.main] after init_on_each_cpu\n");

    // Spawn the first kernel thread on BSP.
    ThreadOptions::new(first_kthread)
        .cpu_affinity(CpuId::bsp().into())
        .sched_policy(SchedPolicy::Idle)
        .spawn();
}

fn init() {
    ostd::arch::boot::pl011_puts_safe(b"DBG: init() start\n");
    ostd::arch::boot::pl011_puts_safe(b"DBG: before thread::init()\n");
    thread::init();
    ostd::arch::boot::pl011_puts_safe(b"DBG: thread::init() done\n");
    ostd::arch::boot::pl011_puts_safe(b"DBG: before util::random::init()\n");
    util::random::init();
    ostd::arch::boot::pl011_puts_safe(b"DBG: util::random::init() done\n");
    ostd::arch::boot::pl011_puts_safe(b"DBG: before driver::init()\n");
    driver::init();
    ostd::arch::boot::pl011_puts_safe(b"DBG: driver::init() done\n");
    ostd::arch::boot::pl011_puts_safe(b"DBG: before time::init()\n");
    time::init();
    ostd::arch::boot::pl011_puts_safe(b"DBG: time::init() done\n");
    ostd::arch::boot::pl011_puts_safe(b"DBG: before net::init()\n");
    net::init();
    ostd::arch::boot::pl011_puts_safe(b"DBG: net::init() done\n");
    ostd::arch::boot::pl011_puts_safe(b"DBG: before sched::init()\n");
    sched::init();
    ostd::arch::boot::pl011_puts_safe(b"DBG: sched::init() done\n");
    ostd::arch::boot::pl011_puts_safe(b"DBG: before process::init()\n");
    process::init();
    ostd::arch::boot::pl011_puts_safe(b"DBG: process::init() done\n");
    ostd::arch::boot::pl011_puts_safe(b"DBG: before fs::init()\n");
    fs::init();
    ostd::arch::boot::pl011_puts_safe(b"DBG: fs::init() done\n");
    ostd::arch::boot::pl011_puts_safe(b"DBG: init() complete\n");
}

fn init_on_each_cpu() {
    sched::init_on_each_cpu();
    process::init_on_each_cpu();
    fs::init_on_each_cpu();
}

fn init_in_first_kthread(fs_resolver: &FsResolver) {
    ostd::arch::boot::pl011_puts_safe(b"[ifk] component::init_all(Kthread)\n");
    component::init_all(InitStage::Kthread, component::parse_metadata!()).unwrap();
    ostd::arch::boot::pl011_puts_safe(b"[ifk] after components\n");
    // Work queue should be initialized before interrupt is enabled,
    // in case any irq handler uses work queue as bottom half
    ostd::arch::boot::pl011_puts_safe(b"[ifk] before work_queue\n");
    thread::work_queue::init_in_first_kthread();
    ostd::arch::boot::pl011_puts_safe(b"[ifk] before net\n");
    net::init_in_first_kthread();
    ostd::arch::boot::pl011_puts_safe(b"[ifk] before fs\n");
    fs::init_in_first_kthread(fs_resolver);
    ostd::arch::boot::pl011_puts_safe(b"[ifk] before ipc\n");
    ipc::init_in_first_kthread();
    ostd::arch::boot::pl011_puts_safe(b"[ifk] done\n");
    #[cfg(any(target_arch = "x86_64", target_arch = "riscv64"))]
    vdso::init_in_first_kthread();
}

fn init_in_first_process(ctx: &Context) {
    ostd::console::early_print(format_args!("[ifp] start\n"));
    device::init_in_first_process(ctx).unwrap();
    ostd::console::early_print(format_args!("[ifp] device done\n"));
    fs::init_in_first_process(ctx);
    ostd::console::early_print(format_args!("[ifp] fs done\n"));
    process::init_in_first_process(ctx);
    ostd::console::early_print(format_args!("[ifp] done\n"));
}

fn ap_init() {
    init_on_each_cpu();

    fn ap_idle_thread() {
        log::info!(
            "Kernel idle thread for CPU #{} started.",
            // No races because `ap_idle_thread` runs on a certain AP.
            CpuId::current_racy().as_usize(),
        );

        loop {
            ostd::task::halt_cpu();
        }
    }

    ThreadOptions::new(ap_idle_thread)
        // No races because `ap_init` runs on a certain AP.
        .cpu_affinity(CpuId::current_racy().into())
        .sched_policy(SchedPolicy::Idle)
        .spawn();
}

fn first_kthread() {
    // TODO: After introducing the mount namespace, use an initial mount namespace to create
    // the `FsResolver`, and the initial mount namespace should be passed to the first process.
    ostd::arch::boot::pl011_puts_safe(b"[fk] start\n");
    let fs_resolver = FsResolver::new();
    ostd::arch::boot::pl011_puts_safe(b"[fk] before init_in_first_kthread\n");
    init_in_first_kthread(&fs_resolver);
    ostd::arch::boot::pl011_puts_safe(b"[fk] after init_in_first_kthread\n");

    ostd::arch::boot::pl011_puts_safe(b"[fk] before print_banner\n");
    print_banner();
    ostd::arch::boot::pl011_puts_safe(b"[fk] after print_banner\n");

    ostd::arch::boot::pl011_puts_safe(b"[fk] before karg\n");
    let karg: KCmdlineArg = boot_info().kernel_cmdline.into();
    ostd::arch::boot::pl011_puts_safe(b"[fk] karg parsed\n");
    ostd::arch::boot::pl011_puts_safe(b"[fk] before argv vec\n");
    let argv = karg.get_initproc_argv().to_vec();
    ostd::arch::boot::pl011_puts_safe(b"[fk] argv done\n");
    ostd::arch::boot::pl011_puts_safe(b"[fk] before envp vec\n");
    let envp = karg.get_initproc_envp().to_vec();
    ostd::arch::boot::pl011_puts_safe(b"[fk] envp done\n");
    ostd::arch::boot::pl011_puts_safe(b"[fk] before spawn_init_process\n");
    let initproc = spawn_init_process(
        karg.get_initproc_path().unwrap(),
        argv,
        envp,
    )
    .expect("Run init process failed.");
    ostd::arch::boot::pl011_puts_safe(b"[fk] spawn_init_process done\n");

    // Wait till initproc become zombie.
    while !initproc.status().is_zombie() {
        ostd::task::halt_cpu();
    }

    // TODO: exit via qemu isa debug device should not be the only way.
    let exit_code = if initproc.status().exit_code() == 0 {
        QemuExitCode::Success
    } else {
        QemuExitCode::Failed
    };
    exit_qemu(exit_code);
}

fn print_banner() {
    println!("");
    println!("{}", logo_ascii_art::get_gradient_color_version());
}
