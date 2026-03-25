// SPDX-License-Identifier: MPL-2.0

use ostd::mm::{io_util::HasVmReaderWriter, Frame, FrameAllocOptions, HasPaddr, UFrame};

use crate::prelude::*;

/// Creates a new `Frame<()>` and initializes it with the contents of the `src`.
///
/// Note that it only duplicates the contents not the metadata.
pub fn duplicate_frame(src: &UFrame) -> Result<Frame<()>> {
    let new_frame = FrameAllocOptions::new().zeroed(false).alloc_frame()?;
    let mut src_reader = src.reader();
    let mut dst_writer = new_frame.writer();
    let copied = dst_writer.write(&mut src_reader);
    Ok(new_frame)
}
