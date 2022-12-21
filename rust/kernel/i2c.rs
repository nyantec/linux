// SPDX-License-Identifier: GPL-2.0

//! I2C devices and drivers.
//!
//! C header: [`include/linux/i2c.h`](../../../../include/linux/i2c.h)

use crate::{
    bindings,
    device::RawDevice,
    driver,
    error::{code, from_kernel_result, Result},
    str::{BStr, CStr},
    to_result,
    types::PointerWrapper,
    ThisModule,
};

/// An I2C device id.
#[derive(Clone, Copy)]
pub struct DeviceId(pub &'static BStr);

// SAFETY: `ZERO` is all zeroed-out and `to_rawid` stores `offset` in `i2c_device_id::driver_data`.
unsafe impl const crate::driver::RawDeviceId for DeviceId {
    type RawType = bindings::i2c_device_id;
    const ZERO: Self::RawType = bindings::i2c_device_id {
        name: [0; 20],
        driver_data: 0,
    };

    fn to_rawid(&self, offset: isize) -> Self::RawType {
        let mut id = Self::ZERO;
        let mut i = 0;
        while i < self.0.len() {
            id.name[i] = self.0[i] as _;
            i += 1;
        }
        id.name[i] = b'\0' as _;
        id.driver_data = offset as _;
        id
    }
}

/// Defines a const I2C device id table that also carries per-entry data/context/info.
///
/// The name of the const is `I2C_DEVICE_ID_TABLE`.
///
/// # Examples
///
/// ```
/// use kernel::i2c;
///
/// define_i2c_id_table! {u32, [
///     (i2c::DeviceId(b"test-device1"), Some(0xff)),
///     (i2c::DeviceId(b"test-device2"), None),
/// ]};
/// ```
#[macro_export]
macro_rules! define_i2c_id_table {
    ($data_type:ty, $($t:tt)*) => {
        $crate::define_id_table!(I2C_DEVICE_ID_TABLE, $crate::i2c::DeviceId, $data_type, $($t)*);
    }
}

/// An adapter for the registration of i2c drivers.
pub struct Adapter<T: Driver>(T);

impl<T: Driver> driver::DriverOps for Adapter<T> {
    type RegType = bindings::i2c_driver;

    unsafe fn register(
        reg: *mut Self::RegType,
        name: &'static CStr,
        module: &'static ThisModule,
    ) -> Result {
        // SAFETY: By the safety requirements of this function (defined in the trait definition),
        // `reg` is non-null and valid.
        let i2cdrv = unsafe { &mut *reg };

        i2cdrv.driver.name = name.as_char_ptr();
        i2cdrv.probe_new = Some(Self::probe_callback);
        i2cdrv.remove = Some(Self::remove_callback);
        if let Some(t) = T::I2C_DEVICE_ID_TABLE {
            i2cdrv.id_table = t.as_ref();
        }

        // SAFETY:
        //   - `pdrv` lives at least until the call to `platform_driver_unregister()` returns.
        //   - `name` pointer has static lifetime.
        //   - `module.0` lives at least as long as the module.
        //   - `probe()` and `remove()` are static functions.
        //   - `of_match_table` is either a raw pointer with static lifetime,
        //      as guaranteed by the [`driver::IdTable`] type, or null.
        to_result(unsafe { bindings::i2c_register_driver(module.0, reg) })
    }

    unsafe fn unregister(reg: *mut Self::RegType) {
        // SAFETY: By the safety requirements of this function (defined in the trait definition),
        // `reg` was passed (and updated) by a previous successful call to
        // `i2c_register_driver`.
        unsafe { bindings::i2c_del_driver(reg) };
    }
}

impl<T: Driver> Adapter<T> {
    fn get_id_info(client: &Client) -> Option<&'static T::IdInfo> {
        let table = T::I2C_DEVICE_ID_TABLE?;

        let id = unsafe { bindings::i2c_match_id(table.as_ref(), client.ptr) };
        if id.is_null() {
            return None;
        }

        // SAFETY: `id` is a pointer within the static table, so it's always valid.
        let offset = unsafe { (*id).driver_data };
        if offset == 0 {
            return None;
        }

        // SAFETY: The offset comes from a previous call to `offset_from` in `IdArray::new`, which
        // guarantees that the resulting pointer is within the table.
        let ptr = unsafe {
            id.cast::<u8>()
                .offset(offset as _)
                .cast::<Option<T::IdInfo>>()
        };

        // SAFETY: The id table has a static lifetime, so `ptr` is guaranteed to be valid for read.
        unsafe { (&*ptr).as_ref() }
    }

    extern "C" fn probe_callback(i2c: *mut bindings::i2c_client) -> core::ffi::c_int {
        from_kernel_result! {
            let mut client = unsafe { Client::from_ptr(i2c) };
            let info = Self::get_id_info(&client);
            let data = T::probe(&mut client, info)?;

            // SAFETY: `i2c` is guaranteed to be a valid, non-null pointer.
            unsafe { bindings::i2c_set_clientdata(i2c, data.into_pointer() as _) };
            Ok(0)
        }
    }

    extern "C" fn remove_callback(i2c: *mut bindings::i2c_client) {
        // SAFETY: `i2c` is guarenteed to be a valid, non-null pointer
        let ptr = unsafe { bindings::i2c_get_clientdata(i2c) };
        // SAFETY:
        //   - we allocated this pointer using `T::Data::into_pointer`,
        //     so it is safe to turn back into a `T::Data`.
        //   - the allocation happened in `probe`, no-one freed the memory,
        //     `remove` is the canonical kernel location to free driver data. so OK
        //     to convert the pointer back to a Rust structure here.
        let data = unsafe { T::Data::from_pointer(ptr) };
        T::remove(&data);
        <T::Data as driver::DeviceRemoval>::device_remove(&data);
    }
}

