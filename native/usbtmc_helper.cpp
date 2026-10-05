// SPDX-License-Identifier: MIT
// Persistent, serial-selected USBTMC transport for the measurement app.
#include "usbtmc_transport.hpp"
#include <sstream>

static std::string hex(const std::string &s) {
    const char *digits = "0123456789abcdef";
    std::string result;
    for (unsigned char c : s) { result += digits[c >> 4]; result += digits[c & 15]; }
    return result;
}
static std::string unhex(const std::string &s) {
    if (s == "-") return "";
    if (s.size() % 2 || s.size() > 131072) throw std::runtime_error("Invalid USB payload size");
    auto nibble = [](char c) {
        if (c >= '0' && c <= '9') return c - '0';
        if (c >= 'a' && c <= 'f') return c - 'a' + 10;
        throw std::runtime_error("Invalid USB hex payload");
    };
    std::string result;
    for (size_t i = 0; i < s.size(); i += 2)
        result += static_cast<char>((nibble(s[i]) << 4) | nibble(s[i+1]));
    return result;
}
static void reply(const char *status, const std::string &value = "") {
    std::cout << "KIV\t" << status << '\t' << hex(value) << std::endl;
}
static unsigned char next_tag(unsigned char &tag) {
    tag = tag == 255 ? 1 : tag + 1;
    return tag;
}
class Transport {
    Handle handle;
    Interface iface;
    unsigned char tag = 0;
    bool ren_supported = false, remote = false;
public:
    unsigned timeout = 5000;
    explicit Transport(libusb_device *device) : handle(device), iface(find_interface(device)) {
        check(libusb_claim_interface(handle.value, iface.number), "Claim USBTMC interface");
        handle.claimed = iface.number;
        unsigned char capabilities[24]{};
        int count = libusb_control_transfer(handle.value, 0xa1, 7, 0, iface.number,
                                             capabilities, sizeof(capabilities), timeout);
        check(count, "Get USBTMC capabilities");
        if (count != 24 || capabilities[0] != 1)
            throw std::runtime_error("Invalid USBTMC capabilities response");
        ren_supported = (capabilities[14] & 2) != 0;
        if (ren_supported) { control(160,1); remote = true; }
    }
    ~Transport() { try { local(); deassert_ren(); } catch (...) {} }
    void control(unsigned char request, uint16_t value = 0) {
        if (!ren_supported) return;
        unsigned char status = 0;
        int count = libusb_control_transfer(handle.value, 0xa1, request, value, iface.number,
                                             &status, 1, timeout);
        check(count, "USB488 remote/local control");
        if (count != 1 || status != 1) throw std::runtime_error("USB488 control request failed");
    }
    void local() { if (remote) control(161); }
    void deassert_ren() { if (remote) { control(160,0); remote = false; } }
    void write(const std::string &command) {
        if (command.empty() || command.size() > 65536) throw std::runtime_error("Invalid USB command size");
        send(handle.value, iface.out, command_message(next_tag(tag), command), timeout);
    }
    std::string read() {
        std::string result;
        constexpr uint32_t limit = 4096;
        unsigned receive_size = ((limit + 12) / iface.packet_size + 1) * iface.packet_size;
        auto deadline = std::chrono::steady_clock::now() + std::chrono::milliseconds(timeout);
        auto remaining = [&]() -> unsigned {
            auto ms = std::chrono::duration_cast<std::chrono::milliseconds>(
                deadline - std::chrono::steady_clock::now()).count();
            if (ms <= 0) throw std::runtime_error("Timed out receiving USBTMC response");
            return static_cast<unsigned>(ms);
        };
        for (unsigned part = 0; part < 4; ++part) {
            unsigned char current = next_tag(tag);
            send(handle.value, iface.out, header(2, current, limit), remaining());
            Bytes message, buffer(receive_size);
            uint32_t expected;
            do {
                int transferred = 0;
                check(libusb_bulk_transfer(handle.value, iface.in, buffer.data(), buffer.size(),
                                           &transferred, remaining()), "USB bulk IN");
                if (transferred <= 0) throw std::runtime_error("Empty USB bulk IN transfer");
                message.insert(message.end(), buffer.begin(), buffer.begin() + transferred);
                expected = response_size(message, current, limit);
            } while (message.size() < 12 + expected);
            result += payload(message, current, limit);
            if (message[8] & 1) {
                if (result.empty()) throw std::runtime_error("Empty instrument response");
                if (std::any_of(result.begin(), result.end(), [](unsigned char c) {
                    return (c < 32 && c != '\r' && c != '\n' && c != '\t') || c > 126;
                })) throw std::runtime_error("Expected an ASCII SCPI response");
                return result;
            }
        }
        throw std::runtime_error("USBTMC response exceeded four transfers");
    }
};
static libusb_device *select_device(Devices &devices, const std::string &serial, uint16_t vid, uint16_t pid) {
    libusb_device *selected = nullptr;
    for (ssize_t i = 0; i < devices.count; ++i) {
        libusb_device_descriptor descriptor{};
        check(libusb_get_device_descriptor(devices.value[i], &descriptor), "Read USB descriptor");
        if (descriptor.idVendor != vid || descriptor.idProduct != pid) continue;
        Handle handle(devices.value[i]);
        unsigned char buffer[256]{};
        int count = libusb_get_string_descriptor_ascii(handle.value, descriptor.iSerialNumber,
                                                       buffer, sizeof(buffer));
        check(count, "Read USB serial number");
        if (std::string(buffer, buffer + count) != serial) continue;
        if (selected) throw std::runtime_error("Multiple USBTMC devices have the requested serial number");
        selected = devices.value[i];
    }
    if (!selected) throw std::runtime_error("USBTMC serial " + serial + " not found; check cable and close other measurement apps.");
    return selected;
}
static std::string list_resources(Devices &devices) {
    std::string resources;
    for (ssize_t i = 0; i < devices.count; ++i) {
        libusb_device_descriptor descriptor{};
        check(libusb_get_device_descriptor(devices.value[i], &descriptor), "Read USB descriptor");
        if (!supported_device(descriptor.idVendor, descriptor.idProduct)) continue;
        find_interface(devices.value[i]);
        Handle handle(devices.value[i]);
        unsigned char buffer[256]{};
        int count = libusb_get_string_descriptor_ascii(handle.value, descriptor.iSerialNumber,
                                                       buffer, sizeof(buffer));
        check(count, "Read USB serial number");
        std::string serial(buffer, buffer + count);
        if (serial.empty() || std::any_of(serial.begin(), serial.end(), [](unsigned char c) {
            return !((c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') ||
                     (c >= '0' && c <= '9') || c == '_' || c == '-');
        })) throw std::runtime_error("Cannot list USBTMC devices: invalid USB serial number");
        resources += usb_resource(descriptor.idVendor, descriptor.idProduct, serial) + "\n";
    }
    return resources;
}
#include "stdio_mode.hpp"
int main(int argc, char **argv) {
    binary_stdio();
    try {
        if (argc == 2 && !std::strcmp(argv[1], "--self-test")) {
            unsigned char tag = 254;
            if (next_tag(tag) != 255 || next_tag(tag) != 1 || next_tag(tag) != 2 ||
                unhex(hex("READ?\n")) != "READ?\n" || unhex("-") != "")
                throw std::runtime_error("Helper self-test failed");
            for (const std::string bad : {"1", "xx"}) {
                bool rejected = false;
                try { unhex(bad); } catch (const std::runtime_error &) { rejected = true; }
                if (!rejected) throw std::runtime_error("Invalid payload accepted");
            }
            std::cout << "USBTMC helper self-test passed.\n";
            return 0;
        }
        if ((argc != 2 && argc != 4) || !std::strlen(argv[1])) throw std::runtime_error("Expected a USBTMC serial number");
        Context context;
        Devices devices(context.value);
        if (!std::strcmp(argv[1], "--list")) {
            // USB descriptors only: no interface claim, remote control, or SCPI.
            reply("OK", list_resources(devices));
            return 0;
        }
        unsigned long vid = argc == 4 ? std::stoul(argv[2], nullptr, 0) : vendor;
        unsigned long pid = argc == 4 ? std::stoul(argv[3], nullptr, 0) : product;
        if (vid > 65535 || pid > 65535 || !supported_device(vid, pid))
            throw std::runtime_error("Unsupported USB device ID");
        Transport transport(select_device(devices, argv[1], vid, pid));
        reply("OK");
        bool poisoned = false;
        std::string line;
        while (std::getline(std::cin, line)) {
            try {
                std::istringstream request(line);
                std::string operation, encoded, extra;
                int address = -1, timeout = 0;
                if (!(request >> operation >> address >> timeout >> encoded) || (request >> extra) ||
                    address != 0 || timeout < 1 || timeout > 3600000)
                    throw std::runtime_error("Invalid USB helper request");
                std::string data = unhex(encoded), response;
                transport.timeout = static_cast<unsigned>(timeout);
                if (operation == "EXIT") {
                    transport.local(); transport.deassert_ren(); reply("OK"); return 0;
                } else if (operation == "LOCAL") transport.local();
                else if (operation == "REN" && data == "0") transport.deassert_ren();
                else if (operation == "WRITE" || operation == "QUERY" || operation == "READ") {
                    if (poisoned) throw std::runtime_error("USB transfer failed; disconnect and reconnect the instruments.");
                    try {
                        if (operation != "READ") transport.write(data);
                        if (operation != "WRITE") response = transport.read();
                    } catch (...) { poisoned = true; throw; }
                } else throw std::runtime_error("Unsupported USB helper operation");
                reply("OK", response);
            } catch (const std::exception &error) { reply("ERR", error.what()); }
        }
        return 0;
    } catch (const std::exception &error) { reply("ERR", error.what()); return 1; }
}
