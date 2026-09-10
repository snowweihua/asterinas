// SPDX-License-Identifier: MPL-2.0

use ostd::arch::boot::DEVICE_TREE;

mod pl011;

pub(super) fn init() {
    let device_tree = DEVICE_TREE.get().unwrap();

    if let Some(pl011_node) = device_tree.find_compatible(&["arm,pl011"]) {
        pl011::init(pl011_node);
    }
}