/// A I2C driver.
pub trait Driver {
    /// Data stored on device by driver.
    ///
    /// Corresponds to the data set or retrieved via the kernel's
    /// `i2c_{set,get}_clientdata()` functions.
    ///
    /// Require that `Data` implements `PointerWrapper`. We guarantee to
    /// never move the underlying wrapped data structure. This allows
    type Data: PointerWrapper + Send + Sync + driver::DeviceRemoval = ();

    /// The type holding information about each device id supported by the driver.
    type IdInfo: 'static = ();

    /// The table of device ids supported by the driver.
    const I2C_DEVICE_ID_TABLE: Option<driver::IdTable<'static, DeviceId, Self::IdInfo>> = None;

    /// I2C driver probe.
    ///
    /// Called when a new i2c client is added or discovered.
    /// Implementers should attempt to initialize the client here.
    fn probe(client: &mut Client, id_info: Option<&Self::IdInfo>) -> Result<Self::Data>;

    /// I2C driver remove.
    ///
    /// Called when an i2c client is removed.
    fn remove(_data: &Self::Data) {}
}

/// A I2C Client device.
///
/// # Invariants
///
/// The field `ptr` is non-null and valid for the lifetime of the object.
pub struct Client {
    ptr: *mut bindings::i2c_client,
}

impl Client {
    /// Creates a new client from the given pointer.
    ///
    /// # Safety
    ///
    /// `ptr` must be non-null and valid. It must remain valid for the lifetime of the returned
    /// instance.
    unsafe fn from_ptr(ptr: *mut bindings::i2c_client) -> Self {
        // INVARIANT: The safety requirements of the function ensure the lifetime invariant.
        Self { ptr }
    }

    /// Get Chip address.
    pub fn get_addr(&self) -> u16 {
        // SAFETY: By the type invariants, we know that `self.ptr` is non-null and valid.
        unsafe { (*self.ptr).addr }
    }

