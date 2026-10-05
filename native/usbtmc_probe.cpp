// SPDX-License-Identifier: MIT
// Identification only: no measurement or output commands.
#include "usbtmc_transport.hpp"

static void require(bool ok) { if (!ok) throw std::runtime_error("Self-test failed"); }
static void self_test() {
    require(idn_message(1) == Bytes({1,1,254,0,6,0,0,0,1,0,0,0,42,73,68,78,63,10,0,0}));
    require(header(2,255,4096) == Bytes({2,255,0,0,0,16,0,0,0,0,0,0}));
    Bytes good = header(2,2,3); good[8] = 1;
    good.insert(good.end(), {'A','B','C',0});
    require(payload(good,2,4096) == "ABC");
    for (int type = 0; type < 6; ++type) {
        Bytes bad = good;
        if (type == 0) bad[0] = 1;
        if (type == 1) bad[1] = 3;
        if (type == 2) bad[2] = 0;
        if (type == 3) bad.resize(13);
        if (type == 4) put32(bad,4,4097);
        if (type == 5) bad.resize(19);
        bool rejected = false;
        try { payload(bad,2,4096); } catch (const std::runtime_error &) { rejected = true; }
        require(rejected);
    }
    std::cout << "USBTMC framing self-test passed.\n";
}
#include "stdio_mode.hpp"
int main(int argc, char **argv) {
    binary_stdio();
    if (argc != 2 || (std::strcmp(argv[1], "--list") &&
                      std::strcmp(argv[1], "--idn") && std::strcmp(argv[1], "--self-test"))) {
        std::cerr << "Usage: usbtmc-probe --list | --idn | --self-test\n";
        return 2;
    }
    try {
        if (!std::strcmp(argv[1], "--self-test")) { self_test(); return 0; }
        Context context;
        Devices devices(context.value);
        unsigned matches = 0;
        for (ssize_t i = 0; i < devices.count; ++i) {
            libusb_device_descriptor descriptor{};
            check(libusb_get_device_descriptor(devices.value[i], &descriptor), "Read USB descriptor");
            if (!supported_device(descriptor.idVendor, descriptor.idProduct)) continue;
            ++matches;
            Handle handle(devices.value[i]);
            unsigned char serial[256]{};
            int size = libusb_get_string_descriptor_ascii(handle.value, descriptor.iSerialNumber, serial, sizeof(serial));
            check(size, "Read USB serial");
            auto resource = usb_resource(descriptor.idVendor, descriptor.idProduct, std::string(serial, serial + size));
            std::cout << "Detected: " << resource << std::endl;
            Interface iface = find_interface(devices.value[i]);
            if (!std::strcmp(argv[1], "--list")) continue;
            std::printf("USBTMC interface=%d bulk OUT=0x%02X IN=0x%02X packet=%u\n",
                        iface.number, iface.out, iface.in, iface.packet_size);
            std::string response = identify(handle, iface);
            check(libusb_release_interface(handle.value, handle.claimed), "Release USBTMC interface");
            handle.claimed = -1;
            std::cout << "USB IDN: " << response << '\n';
        }
        if (!matches) throw std::runtime_error("No supported GS210 / 34411A USB device found");
        return 0;
    } catch (const std::exception &error) {
        std::cerr << "Error: " << error.what() << '\n';
        return 1;
    }
}
