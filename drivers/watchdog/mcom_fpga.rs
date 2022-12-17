// SPDX-License-Identifier: GPL-2.0

use kernel::i2c::{self, DeviceId};
use kernel::prelude::*;

kernel::module_i2c_driver! {
    type: MComFPGA,
    name: "mcom_fpga_wdt",
    author: "Finn Behrens",
    description: "MCOM FPGA I2C Driver",
    license: "GPL",
}
/*alias: [
    "i2c:fpga"
]*/

struct MComFPGA;

//kernel::define_i2c_id_table! {(), [ (DeviceId{"fpga"}, None) ]};

impl i2c::Driver for MComFPGA {
    type Data = ();

    kernel::define_i2c_id_table! {(), [(DeviceId(b"fpga"), None),]}

    fn probe(_: &mut i2c::Client, _: Option<&Self::IdInfo>) -> Result<Self::Data> {
        Ok(())
    }
}

/*impl kernel::Module for MComFPGA {
    fn init(_name: &'static CStr, _module: &'static ThisModule) -> Result<Self> {
        Ok(MComFPGA)
    }
}*/
