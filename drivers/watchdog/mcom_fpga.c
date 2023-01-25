// SPDX-License-Identifier: GPL-2.0
/*
 * Siemens MCOM FPGA Watchdog driver
 * 
 * Copyright Finn Behrens 2023
 * Copyright (C) Siemens Mobility GmbH 2021 All Rights Reserved.
 * 
 * Authors:
 *   Finn Behrens <fin@nynatec.com>
 * 
 * TODO: Steve Tucker erwähnen
*/

#include <linux/device.h>
#include <linux/i2c.h>
#include <linux/sysfs.h>

#include <linux/watchdog.h>


#define DRIVER_NAME "mcom_fpga"
#define MAX_CHRDEV 255

#define WD_DIS_MASK 0x7F
#define WD_MODE_MASK 0xF8
#define WD_START_MODE 0x01
#define WD_NORMAL_MODE 0x02
#define WD_DOWN_MODE 0x04

// Watchdog operations
#define WD_STATUS_CONTROLL 0x00
#define WD_DISABLE_UBS 0x12
#define WD_UPTIME 0x20
#define WD_NORMALTIME 0x22
#define WD_DOWNTIME 0x24
#define WD_UBSTIME 0x26
#define WD_PEREPHERIE_RESET 0x28
#define WD_WINDOWTIME 0x2C
#define WD_KICK 0x2E
#define WD_TEMP 0x50
#define WD_MVB_STATUS 0x90
#define WD_MVB_CTRL 0x92

// Module parameters
static int wdt_timeout;
module_param(wdt_timeout, int, 0);
MODULE_PARAM_DESC(wdt_timeout, "Watchdog timeout in seconds.")

struct mcom_fpga_data {
	struct watchdog_device wdd;
};

static int wd_major = 0;
static struct class *chardev_module_class;
static dev_t major_minor_range;
static const int max_minors = MINORMASK;


static int kick_wdt(struct i2c_client *client)
{
	// Set kick bit to low
	int err = i2c_smbus_write_word_data(client, WD_KICK, 0x0000);
	if (err) {
		return err;
	}

	// set kick bit to high
	err = i2c_smbus_write_word_data(client, WD_KICK, 0x0100);
	if (err) {
		return err;
	}

	// Set kick bit to low
	err = i2c_smbus_write_word_data(client, WD_KICK, 0x0000);
	if (err) {
		return err;
	}

	return 0;
}

/* SysFS */
static ssize_t mcom_fpga_store_word(const char *buf, size_t count, u8 reg)
{
	u16 result;
	int err = kstrtou16_from_user(buf, count, 16, &result);
	if (err) {
		return err;
	}

	err = i2c_smbus_write_word_data(mcom_fpga_client, reg, result);
	if (err) {
		return err;
	}

	return count;
}
// status controll
static ssize_t status_controll_show(struct device *dev, struct device_attribute *attr, char *buf)
{
	s32 err = i2c_smbus_read_word_data(mcom_fpga_client, WD_STATUS_CONTROLL);
	if (err < 0) {
		return err;
	}

	return sysfs_emit(buf, "0x%04x\n", err);
}

static ssize_t status_controll_store(struct device *dev, struct device_attribute *attr, const char *buf, size_t count)
{
	return mcom_fpga_store_word(buf, count, WD_STATUS_CONTROLL);
}
DEVICE_ATTR_RW(status_controll);

// Disable UBS
static ssize_t disable_ubs_show(struct device *dev, struct device_attribute *attr, char *buf)
{
	s32 err = i2c_smbus_read_word_data(mcom_fpga_client, WD_DISABLE_UBS);
	if (err < 0) {
		return err;
	}

	return sysfs_emit(buf, "0x%04x\n", err);
}

static ssize_t disable_ubs_store(struct device *dev, struct device_attribute *attr, const char *buf, size_t count)
{
	return mcom_fpga_store_word(buf, count, WD_DISABLE_UBS);
}
DEVICE_ATTR_RW(disable_ubs);

// Uptime
static ssize_t uptime_show(struct device *dev, struct device_attribute *attr, char *buf)
{
	s32 err = i2c_smbus_read_word_data(mcom_fpga_client, WD_UPTIME);
	if (err < 0) {
		return err;
	}

	return sysfs_emit(buf, "0x%04x\n", err);
}

static ssize_t uptime_store(struct device *dev, struct device_attribute *attr, const char *buf, size_t count)
{
	return mcom_fpga_store_word(buf, count, WD_UPTIME);
}
DEVICE_ATTR_RW(uptime);

// Normaltime
static ssize_t normaltime_show(struct device *dev, struct device_attribute *attr, char *buf)
{
	s32 err = i2c_smbus_read_word_data(mcom_fpga_client, WD_NORMALTIME);
	if (err < 0) {
		return err;
	}

	return sysfs_emit(buf, "0x%04x\n", err);
}

