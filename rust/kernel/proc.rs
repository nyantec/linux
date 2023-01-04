// SPDX-License-Identifier: GPL-2.0

//! Proc filesystem.
//!
//! C header: [`include/linux/proc_fs.h`](../../../../include/linux/proc_fs.h)

use alloc::boxed::Box;
use core::{marker, mem, ptr};

use crate::{
    bindings,
    error::{code, from_kernel_result, Result},
    file::{File, IoctlCommand, OpenAdapter, PollTable, SeekFrom},
    io_buffer::{IoBufferReader, IoBufferWriter},
    iov_iter::IovIter,
    mm,
    str::CStr,
    types::{Mode, PointerWrapper},
    user_ptr::UserSlicePtr,
};
use macros::vtable;

/// A proc filesystem entry.
///
/// # Invariants
///
/// The field `ptr` is valid for the lifetime of the object.
pub struct ProcDirEntry<T = ()> {
    ptr: core::ptr::NonNull<bindings::proc_dir_entry>,
    marker: marker::PhantomData<T>,
}

// SAFETY: `ProcDirEntry` only holds a pointer to a C device,
// which is safe to be used from any thread.
unsafe impl<T: Send> Send for ProcDirEntry<T> {}

// SAFETY: `ProcDirEntry` only holds a pointer to a C device,
// references to which are safe to be used from any thread.
unsafe impl<T: Sync> Sync for ProcDirEntry<T> {}

impl ProcDirEntry {
    /// Get a pointer to the parent proc dir entry or null.
    fn parent_ptr(parent: Option<&ProcDirEntry>) -> *mut bindings::proc_dir_entry {
        if let Some(parent) = parent {
            parent.ptr.as_ptr()
        } else {
            ptr::null_mut()
        }
    }

    /// Create a new directory in procfs.
    pub fn mkdir(name: &CStr, parent: Option<&ProcDirEntry>) -> Result<Self> {
        Self::mkdir_mode(name, parent, Mode::from_int(0))
    }

    /// Create a new directory in procfs with mode.
    pub fn mkdir_mode(name: &CStr, parent: Option<&ProcDirEntry>, mode: Mode) -> Result<Self> {
        let parent_ptr = ProcDirEntry::parent_ptr(parent);

        // SAFETY: name is valid an non-null
        // SAFETY: parent_ptr is valid
        let ptr =
            unsafe { bindings::proc_mkdir_mode(name.as_char_ptr(), mode.as_int(), parent_ptr) };

        Ok(Self {
            ptr: core::ptr::NonNull::new(ptr).ok_or(code::ENOMEM)?,
            marker: marker::PhantomData,
        })
    }
}

impl<T: Operations> ProcDirEntry<T> {
    /// Generate a new proc file entry with given data.
    pub fn new(
        name: &CStr,
        mode: Mode,
        parent: Option<&ProcDirEntry>,
        data: Box<T::OpenData>,
    ) -> Result<Self> {
        // SAFETY: the adapter is compatible with ProcDirEntry
        let proc_ops = unsafe { OperationsVtable::<Self, T>::build() };

        let parent_ptr = ProcDirEntry::parent_ptr(parent);

        // SAFETY: name is valid an non-null
        // SAFETY: parent_ptr is valid
        // SAFETY: proc_ops is valid
        let ptr = unsafe {
            bindings::proc_create_data(
                name.as_char_ptr(),
                mode.as_int(),
                parent_ptr,
                proc_ops,
                Box::into_raw(data) as _,
            )
        };

        Ok(Self {
            ptr: core::ptr::NonNull::new(ptr).ok_or(code::ENOMEM)?,
            marker: marker::PhantomData,
        })
    }
}

impl<T: Operations> OpenAdapter<T::OpenData> for ProcDirEntry<T> {
    unsafe fn convert(
        _inode: *mut bindings::inode,
        file: *mut bindings::file,
    ) -> *const T::OpenData {
        (unsafe { (*file).private_data }) as _
    }
}

