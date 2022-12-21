// SPDX-License-Identifier: GPL-2.0

//! Rust procfs sample.

use kernel::file::{self, File};
use kernel::io_buffer::{IoBufferReader, IoBufferWriter};
use kernel::prelude::*;
use kernel::proc::{self, ProcDirEntry};
use kernel::str::CString;
use kernel::Mode;

module! {
    type: RustProcFS,
    name: "rust_procfs",
    author: "Rust for Linux Contributors",
    description: "Rust procfs sample",
    license: "GPL",
}

struct RustProcFS {
    root: ProcDirEntry,
    file: ProcDirEntry<Test>,
}

impl kernel::Module for RustProcFS {
    fn init(name: &'static CStr, _module: &'static ThisModule) -> Result<Self> {
        let root_name = CString::try_from_fmt(fmt!("samples/{}", name))?;
        let root = ProcDirEntry::mkdir(&root_name, None)?;

        let file_name = CString::try_from_fmt(fmt!("test"))?;
        let file = ProcDirEntry::new(
            &file_name,
            Mode::from_int(0),
            Some(&root),
            Box::try_new(())?,
        )?;

        Ok(Self { root, file })
    }
}

pub struct Test;
#[vtable]
impl proc::Operations for Test {
    fn open(shared: &(), _file: &File) -> Result<()> {
        Ok(())
    }

    fn read(shared: (), _: &File, data: &mut impl IoBufferWriter, offset: u64) -> Result<usize> {
        if data.is_empty() || offset != 0 {
            return Ok(0);
        }

        data.write_slice(&[1u8; 1])?;
        Ok(1)
    }
}
