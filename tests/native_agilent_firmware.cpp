// SPDX-License-Identifier: GPL-2.0-only
// Exercise actual firmware verification/loading with a mocked control endpoint.
#include "../native/agilent_82357b.hpp"
#include <cassert>
#include <cstdio>
#include <filesystem>
#include <iostream>
#include <bitset>
static unsigned bytes, starts, stops, calls;
static std::bitset<6908> addresses;
static bool fail_ram, disconnect_restart;
extern "C" {
int libusb_init(libusb_context **) { assert(false); return -1; }
void libusb_exit(libusb_context *) { assert(false); }
ssize_t libusb_get_device_list(libusb_context *, libusb_device ***) { assert(false); return -1; }
void libusb_free_device_list(libusb_device **, int) { assert(false); }
int libusb_get_device_descriptor(libusb_device *, libusb_device_descriptor *) { assert(false); return -1; }
int libusb_open(libusb_device *, libusb_device_handle **) { assert(false); return -1; }
void libusb_close(libusb_device_handle *) { assert(false); }
int libusb_claim_interface(libusb_device_handle *, int) { assert(false); return -1; }
int libusb_release_interface(libusb_device_handle *, int) { assert(false); return -1; }
int libusb_set_auto_detach_kernel_driver(libusb_device_handle *, int) { assert(false); return -1; }
int libusb_bulk_transfer(libusb_device_handle *, unsigned char, unsigned char *, int, int *, unsigned int) { assert(false); return -1; }
const char *libusb_error_name(int) { return "Mock USB error"; }
}
extern "C" int libusb_control_transfer(libusb_device_handle *, uint8_t type, uint8_t request,
    uint16_t address, uint16_t index, unsigned char *data, uint16_t length, unsigned int timeout) {
    assert(type == 0x40 && request == 0xa0 && index == 0 && timeout == 1000);
    ++calls;
    if (address == 0xe600) {
        assert(length == 1);
        if (data[0] == 1) { ++stops; assert(starts == 0 && bytes == 0); }
        else { ++starts; assert(bytes == 6908 && stops == 1); if (disconnect_restart) return LIBUSB_ERROR_NO_DEVICE; }
    } else {
        assert(stops == 1 && starts == 0 && address + length <= 6908);
        if (fail_ram) return LIBUSB_ERROR_TIMEOUT;
        for (unsigned i = address; i < address + length; ++i) { assert(!addresses[i]); addresses.set(i); }
        bytes += length;
    }
    return length;
}
static void reset() { bytes = starts = stops = calls = 0; addresses.reset(); fail_ram = disconnect_restart = false; }
int main(int argc, char **argv) {
    assert(argc == 2);
    std::string path = argv[1];
    reset(); Agilent82357B::loadFirmware(nullptr, path);
    assert(bytes == 6908 && starts == 1 && stops == 1);
    reset(); disconnect_restart = true; Agilent82357B::loadFirmware(nullptr, path); assert(starts == 1);
    reset(); fail_ram = true;
    try { Agilent82357B::loadFirmware(nullptr, path); assert(false); } catch (const std::runtime_error &) {}
    assert(stops == 1 && starts == 0);
    reset();
    try { Agilent82357B::loadFirmware(nullptr, path + ".missing"); assert(false); } catch (const std::runtime_error &) {}
    assert(calls == 0);
    auto temp = std::filesystem::temp_directory_path() / "kohdalab-test-corrupt-82357b.hex";
    std::ifstream input(path); std::string original((std::istreambuf_iterator<char>(input)), {});
    for (std::string content : {original.substr(0, 100), original + "\n", std::string(70000, '0')}) {
        std::ofstream(temp) << content;
        try { Agilent82357B::loadFirmware(nullptr, temp.string()); assert(false); } catch (const std::runtime_error &) {}
        assert(calls == 0);
    }
    std::filesystem::remove(temp);
    std::cout << "82357B firmware checks passed\n";
}
