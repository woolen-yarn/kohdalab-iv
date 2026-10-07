/* SPDX-License-Identifier: GPL-2.0-or-later
 * Exercise the actual HS+ extra-init function against mocked USB transfers.
 * Build as C with the platform compatibility shim and libusb dependency.
 */
#define libusb_control_transfer mock_control_transfer
#include "../native/vendor/ni-gpib/linux-gpib/ni_usb_gpib.c"
#include <assert.h>

static int calls;
static int fail_request;

int LIBUSB_CALL mock_control_transfer(libusb_device_handle *handle,
    uint8_t type, uint8_t request, uint16_t value, uint16_t index,
    unsigned char *data, uint16_t length, unsigned int timeout)
{
    assert(handle != NULL);
    assert(timeout == 1000);
    ++calls;
    if (request == NI_USB_HS_PLUS_0x48_REQUEST) {
        assert(type == (USB_DIR_IN | USB_TYPE_VENDOR | USB_RECIP_DEVICE));
        assert(value == 0 && index == 0 && length == 16);
    } else if (request == NI_USB_HS_PLUS_LED_REQUEST) {
        assert(type == (USB_DIR_IN | USB_TYPE_VENDOR | USB_RECIP_DEVICE));
        assert(value == 1 && index == 0 && length == 2);
    } else {
#ifdef _WIN32
        assert(!"Windows must not request the unused analyzer interface");
#else
        assert(request == NI_USB_HS_PLUS_0xf8_REQUEST);
        assert(type == (USB_DIR_IN | USB_TYPE_VENDOR | USB_RECIP_INTERFACE));
        assert(value == 0 && index == 1 && length == 9);
#endif
    }
    if (request == fail_request) return LIBUSB_ERROR_TIMEOUT;
    memset(data, 0, length);
    data[0] = request;
    return length;
}

int main(void)
{
    struct usb_device device = {0};
    struct usb_interface interface = {0};
    ni_usb_private_t private = {0};
    device.handle = (libusb_device_handle *)(uintptr_t)1;
    interface.udev = &device;
    private.bus_interface = &interface;
    mutex_init(&private.control_transfer_lock);
    calls = 0;
    assert(ni_usb_hs_plus_extra_init(&private) >= 0);
#ifdef _WIN32
    assert(calls == 2);
#else
    assert(calls == 3);
#endif
    fail_request = NI_USB_HS_PLUS_0x48_REQUEST;
    calls = 0;
    assert(ni_usb_hs_plus_extra_init(&private) == LIBUSB_ERROR_TIMEOUT);
    assert(calls == 1);
    fail_request = NI_USB_HS_PLUS_LED_REQUEST;
    calls = 0;
    assert(ni_usb_hs_plus_extra_init(&private) == LIBUSB_ERROR_TIMEOUT);
    assert(calls == 2);
    puts("HS+ init regression checks passed: required failures preserved.");
    return 0;
}