impl<T> Drop for ProcDirEntry<T> {
    fn drop(&mut self) {
        // SAFETY: `ptr` is valid by type invariants.
        unsafe {
            bindings::proc_remove(self.ptr.as_ptr());
        }
    }
}

pub(crate) struct OperationsVtable<A, T>(marker::PhantomData<A>, marker::PhantomData<T>);

impl<A: OpenAdapter<T::OpenData>, T: Operations> OperationsVtable<A, T> {
    /// Calls `T::open` on the returned value of `A::convert`.
    ///
    /// # Safety
    ///
    /// The returned value of `A::convert` must be a valid non-null pointer and
    /// `T:open` must return a valid non-null pointer on an `Ok` result.
    unsafe extern "C" fn open_callback(
        inode: *mut bindings::inode,
        file: *mut bindings::file,
    ) -> core::ffi::c_int {
        from_kernel_result! {
            // SAFETY: `A::convert` must return a valid non-null pointer that
            // should point to data in the inode or file that lives longer
            // than the following use of `T::open`.
            let arg = unsafe { A::convert(inode, file) };
            // SAFETY: The C contract guarantees that `file` is valid. Additionally,
            // `fileref` never outlives this function, so it is guaranteed to be
            // valid.
            let fileref = unsafe { File::from_ptr(file) };
            // SAFETY: `arg` was previously returned by `A::convert` and must
            // be a valid non-null pointer.
            let ptr = T::open(unsafe { &*arg }, fileref)?.into_pointer();
            // SAFETY: The C contract guarantees that `private_data` is available
            // for implementers of the file operations (no other C code accesses
            // it), so we know that there are no concurrent threads/CPUs accessing
            // it (it's not visible to any other Rust code).
            unsafe { (*file).private_data = ptr as *mut core::ffi::c_void };
            Ok(0)
        }
    }

    unsafe extern "C" fn read_callback(
        file: *mut bindings::file,
        buf: *mut core::ffi::c_char,
        len: core::ffi::c_size_t,
        offset: *mut bindings::loff_t,
    ) -> core::ffi::c_ssize_t {
        from_kernel_result! {
            let mut data =
                unsafe { UserSlicePtr::new(buf as *mut core::ffi::c_void, len).writer() };
            // SAFETY: `private_data` was initialised by `open_callback` with a value returned by
            // `T::Data::into_pointer`. `T::Data::from_pointer` is only called by the
            // `release` callback, which the C API guarantees that will be called only when all
            // references to `file` have been released, so we know it can't be called while this
            // function is running.
            let f = unsafe { T::Data::borrow((*file).private_data) };
            // No `FMODE_UNSIGNED_OFFSET` support, so `offset` must be in [0, 2^63).
            // See <https://github.com/fishinabarrel/linux-kernel-module-rust/pull/113>.
            let read = T::read(
                f,
                unsafe { File::from_ptr(file) },
                &mut data,
                unsafe { *offset }.try_into()?,
            )?;
            unsafe { (*offset) += bindings::loff_t::try_from(read).unwrap() };
            Ok(read as _)
        }
    }

    unsafe extern "C" fn read_iter_callback(
        iocb: *mut bindings::kiocb,
        raw_iter: *mut bindings::iov_iter,
    ) -> isize {
        from_kernel_result! {
            let mut iter = unsafe { IovIter::from_ptr(raw_iter) };
            let file = unsafe { (*iocb).ki_filp };
            let offset = unsafe { (*iocb).ki_pos };
            // SAFETY: `private_data` was initialised by `open_callback` with a value returned by
            // `T::Data::into_pointer`. `T::Data::from_pointer` is only called by the
            // `release` callback, which the C API guarantees that will be called only when all
            // references to `file` have been released, so we know it can't be called while this
            // function is running.
            let f = unsafe { T::Data::borrow((*file).private_data) };
            let read = T::read(
                f,
                unsafe { File::from_ptr(file) },
                &mut iter,
                offset.try_into()?,
            )?;
            unsafe { (*iocb).ki_pos += bindings::loff_t::try_from(read).unwrap() };
            Ok(read as _)
        }
    }

