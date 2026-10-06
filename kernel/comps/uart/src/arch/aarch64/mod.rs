// SPDX-License-Identifier: MPL-2.0

use ostd::arch::boot::DEVICE_TREE;

mod pl011;

pub(super) fn init() {
    let device_tree = DEVICE_TREE.get().unwrap();

    let pl011_node = device_tree
        .find_compatible(&["arm,pl011", "arm,pl011-axi"])
        .or_else(|| {
            device_tree.all_nodes().find(|n| {
                n.compatible()
                    .is_some_and(|c| c.all().any(|s| s.contains("pl011")))
            })
        });

    if let Some(pl011_node) = pl011_node {
        pl011::init(pl011_node);
    } else {
    }
}
