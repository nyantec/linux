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

impl i2c::Driver for MComFPGA {
    type Data = ();

    kernel::define_i2c_id_table! {(), [(DeviceId(b"fpga"), None),]}

    fn probe(client: &mut i2c::Client, _: Option<&Self::IdInfo>) -> Result<Self::Data> {
        dev_info!(client, "probe\n");
        dev_info!(client, "slaveaddr: {}\n", client.get_addr());
        if client.get_addr() != 0x3c {
            dev_info!(client, "probe: not found\n");
            return Err(code::EINVAL);
        }

        // Configure port 0 as output
        let cmd = [0x06, 0x00];
        client.master_send(&buf);

        Ok(())
    }
}
