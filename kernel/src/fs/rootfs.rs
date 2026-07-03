// SPDX-License-Identifier: MPL-2.0

use core3::io::{Cursor, Read};
use cpio_decoder::{CpioDecoder, FileType};
use lending_iterator::LendingIterator;
use libflate::gzip::Decoder as GZipDecoder;
use ostd::boot::boot_info;
use spin::Once;

use super::{
    fs_resolver::{FsPath, FsResolver},
    path::Mount,
    ramfs::RamFs,
    utils::{FileSystem, InodeMode, InodeType},
};
use crate::{fs::path::is_dot, prelude::*};

struct BoxedReader<'a>(Box<dyn core3::io::Read + 'a>);

impl<'a> BoxedReader<'a> {
    pub fn new(reader: Box<dyn core3::io::Read + 'a>) -> Self {
        BoxedReader(reader)
    }
}

impl<'a> core3::io::Read for BoxedReader<'a> {
    fn read(&mut self, buf: &mut [u8]) -> core3::io::Result<usize> {
        self.0.read(buf)
    }
}

struct LibflateDecoderAdapter<'a> {
    decoder: libflate::gzip::Decoder<&'a [u8]>,
}

impl<'a> core3::io::Read for LibflateDecoderAdapter<'a> {
    fn read(&mut self, buf: &mut [u8]) -> core3::io::Result<usize> {
        match core2::io::Read::read(&mut self.decoder, buf) {
            Ok(n) => Ok(n),
            Err(e) => Err(core3::io::Error::new(
                match e.kind() {
                    core2::io::ErrorKind::UnexpectedEof => core3::io::ErrorKind::UnexpectedEof,
                    core2::io::ErrorKind::WouldBlock => core3::io::ErrorKind::WouldBlock,
                    _ => core3::io::ErrorKind::Other,
                },
                "libflate error",
            )),
        }
    }
}

/// Unpack and prepare the rootfs from the initramfs CPIO buffer.
pub fn init_in_first_kthread(fs_resolver: &FsResolver) -> Result<()> {
    let initramfs_buf = boot_info().initramfs.expect("No initramfs found!");

    let reader = {
        match &initramfs_buf[..4] {
            &[0x1F, 0x8B, _, _] => {
                let gzip_decoder = GZipDecoder::new(initramfs_buf)
                    .map_err(|_| Error::with_message(Errno::EINVAL, "invalid gzip buffer"))?;
                BoxedReader::new(Box::new(LibflateDecoderAdapter {
                    decoder: gzip_decoder,
                }))
            }
            _ => BoxedReader::new(Box::new(Cursor::new(initramfs_buf))),
        }
    };
    let mut decoder = CpioDecoder::new(reader);

    loop {
        let Some(entry_result) = decoder.next() else {
            break;
        };

        let mut entry = entry_result?;

        // Make sure the name is a relative path, and is not end with "/".
        let entry_name = entry.name().trim_start_matches('/').trim_end_matches('/');
        if entry_name.is_empty() {
            return_errno_with_message!(Errno::EINVAL, "invalid entry name");
        }
        if is_dot(entry_name) {
            continue;
        }

        // Here we assume that the directory referred by "prefix" must has been created.
        // The basis of this assumption is：
        // The mkinitramfs script uses `find` command to ensure that the entries are
        // sorted that a directory always appears before its child directories and files.
        let (parent, name) = if let Some((prefix, last)) = entry_name.rsplit_once('/') {
            (fs_resolver.lookup(&FsPath::try_from(prefix)?)?, last)
        } else {
            (fs_resolver.root().clone(), entry_name)
        };

        let metadata = entry.metadata();
        let mode = InodeMode::from_bits_truncate(metadata.permission_mode());
        match metadata.file_type() {
            FileType::File => {
                let path = parent.new_fs_child(name, InodeType::File, mode)?;
                entry.read_all(path.inode().writer(0))?;
            }
            FileType::Dir => {
                let _ = parent.new_fs_child(name, InodeType::Dir, mode)?;
            }
            FileType::Link => {
                let path = parent.new_fs_child(name, InodeType::SymLink, mode)?;
                let link_content = {
                    let mut link_data: Vec<u8> = Vec::new();
                    entry.read_all(&mut link_data)?;
                    core::str::from_utf8(&link_data)?.to_string()
                };
                path.inode().write_link(&link_content)?;
            }
            type_ => {
                panic!("unsupported file type = {:?} in initramfs", type_);
            }
        }
    }
    // Mount DevFS
    let dev_path = fs_resolver.lookup(&FsPath::try_from("/dev")?)?;
    dev_path.mount(RamFs::new())?;

    Ok(())
}

pub fn mount_fs_at(
    fs: Arc<dyn FileSystem>,
    fs_path: &FsPath,
    fs_resolver: &FsResolver,
) -> Result<()> {
    let target_path = fs_resolver.lookup(fs_path)?;
    target_path.mount(fs)?;
    Ok(())
}

static ROOT_MOUNT: Once<Arc<Mount>> = Once::new();

pub(super) fn init() {
    ROOT_MOUNT.call_once(|| -> Arc<Mount> {
        let rootfs = RamFs::new();
        Mount::new_root(rootfs)
    });
}

pub fn root_mount() -> &'static Arc<Mount> {
    ROOT_MOUNT.get().unwrap()
}