    /// Send data to I2C client.
    ///
    /// The master routines are the ones normally used to transmit data to devices
    /// on a bus (or read from them). Apart from two basic transfer functions to
    /// transmit one message at a time, a more complex version can be used to
    /// transmit an arbitrary number of messages without interruption.
    ///
    /// Buf must be smaller than 64k.
    pub fn transfer_buffer_flags(&self, buf: &mut [u8], flags: u16) -> Result<usize> {
        // SAFETY: buf is valid
        unsafe { self.transfer_buffer_flags_ptr(buf.as_mut_ptr(), buf.len(), flags) }
    }

    unsafe fn transfer_buffer_flags_ptr(
        &self,
        buf: *mut u8,
        count: usize,
        flags: u16,
    ) -> Result<usize> {
        if count > u16::MAX as usize || count == 0 {
            return Err(code::EINVAL);
        }

        // SAFETY: By the type invariants, we know that `self.ptr` is non-null and valid.
        // SAFETY: `buf` is valid and non-null.
        let count =
            unsafe { bindings::i2c_transfer_buffer_flags(self.ptr, buf as _, count as _, flags) };
        to_result(count)?;
        Ok(count as _)
    }

    /// Issue a single I2C message in master transmit mode.
    pub fn master_send(&mut self, buf: &[u8]) -> Result<usize> {
        // SAFETY: buf is valid
        // SAFETY: buf is only read. transfer will not write, when flag RD is not set.
        unsafe { self.transfer_buffer_flags_ptr(buf.as_ptr() as _, buf.len(), 0) }
    }

    /// Issue a single I2C message in master receive mode.
    pub fn master_recv(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.transfer_buffer_flags(buf, msg_flags::RD)
    }
}

// SAFETY: The device returned by `raw_device` is the raw i2c device.
unsafe impl RawDevice for Client {
    fn raw_device(&self) -> *mut bindings::device {
        // SAFETY: By the type invariants, we know that `self.ptr` is non-null and valid.
        unsafe { &mut (*self.ptr).dev }
    }
}

/// Declares a kernel module that exposes a single i2c driver.
///
/// # Examples
///
/// ```ignore
/// # use kernel::{i2c, define_i2c_id_table, module_i2c_driver};
/// struct MyDriver;
/// impl i2c::Driver for MyDriver {
///     // [...]
/// #   fn probe(_client: &mut i2c::Client, _id_info: Option<&Self::IdInfo>) -> Result {
/// #       Ok(())
/// #   }
/// #   define_i2c_id_table! {(), [
/// #       (i2c::DeviceId(b"fpga"), None);
/// #   ]}
/// }
///
/// module_i2c_driver! {
///     type: MyDriver,
///     name: "module_name",
///     author: "Author name",
///     license: "GPL",
/// }
/// ```
#[macro_export]
macro_rules! module_i2c_driver {
    ($($f:tt)*) => {
        $crate::module_driver!(<T>, $crate::i2c::Adapter<T>, { $($f)* });
    };
}

/// I2C Message flags.
pub mod msg_flags {
    pub const RD: u16 = bindings::I2C_M_RD as _;

    pub const TEN: u16 = bindings::I2C_M_TEN as _;

    pub const DMA_SAFE: u16 = bindings::I2C_M_DMA_SAFE as _;

    pub const RECV_LEN: u16 = bindings::I2C_M_RECV_LEN as _;

    pub const NO_RD_ACK: u16 = bindings::I2C_M_NO_RD_ACK as _;

    pub const IGNORE_NAK: u16 = bindings::I2C_M_IGNORE_NAK as _;

    pub const REV_DIR_ADDR: u16 = bindings::I2C_M_REV_DIR_ADDR as _;

    pub const NOSTART: u16 = bindings::I2C_M_NOSTART as _;

    pub const STOP: u16 = bindings::I2C_M_STOP as _;
}
