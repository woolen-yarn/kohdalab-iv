// SPDX-License-Identifier: MIT
// In-process fake USB device for testing the real native helper without hardware.
#include <libusb.h>
#include <algorithm>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

struct libusb_context {};
struct libusb_device {};
struct libusb_device_handle {};
static libusb_device device;
static libusb_device *devices[] = {&device, nullptr};
static libusb_endpoint_descriptor endpoints[2]{};
static libusb_interface_descriptor alternate{};
static libusb_interface interface{};
static libusb_config_descriptor configuration{};
static unsigned char last_tag = 0;
static std::string command;
static std::vector<unsigned char> response;
static bool claimed = false;

int libusb_init(libusb_context **context) { *context = new libusb_context; return 0; }
void libusb_exit(libusb_context *context) { delete context; }
ssize_t libusb_get_device_list(libusb_context *, libusb_device ***list) {
    *list = devices; return std::getenv("KOHDALAB_TEST_USB_NO_DEVICE") ? 0 : 1;
}
void libusb_free_device_list(libusb_device **, int) {}
int libusb_get_device_descriptor(libusb_device *, libusb_device_descriptor *descriptor) {
    *descriptor = {}; descriptor->idVendor = 0x0957; descriptor->idProduct = 0x0a07;
    if (std::getenv("KOHDALAB_TEST_USB_GS210")) {
        descriptor->idVendor = 0x0b21; descriptor->idProduct = 0x0039;
    }
    if (std::getenv("KOHDALAB_TEST_USB_OTHER_DEVICE")) descriptor->idProduct = 1;
    descriptor->iSerialNumber = 3; return 0;
}
int libusb_open(libusb_device *, libusb_device_handle **handle) { *handle = new libusb_device_handle; return 0; }
void libusb_close(libusb_device_handle *handle) { delete handle; }
int libusb_get_active_config_descriptor(libusb_device *, libusb_config_descriptor **config) {
    endpoints[0].bEndpointAddress = 2; endpoints[1].bEndpointAddress = 0x86;
    for (auto &endpoint : endpoints) { endpoint.bmAttributes = 2; endpoint.wMaxPacketSize = 512; }
    alternate.bInterfaceClass = 0xfe; alternate.bInterfaceSubClass = 3;
    alternate.bInterfaceProtocol = 1; alternate.bNumEndpoints = 2; alternate.endpoint = endpoints;
    interface.num_altsetting = 1; interface.altsetting = &alternate;
    configuration.bNumInterfaces = 1; configuration.interface = &interface;
    *config = &configuration; return 0;
}
void libusb_free_config_descriptor(libusb_config_descriptor *) {}
int libusb_claim_interface(libusb_device_handle *, int number) {
    if (std::getenv("KOHDALAB_TEST_USB_DISCOVERY_ONLY")) return LIBUSB_ERROR_ACCESS;
    if (number || claimed) return LIBUSB_ERROR_BUSY;
    claimed = true; return 0;
}
int libusb_release_interface(libusb_device_handle *, int number) {
    if (number || !claimed) return LIBUSB_ERROR_INVALID_PARAM;
    claimed = false; return 0;
}
int libusb_get_string_descriptor_ascii(libusb_device_handle *, uint8_t index, unsigned char *buffer, int size) {
    const std::string serial = std::getenv("KOHDALAB_TEST_USB_GS210") ? "91N928756" : "MY53000981";
    if (index != 3 || size < static_cast<int>(serial.size())) return LIBUSB_ERROR_INVALID_PARAM;
    std::memcpy(buffer, serial.data(), serial.size()); return serial.size();
}
int libusb_control_transfer(libusb_device_handle *, uint8_t type, uint8_t request, uint16_t value,
                            uint16_t index, unsigned char *buffer, uint16_t length, unsigned timeout) {
    if (type != 0xa1 || index || !claimed || !timeout) return LIBUSB_ERROR_INVALID_PARAM;
    if (request == 7 && value == 0 && length == 24) {
        std::memset(buffer,0,24); buffer[0] = 1; buffer[14] = 6; return 24;
    }
    if (((request == 160 && value <= 1) || (request == 161 && value == 0)) && length == 1) {
        buffer[0] = 1; return 1;
    }
    return LIBUSB_ERROR_INVALID_PARAM;
}
int libusb_bulk_transfer(libusb_device_handle *, unsigned char endpoint, unsigned char *buffer,
                         int length, int *transferred, unsigned timeout) {
    *transferred = 0;
    if (!claimed || !timeout) return LIBUSB_ERROR_INVALID_PARAM;
    if (endpoint == 2) {
        unsigned char expected_tag = last_tag == 255 ? 1 : last_tag + 1;
        if (length < 12 || buffer[1] != expected_tag || buffer[2] != static_cast<unsigned char>(~expected_tag))
            return LIBUSB_ERROR_IO;
        last_tag = expected_tag;
        unsigned size = 0;
        for (unsigned i = 0; i < 4; ++i) size |= unsigned(buffer[4+i]) << (8*i);
        if (buffer[0] == 1) {
            if (length != static_cast<int>((12 + size + 3) / 4 * 4) || buffer[8] != 1) return LIBUSB_ERROR_IO;
            command.assign(buffer+12, buffer+12+size);
        } else if (buffer[0] == 2) {
            if (length != 12 || size != 4096 || buffer[8]) return LIBUSB_ERROR_IO;
            std::string text = command == "READ?\n" ? "1.25e-6\n" :
                (command == "SYST:ERR?\n" ? "0,No error\n" : (std::getenv("KOHDALAB_TEST_USB_GS210") ? "YOKOGAWA,GS210,91N928756,2.01\n" : "Agilent Technologies,34411A,MY53000981,2.40\n"));
            response.assign(12,0); response[0] = 2; response[1] = last_tag;
            response[2] = static_cast<unsigned char>(~last_tag); response[8] = 1;
            for (unsigned i = 0; i < 4; ++i) response[4+i] = (text.size() >> (8*i)) & 255;
            response.insert(response.end(),text.begin(),text.end());
            while (response.size() % 4) response.push_back(0);
        } else return LIBUSB_ERROR_IO;
        *transferred = length; return 0;
    }
    if (endpoint == 0x86) {
        if (std::getenv("KOHDALAB_TEST_USB_FAIL_READ") && command == "READ?\n") return LIBUSB_ERROR_TIMEOUT;
        if (length % 512 || length < static_cast<int>(response.size()) || response.empty()) return LIBUSB_ERROR_IO;
        std::memcpy(buffer,response.data(),response.size()); *transferred = response.size(); response.clear(); return 0;
    }
    return LIBUSB_ERROR_INVALID_PARAM;
}
const char *libusb_error_name(int error) { return error == LIBUSB_ERROR_TIMEOUT ? "LIBUSB_ERROR_TIMEOUT" : "LIBUSB_ERROR_IO"; }