    unsafe extern "C" fn write_callback(
        file: *mut bindings::file,
        buf: *const core::ffi::c_char,
        len: core::ffi::c_size_t,
        offset: *mut bindings::loff_t,
    ) -> core::ffi::c_ssize_t {
        from_kernel_result! {
            let mut data =
                unsafe { UserSlicePtr::new(buf as *mut core::ffi::c_void, len).reader() };
            // SAFETY: `private_data` was initialised by `open_callback` with a value returned by
            // `T::Data::into_pointer`. `T::Data::from_pointer` is only called by the
            // `release` callback, which the C API guarantees that will be called only when all
            // references to `file` have been released, so we know it can't be called while this
            // function is running.
            let f = unsafe { T::Data::borrow((*file).private_data) };
            // No `FMODE_UNSIGNED_OFFSET` support, so `offset` must be in [0, 2^63).
            // See <https://github.com/fishinabarrel/linux-kernel-module-rust/pull/113>.
            let written = T::write(
                f,
                unsafe { File::from_ptr(file) },
                &mut data,
                unsafe { *offset }.try_into()?,
            )?;
            unsafe { (*offset) += bindings::loff_t::try_from(written).unwrap() };
            Ok(written as _)
        }
    }

    unsafe extern "C" fn release_callback(
        _inode: *mut bindings::inode,
        file: *mut bindings::file,
    ) -> core::ffi::c_int {
        let ptr = mem::replace(unsafe { &mut (*file).private_data }, ptr::null_mut());
        T::release(unsafe { T::Data::from_pointer(ptr as _) }, unsafe {
            File::from_ptr(file)
        });
        0
    }

    unsafe extern "C" fn lseek_callback(
        file: *mut bindings::file,
        offset: bindings::loff_t,
        whence: core::ffi::c_int,
    ) -> bindings::loff_t {
        from_kernel_result! {
            let off = match whence as u32 {
                bindings::SEEK_SET => SeekFrom::Start(offset.try_into()?),
                bindings::SEEK_CUR => SeekFrom::Current(offset),
                bindings::SEEK_END => SeekFrom::End(offset),
                _ => return Err(code::EINVAL),
            };
            // SAFETY: `private_data` was initialised by `open_callback` with a value returned by
            // `T::Data::into_pointer`. `T::Data::from_pointer` is only called by the
            // `release` callback, which the C API guarantees that will be called only when all
            // references to `file` have been released, so we know it can't be called while this
            // function is running.
            let f = unsafe { T::Data::borrow((*file).private_data) };
            let off = T::seek(f, unsafe { File::from_ptr(file) }, off)?;
            Ok(off as bindings::loff_t)
        }
    }

    unsafe extern "C" fn ioctl_callback(
        file: *mut bindings::file,
        cmd: core::ffi::c_uint,
        arg: core::ffi::c_ulong,
    ) -> core::ffi::c_long {
        from_kernel_result! {
            // SAFETY: `private_data` was initialised by `open_callback` with a value returned by
            // `T::Data::into_pointer`. `T::Data::from_pointer` is only called by the
            // `release` callback, which the C API guarantees that will be called only when all
            // references to `file` have been released, so we know it can't be called while this
            // function is running.
            let f = unsafe { T::Data::borrow((*file).private_data) };
            let mut cmd = IoctlCommand::new(cmd as _, arg as _);
            let ret = T::ioctl(f, unsafe { File::from_ptr(file) }, &mut cmd)?;
            Ok(ret as _)
        }
    }

