// SPDX-License-Identifier: MPL-2.0

//! Component system
//!

#![no_std]
#![expect(unsafe_code)]
#![feature(fn_traits)]

extern crate alloc;

use alloc::{
    borrow::ToOwned,
    collections::BTreeMap,
    fmt::Debug,
    string::{String, ToString},
    vec::Vec,
};

pub use component_macro::*;
pub use inventory::submit;
use log::{debug, error, info};

/// The initialization stages of the component system.
///
/// - `Bootstrap`: The earliest stage, called after OSTD initialization is
///   complete but before kernel subsystem initialization begins. This stage
///   runs on the BSP (Bootstrap Processor) only, before SMP (Symmetric
///   Multi-Processing) is enabled. Components in this stage can initialize
///   core kernel services that other components depend on.
/// - `Kthread`: The kernel thread stage, initialized after SMP is enabled
///   and the first kernel thread is spawned. This stage runs in the context
///   of the first kernel thread on the BSP.
/// - `Process`: The process stage, initialized after the first user process
///   is created. This stage runs in the context of the first user process,
///   and prepares the system for user-space execution.
#[derive(Debug, PartialEq, Eq)]
pub enum InitStage {
    Bootstrap,
    Kthread,
    Process,
}

#[derive(Debug)]
pub enum ComponentInitError {
    UninitializedDependencies(String),
    Unknown,
}

pub struct ComponentRegistry {
    stage: InitStage,
    function: &'static (dyn Fn() -> Result<(), ComponentInitError> + Sync),
    path: &'static str,
}

impl ComponentRegistry {
    pub const fn new(
        stage: InitStage,
        function: &'static (dyn Fn() -> Result<(), ComponentInitError> + Sync),
        path: &'static str,
    ) -> Self {
        Self {
            stage,
            function,
            path,
        }
    }
}

inventory::collect!(ComponentRegistry);

impl Debug for ComponentRegistry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ComponentRegistry")
            .field("stage", &self.stage)
            .field("path", &self.path)
            .finish()
    }
}

pub struct ComponentInfo {
    name: String,
    path: String,
    priority: u32,
    function: Option<&'static (dyn Fn() -> Result<(), ComponentInitError> + Sync)>,
}

impl ComponentInfo {
    pub fn new(name: &str, path: &str, priority: u32) -> Self {
        Self {
            name: name.to_string(),
            path: path.to_string(),
            priority,
            function: None,
        }
    }
}

impl PartialEq for ComponentInfo {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority
    }
}

impl Eq for ComponentInfo {}

impl Ord for ComponentInfo {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.priority.cmp(&other.priority)
    }
}

impl PartialOrd for ComponentInfo {
    fn partial_cmp(&self, other: &ComponentInfo) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Debug for ComponentInfo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ComponentInfo")
            .field("name", &self.name)
            .field("path", &self.path)
            .field("priority", &self.priority)
            .finish()
    }
}

#[derive(Debug)]
pub enum ComponentSystemInitError {
    FileNotValid,
    NotIncludeAllComponent(String),
}

/// Initializes the component system for a specific stage.
///
/// It collects all functions marked with the `init_component` macro, filters them
/// according to the given stage, and invokes them in the correct order while honoring
/// dependencies and priorities between crates.
///
/// The collection of ComponentInfo usually generate by `parse_metadata` macro.
///
/// ```rust
///     component::init_all(component::InitStage::Bootstrap, component::parse_metadata!());
/// ```
///
pub fn init_all(
    stage: InitStage,
    components: Vec<ComponentInfo>,
) -> Result<(), ComponentSystemInitError> {
    let components_info = parse_input(components);
    match_and_call(stage, components_info)?;
    Ok(())
}

fn parse_input(components: Vec<ComponentInfo>) -> BTreeMap<String, ComponentInfo> {
    mini_uart_puts(b"[par] start\n");
    let mut out = BTreeMap::new();
    for component in components {
        out.insert(component.path.clone(), component);
    }
    mini_uart_puts(b"[par] done\n");
    out
}