static ssize_t normaltime_store(struct device *dev, struct device_attribute *attr, const char *buf, size_t count)
{
	return mcom_fpga_store_word(buf, count, WD_NORMALTIME);
}
DEVICE_ATTR_RW(normaltime);

// Downtime
static ssize_t downtime_show(struct device *dev, struct device_attribute *attr, char *buf)
{
	s32 err = i2c_smbus_read_word_data(mcom_fpga_client, WD_DOWNTIME);
	if (err < 0) {
		return err;
	}

	return sysfs_emit(buf, "0x%04x\n", err);
}

static ssize_t downtime_store(struct device *dev, struct device_attribute *attr, const char *buf, size_t count)
{
	return mcom_fpga_store_word(buf, count, WD_DOWNTIME);
}
DEVICE_ATTR_RW(downtime);

// ubs time
static ssize_t ubstime_show(struct device *dev, struct device_attribute *attr, char *buf)
{
	s32 err = i2c_smbus_read_word_data(mcom_fpga_client, WD_UBSTIME);
	if (err < 0) {
		return err;
	}

	return sysfs_emit(buf, "0x%04x\n", err);
}

static ssize_t ubstime_store(struct device *dev, struct device_attribute *attr, const char *buf, size_t count)
{
	return mcom_fpga_store_word(buf, count, WD_UBSTIME);
}
DEVICE_ATTR_RW(ubstime);


// perepherie_reset
static ssize_t perepherie_reset_store(struct device *dev, struct device_attribute *attr, const char *buf, size_t count)
{
	return mcom_fpga_store_word(buf, count, WD_PEREPHERIE_RESET);
}
DEVICE_ATTR_WO(perepherie_reset);

// windowtime
static ssize_t windowtime_show(struct device *dev, struct device_attribute *attr, char *buf)
{
	s32 err = i2c_smbus_read_word_data(mcom_fpga_client, WD_WINDOWTIME);
	if (err < 0) {
		return err;
	}

	return sysfs_emit(buf, "0x%04x\n", err);
}

static ssize_t windowtime_store(struct device *dev, struct device_attribute *attr, const char *buf, size_t count)
{
	return mcom_fpga_store_word(buf, count, WD_WINDOWTIME);
}
DEVICE_ATTR_RW(windowtime);

// temperature
static ssize_t temperature_show(struct device *dev, struct device_attribute *attr, char *buf)
{
	s32 err = i2c_smbus_read_word_data(mcom_fpga_client, WD_TEMP);
	if (err < 0) {
		return err;
	}

	return sysfs_emit(buf, "0x%04x\n", err);
}
DEVICE_ATTR_RO(temperature);

// mvb status
static ssize_t mvb_status_show(struct device *dev, struct device_attribute *attr, char *buf)
{
	s32 err = i2c_smbus_read_word_data(mcom_fpga_client, WD_MVB_STATUS);
	if (err < 0) {
		return err;
	}

	return sysfs_emit(buf, "0x%04x\n", err);
}
DEVICE_ATTR_RO(mvb_status);

// mvb ctrl
static ssize_t mvb_ctrl_show(struct device *dev, struct device_attribute *attr, char *buf)
{
	s32 err = i2c_smbus_read_word_data(mcom_fpga_client, WD_MVB_CTRL);
	if (err < 0) {
		return err;
	}

	return sysfs_emit(buf, "0x%04x\n", err);
}

static ssize_t mvb_ctrl_store(struct device *dev, struct device_attribute *attr, const char *buf, size_t count)
{
	return mcom_fpga_store_word(buf, count, WD_MVB_CTRL);
}
DEVICE_ATTR_RW(mvb_ctrl);

static struct attribute *mcom_fpga_attrs[] = {
	&dev_attr_status_controll.attr,
	&dev_attr_disable_ubs.attr,
	&dev_attr_uptime.attr,
	&dev_attr_normaltime.attr,
	&dev_attr_downtime.attr,
	&dev_attr_ubstime.attr,
	&dev_attr_perepherie_reset.attr,
	&dev_attr_windowtime.attr,
	&dev_attr_temperature.attr,
	&dev_attr_mvb_status.attr,
	&dev_attr_mvb_ctrl.attr,
	NULL,
};
ATTRIBUTE_GROUPS(mcom_fpga);

// watchdogs ops
static int mcom_fpga_set_mode(struct watchdog_device *wdd, u16 mode)
{
	s32 err;
	struct i2c_client *client = to_i2c_client(wdd->parent);

	err = i2c_smbus_read_word_data(client, WD_STATUS_CONTROLL);
	if (err < 0)
		return err;

	err = (err & WD_DIS_MASK & WD_MODE_MASK) | mode;
	err = i2c_smbus_write_word_data(client, WD_STATUS_CONTROLL, err);
	return err;
}