    #[cfg(CONFIG_COMPAT)]
    unsafe extern "C" fn compat_ioctl_callback(
        file: *mut bindings::file,
        cmd: core::ffi::c_uint,
        arg: core::ffi::c_ulong,
    ) -> core::ffi::c_long {
        from_kernel_result! {
            // SAFETY: `private_data` was initialised by `open_callback` with a value returned by
            // `T::Data::into_pointer`. `T::Data::from_pointer` is only called by the
            // `release` callback, which the C API guarantees that will be called only when all
            // references to `file` have been released, so we know it can't be called while this
            // function is running.
            let f = unsafe { T::Data::borrow((*file).private_data) };
            let mut cmd = IoctlCommand::new(cmd as _, arg as _);
            let ret = T::compat_ioctl(f, unsafe { File::from_ptr(file) }, &mut cmd)?;
            Ok(ret as _)
        }
    }

    unsafe extern "C" fn mmap_callback(
        file: *mut bindings::file,
        vma: *mut bindings::vm_area_struct,
    ) -> core::ffi::c_int {
        from_kernel_result! {
            // SAFETY: `private_data` was initialised by `open_callback` with a value returned by
            // `T::Data::into_pointer`. `T::Data::from_pointer` is only called by the
            // `release` callback, which the C API guarantees that will be called only when all
            // references to `file` have been released, so we know it can't be called while this
            // function is running.
            let f = unsafe { T::Data::borrow((*file).private_data) };

            // SAFETY: The C API guarantees that `vma` is valid for the duration of this call.
            // `area` only lives within this call, so it is guaranteed to be valid.
            let mut area = unsafe { mm::virt::Area::from_ptr(vma) };

            // SAFETY: The C API guarantees that `file` is valid for the duration of this call,
            // which is longer than the lifetime of the file reference.
            T::mmap(f, unsafe { File::from_ptr(file) }, &mut area)?;
            Ok(0)
        }
    }

    unsafe extern "C" fn poll_callback(
        file: *mut bindings::file,
        wait: *mut bindings::poll_table_struct,
    ) -> bindings::__poll_t {
        // SAFETY: `private_data` was initialised by `open_callback` with a value returned by
        // `T::Data::into_pointer`. `T::Data::from_pointer` is only called by the `release`
        // callback, which the C API guarantees that will be called only when all references to
        // `file` have been released, so we know it can't be called while this function is running.
        let f = unsafe { T::Data::borrow((*file).private_data) };
        match T::poll(f, unsafe { File::from_ptr(file) }, unsafe {
            &PollTable::from_ptr(wait)
        }) {
            Ok(v) => v,
            Err(_) => bindings::POLLERR,
        }
    }

    const VTABLE: bindings::proc_ops = bindings::proc_ops {
        proc_flags: 0, // FIXME: real value
        proc_open: Some(Self::open_callback),
        proc_release: Some(Self::release_callback),
        proc_read: if T::HAS_READ {
            Some(Self::read_callback)
        } else {
            None
        },
        proc_read_iter: if T::HAS_READ {
            Some(Self::read_iter_callback)
        } else {
            None
        },
        proc_write: if T::HAS_WRITE {
            Some(Self::write_callback)
        } else {
            None
        },
        proc_lseek: if T::HAS_SEEK {
            Some(Self::lseek_callback)
        } else {
            None
        },
        proc_poll: if T::HAS_POLL {
            Some(Self::poll_callback)
        } else {
            None
        },
        proc_ioctl: if T::HAS_IOCTL {
            Some(Self::ioctl_callback)
        } else {
            None
        },
        #[cfg(CONFIG_COMPAT)]
        proc_compat_ioctl: if T::HAS_COMPAT_IOCTL {
            Some(Self::compat_ioctl_callback)
        } else {
            None
        },
        proc_mmap: if T::HAS_MMAP {
            Some(Self::mmap_callback)
        } else {
            None
        },
        proc_get_unmapped_area: None,
    };

    /// Builds an instance of [`struct proc_ops`].
    ///
    /// # Safety
    ///
    /// The caller must ensure that the adapter is compatible with the way the device is registered.
    pub(crate) const unsafe fn build() -> &'static bindings::proc_ops {
        &Self::VTABLE
    }
}