/// Match the ComponentInfo with ComponentRegistry. The key is the relative path of one component
fn mini_uart_puts(s: &[u8]) {
    for &b in s {
        unsafe {
            core::arch::asm!(
                "movz x4, #0x3F21, lsl #16",
                "movk x4, #0x5054",
                "1: ldrb w5, [x4]",
                "tst w5, #0x20",
                "beq 1b",
                out("x4") _, out("w5") _,
                options(nostack),
            );
            core::arch::asm!(
                "movz x4, #0x3F21, lsl #16",
                "movk x4, #0x5040",
                "strb w3, [x4]",
                in("w3") b as u32,
                out("x4") _,
                options(nostack),
            );
        }
    }
}

fn match_and_call(
    stage: InitStage,
    mut components: BTreeMap<String, ComponentInfo>,
) -> Result<(), ComponentSystemInitError> {
    mini_uart_puts(b"[mac.M] enter\n");
    let mut infos = Vec::new();
    #[cfg(target_arch = "aarch64")]
    let registries = aarch64_component_registries().iter();
    #[cfg(not(target_arch = "aarch64"))]
    let registries = inventory::iter::<ComponentRegistry>.into_iter();
    for registry in registries {
        mini_uart_puts(b"[mac.I] item\n");
        if registry.stage != stage {
            continue;
        }

        // relative/path/to/comps/pci/src/lib.rs
        let mut str: String = registry.path.to_owned();
        str = str.replace('\\', "/");
        // relative/path/to/comps/pci
        // There are two cases, one in the test folder and one in the src folder.
        // There may be multiple directories within the folder.
        // There we assume it will not have such directories: 'comp1/src/comp2/src/lib.rs' so that we can split by tests or src string
        if str.contains("src/") {
            str = str
                .trim_end_matches(str.get(str.find("src/").unwrap()..str.len()).unwrap())
                .to_string();
        } else if str.contains("tests/") {
            str = str
                .trim_end_matches(str.get(str.find("tests/").unwrap()..str.len()).unwrap())
                .to_string();
        } else {
            panic!("The path of {} cannot recognized by component system", str);
        }
        let str = str.trim_end_matches('/').to_owned();

        let mut info = components
            .remove(&str)
            .ok_or(ComponentSystemInitError::NotIncludeAllComponent(str))?;
        info.function.replace(registry.function);
        infos.push(info);
    }

    debug!("Remain components:{components:?}");

    if !components.is_empty() {
        info!("Exists components that are not initialized");
    }
    info!(
        "[mac-1] after for-loop, before sort, infos.len={}",
        infos.len()
    );

    infos.sort();
    mini_uart_puts(b"[mac.S] sort done, len=");
    // Write a crude length marker
    for _ in 0..infos.len() { mini_uart_puts(b"."); }
    mini_uart_puts(b"\n");
    info!("[mac-2] after sort");
    debug!("component infos: {infos:?}");
    info!(
        "[mac-pre] match_and_call: {} components in stage {:?}",
        infos.len(),
        stage
    );
    info!("Components initializing in {stage:?} stage...");
    info!("[mac-post] info printed ok");

    for i in infos {
        mini_uart_puts(b"\n");
        info!("Component initializing:{:?}", i);
        if let Err(res) = i.function.unwrap().call(()) {
            error!("Component initialize error:{:?}", res);
        } else {
            info!("Component initialize complete");
        }
    }
    info!("All components initialization in {stage:?} stage completed");
    Ok(())
}

#[cfg(target_arch = "aarch64")]
fn aarch64_component_registries() -> &'static [ComponentRegistry] {
    unsafe extern "C" {
        static __component_registry_start: u8;
        static __component_registry_end: u8;
    }

    let start = &raw const __component_registry_start as usize;
    let end = &raw const __component_registry_end as usize;
    let len = (end - start) / size_of::<ComponentRegistry>();
    unsafe { core::slice::from_raw_parts(start as *const ComponentRegistry, len) }
}
