// SPDX-License-Identifier: MPL-2.0

//! Component system
//!

#![no_std]
#![deny(unsafe_code)]

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
use spin::Once;
// This crate intentionally uses the `log` crate directly (not `ostd::log`)
// because it is a standalone framework crate that does not depend on OSTD.
// Messages are forwarded to the OSTD logger via the `LogCrateBridge`.
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
#[derive(Debug, Eq, PartialEq)]
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

// TEMP-HW-DEBUG (revert before MR-1): optional per-component boot marker.
// The kernel sets this before `init_all`; `match_and_call` invokes it with
// each component path so HW serial shows the init running at a panic.
static BOOT_MARKER_HOOK: Once<fn(&str)> = Once::new();

/// Sets the TEMP-HW-DEBUG per-component boot marker. Revert before MR-1.
pub fn set_boot_marker_hook(emit: fn(&str)) {
    let _ = BOOT_MARKER_HOOK.call_once(|| emit);
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
    debug!("All component: {components:?}");
    let mut out = BTreeMap::new();
    for component in components {
        out.insert(component.path.clone(), component);
    }
    out
}

/// Match the ComponentInfo with ComponentRegistry. The key is the relative path of one component
fn match_and_call(
    stage: InitStage,
    mut components: BTreeMap<String, ComponentInfo>,
) -> Result<(), ComponentSystemInitError> {
    let mut infos = Vec::new();
    for registry in inventory::iter::<ComponentRegistry> {
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

    debug!("Remain components: {components:?}");

    if !components.is_empty() {
        info!("Exists components that are not initialized");
    }

    infos.sort();
    debug!("component infos: {infos:?}");
    info!("Components initializing in {stage:?} stage...");

    for info in infos {
        info!("Component initializing: {:?}", info);
        // TEMP-HW-DEBUG (revert before MR-1): the lossy HW serial channel
        // drops log lines, so an optional boot marker reports each init.
        if let Some(emit) = BOOT_MARKER_HOOK.get() {
            emit(&info.path);
        }
        if let Err(res) = (info.function.unwrap())() {
            error!("Component initialize error: {:?}", res);
        } else {
            info!("Component initialize complete");
        }
    }
    info!("All components initialization in {stage:?} stage completed");
    Ok(())
}