/// Corresponds to kernel's `struct proc_ops`.
///
/// You implement this trait whenever you would create a `struct proc_ops`.
#[vtable]
pub trait Operations {
    /// The type of the context data returned by [`Operations::open`] and made available to
    /// other methods.
    type Data: PointerWrapper + Send + Sync = ();

    /// The type of the context data passed to [`Operations::open`].
    type OpenData: Sync = ();

    /// Creates a new instance of this proc file.
    ///
    /// Corresponds to the `proc_open` function pointer in `struct proc_ops`.
    fn open(context: &Self::OpenData, file: &File) -> Result<Self::Data>;

    /// Cleans up after the last reference to the proc file goes away.
    ///
    /// Note that context data is moved, so it will be freed automatically unless the
    /// implementation moves it elsewhere.
    ///
    /// Corresponds to the `proc_release` function pointer in `struct proc_ops`.
    fn release(_data: Self::Data, _file: &File) {}

    /// Reads data frm this proc file to the caller's buffer.
    ///
    /// Corresponds to the `proc_read` and `proc_read_iter` function pointers in `struct proc_ops`.
    fn read(
        _data: <Self::Data as PointerWrapper>::Borrowed<'_>,
        _file: &File,
        _writer: &mut impl IoBufferWriter,
        _offset: u64,
    ) -> Result<usize> {
        Err(code::EINVAL)
    }

    /// Writes data from the caller's buffer to this proc file.
    ///
    /// Corresponds to the `proc_write` function pointers in `struct proc_ops`.
    fn write(
        _data: <Self::Data as PointerWrapper>::Borrowed<'_>,
        _file: &File,
        _reader: &mut impl IoBufferReader,
        _offset: u64,
    ) -> Result<usize> {
        Err(code::EINVAL)
    }

    /// Changes the position of the proc file.
    ///
    /// Corresponds to the `proc_lseek` function pointer in `struct file_operations`.
    fn seek(
        _data: <Self::Data as PointerWrapper>::Borrowed<'_>,
        _file: &File,
        _offset: SeekFrom,
    ) -> Result<u64> {
        Err(code::EINVAL)
    }

    /// Performs IO control operations that are specific to the proc file.
    ///
    /// Corresponds to the `proc_ioctl` function pointer in `struct proc_ops`.
    fn ioctl(
        _data: <Self::Data as PointerWrapper>::Borrowed<'_>,
        _file: &File,
        _cmd: &mut IoctlCommand,
    ) -> Result<i32> {
        Err(code::ENOTTY)
    }

    /// Performs 32-bit IO control operations on that are specific to the proc file on
    /// 64-bit kernels.
    ///
    /// Corresponds to the `proc_compat_ioctl` function pointer in `struct proc_ops`.
    #[cfg(any(CONFIG_COMPAT, doc))]
    #[doc(cfg(CONFIG_COMPAT))]
    fn compat_ioctl(
        _data: <Self::Data as PointerWrapper>::Borrowed<'_>,
        _file: &File,
        _cmd: &mut IoctlCommand,
    ) -> Result<i32> {
        Err(code::ENOTTY)
    }

    /// Maps areas of the caller's virtual memory with device/file memory.
    ///
    /// Corresponds to the `proc_mmap` function pointer in `struct proc_ops`.
    fn mmap(
        _data: <Self::Data as PointerWrapper>::Borrowed<'_>,
        _file: &File,
        _vma: &mut mm::virt::Area,
    ) -> Result {
        Err(code::EINVAL)
    }

    /// Checks the state of the file and optionally registers for notification when the state
    /// changes.
    ///
    /// Corresponds to the `proc_poll` function pointer in `struct proc_ops`.
    fn poll(
        _data: <Self::Data as PointerWrapper>::Borrowed<'_>,
        _file: &File,
        _table: &PollTable,
    ) -> Result<u32> {
        Ok(bindings::POLLIN | bindings::POLLOUT | bindings::POLLRDNORM | bindings::POLLWRNORM)
    }
}
