// SPDX-License-Identifier: MPL-2.0

use ostd::arch::boot::DEVICE_TREE;

mod pl011;

pub(super) fn init() {
    ostd::arch::serial::marker_str("AE");
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
        ostd::arch::serial::marker_str("F1");
        if let Some(c) = pl011_node.compatible() {
            for s in c.all() {
                if s.contains("pl011") {
                    ostd::arch::serial::marker_str(s);
                }
            }
        }
        pl011::init(pl011_node);
    } else {
        ostd::arch::serial::marker_str("F0");
    }
}