static int mcom_fpga_start(struct watchdog_device *wdd)
{
	return mcom_fpga_set_mode(wdd, WD_START_MODE);
}

static int mcom_fpga_stop(struct watchdog_device *wdd)
{
	return mcom_fpga_set_mode(wdd, WD_DOWN_MODE);
}

static int mcom_fpga_ping(struct watchdog_device *wdd)
{
	struct i2c_client *client = to_i2c_client(wdd->parent);

	return kick_wdt(client);
}

/*static unsigned int mcom_fpga_get_timeleft(struct watchdog_device *wdd)
{
	struct i2c_client *client = to_i2c_client(wdd->parent);

	return i2c_smbus_read_word_data(client, WD_UPTIME);
)*/

static int mcom_fpga_set_timeout(struct watchdog_device *wdd, unsigned int timeout)
{
	int ret;
	struct i2c_client *client = to_i2c_client(wdd->parent);

	ret = i2c_smbus_write_word_data(client, WD_UPTIME, timeout);
	if (!ret)
		wdd->timeout = timeout;
	
	return ret;
}

static const struct watchdog_info mcom_fpga_info = {
	.identity = "MCOM FPGA Watchdog",
	.options = WDIOF_SETTIMEOUT | WDIOF_KEEPALIVEPING,
}

static const struct watchdog_ops mcom_fpga_ops = {
	.owner = THIS_MODULE,
	.start = mcom_fpga_start,
	.stop = mcom_fpga_stop,
	.ping = mcom_fpga_ping,
	.set_timeout = mcom_fpga_set_timeout,
	//get_timeleft = mcom_fpga_get_timeleft,
};

// I2C functions
static int mcom_fpga_probe(struct i2c_client *client)
{
	struct mcom_fpga_data *data;
	int val;

	if (!i2c_check_functionality(client->adapter, 
					I2C_FUNC_SMBUS_BYTE |
					I2C_FUNC_SMBUS_BYTE_DATA |
					I2C_FUNC_SMBUS_WORD |
					I2C_FUNC_SMBUS_WORD_DATA))
		return -ENODEV;

    if (client->addr != 0x3c) {
		dev_err(&client->dev, "wrong I2C address");
		return -ENODEV;
	}

	data = devm_kzalloc(&client->dev, sizeof(*data), GFP_KERNEL);
	if (!data)
		return -ENOMEM;

	data->wdd.info = &mcom_fpga_wdd_info;
	data->wdd.ops = &mcom_fpga_wdd_ops;
	data->wdd.parent = &client->dev;
	//data->wdd.groups = mcom_fpga_groups;

	watchdog_init_timeout(&data->wdd, wdt_timeout, &client->dev);


	/*
	 * The default value set in the watchdog should be perfectly valid, so
	 * pass that in if we haven't provided one via the module parameter or
	 * of property.
	 */
	if (data->wdd.timeout == 0) {
		val = i2c_smbus_read_word_data(client, WD_UPTIME);
		if (val < 0) {
			dev_err(&client->dev, "Failed to read timeout\n");
			return val;
		}

		data->wdd.timeout = val;
	}

	ret = mcom_fpga_set_timeout(&data->wdd, data->wdd.timeout);
	if (ret) {
		dev_err(&client->dev, "Failed to set timeout\n");
		return ret;
	}

	dev_info(&client->dev, "Watchdog timeout set to %ds\n",
		 data->wdd.timeout);

	i2c_set_clientdata(client, data);



	// set port 0 as output
	ret =  i2c_smbus_write_byte_data(mcom_fpga_client, 0x06, 0x00);
	if (ret) {
		dev_err(&client->dev, "Failed to set port 0 as output\n");
		return ret;
	}

	ret = watchdog_register_device(&data->wdd);
	return ret;
}

static void mcom_fpga_remove(struct i2c_client *client)
{
	struct mcom_fpga_data *data = i2c_get_clientdata(client);

	watchdog_unregister_device(&data->wdd);
}

static const struct i2c_device_id mcom_fpga_i2c_match[] = {
	{ "fpga", 0 },
	{ }
};
MODULE_DEVICE_TABLE(i2c, mcom_fpga_i2c_match);

static struct i2c_driver mcom_fpga_driver = {
	.probe_new	= mcom_fpga_probe,
	.remove		= mcom_fpga_remove,
	.driver		= {
		.name		= DRIVER_NAME,
		.groups = mcom_fpga_groups,
	},
	.id_table = mcom_fpga_i2c_match,
};
module_i2c_driver(mcom_fpga_driver);

MODULE_LICENSE("GPL v2");
MODULE_DESCRIPTION("Siemens MCOM FPGA Watchdog driver");
MODULE_AUTHOR("Finn Behrens <fin@nyantec.com>");