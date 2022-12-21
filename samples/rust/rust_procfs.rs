// SPDX-License-Identifier: GPL-2.0

//! Rust procfs sample.

use kernel::prelude::*;
use kernel::proc::ProcDirEntry;
use kernel::str::CString;

module! {
    type: RustProcFS,
    name: "rust_procfs",
    author: "Rust for Linux Contributors",
    description: "Rust procfs sample",
    license: "GPL",
}

struct RustProcFS {
    root: ProcDirEntry,
}

impl kernel::Module for RustProcFS {
    fn init(name: &'static CStr, _module: &'static ThisModule) -> Result<Self> {
        let root_name = CString::try_from_fmt(fmt!("samples/{}", name))?;
        let root = ProcDirEntry::mkdir(&root_name, None)?;

        Ok(Self { root })
    }
}
