// SPDX-License-Identifier: GPL-2.0

//! Proc filesystem.
//!
//! C header: [`include/linux/proc_fs.h`](../../../../include/linux/proc_fs.h)

use crate::{
    bindings,
    error::{code, Result},
    str::CStr,
};

/// A proc filesystem entry.
///
/// # Invariants
///
/// The field `ptr` is valid for the lifetime of the object.
pub struct ProcDirEntry {
    ptr: core::ptr::NonNull<bindings::proc_dir_entry>, //*mut bindings::proc_dir_entry;
}

// SAFETY: `ProcDirEntry` only holds a pointer to a C device, which is safe to be used from any thread.
unsafe impl Send for ProcDirEntry {}

// SAFETY: `ProcDirEntry` only holds a pointer to a C device, references to which are safe to be used
// from any thread.
unsafe impl Sync for ProcDirEntry {}

impl ProcDirEntry {
    /// Create a new directory in procfs.
    pub fn mkdir(name: &CStr, parent: Option<&Self>) -> Result<Self> {
        let parent_ptr = if let Some(parent) = parent {
            parent.ptr.as_ptr()
        } else {
            core::ptr::null_mut()
        };

        // SAFETY: name is valid an non-null
        // SAFETY: parent_ptr is valid
        let ptr = unsafe { bindings::proc_mkdir(name.as_char_ptr(), parent_ptr) };

        Ok(Self {
            ptr: core::ptr::NonNull::new(ptr).ok_or(code::ENOMEM)?,
        })
    }

    /// Create a new directory in procfs with mode.
    pub fn mkdir_mode(
        name: &CStr,
        parent: Option<&Self>,
        mode: crate::types::Mode,
    ) -> Result<Self> {
        let parent_ptr = if let Some(parent) = parent {
            parent.ptr.as_ptr()
        } else {
            core::ptr::null_mut()
        };

        // SAFETY: name is valid an non-null
        // SAFETY: parent_ptr is valid
        let ptr =
            unsafe { bindings::proc_mkdir_mode(name.as_char_ptr(), mode.as_int(), parent_ptr) };

        Ok(Self {
            ptr: core::ptr::NonNull::new(ptr).ok_or(code::ENOMEM)?,
        })
    }
}

impl Drop for ProcDirEntry {
    fn drop(&mut self) {
        // SAFETY: `ptr` is valid by type invariants.
        unsafe {
            bindings::proc_remove(self.ptr.as_ptr());
        }
    }
}
