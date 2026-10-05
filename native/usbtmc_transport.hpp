// SPDX-License-Identifier: MIT
// Shared USBTMC transport for the probe and persistent GS210 / 34411A helper.
#pragma once
#include <libusb.h>
#include <algorithm>
#include <array>
#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <iostream>
#include <stdexcept>
#include <string>
#include <vector>

using Bytes = std::vector<unsigned char>;
inline constexpr uint16_t vendor = 0x0957, product = 0x0a07;
inline bool supported_device(uint16_t vid, uint16_t pid) {
    return (vid == vendor && pid == product) || (vid == 0x0b21 && pid == 0x0039);
}
inline std::string usb_resource(uint16_t vid, uint16_t pid, const std::string &serial) {
    char prefix[40];
    std::snprintf(prefix, sizeof(prefix), "USB0::0x%04X::0x%04X::", vid, pid);
    return std::string(prefix) + serial + "::INSTR";
}
inline constexpr unsigned timeout_ms = 3000;

inline void put32(Bytes &b, size_t pos, uint32_t value) {
    for (unsigned i = 0; i < 4; ++i) b[pos + i] = (value >> (8 * i)) & 255;
}
inline uint32_t get32(const Bytes &b, size_t pos) {
    uint32_t n = 0;
    for (unsigned i = 0; i < 4; ++i) n |= uint32_t(b.at(pos + i)) << (8 * i);
    return n;
}
inline Bytes header(unsigned char id, unsigned char tag, uint32_t size) {
    if (!tag) throw std::runtime_error("Invalid zero USBTMC tag");
    Bytes b(12, 0);
    b[0] = id; b[1] = tag; b[2] = static_cast<unsigned char>(~tag);
    put32(b, 4, size);
    return b;
}
inline Bytes command_message(unsigned char tag, const std::string &command) {
    Bytes b = header(1, tag, command.size());
    b[8] = 1; // End of message.
    b.insert(b.end(), command.begin(), command.end());
    while (b.size() % 4) b.push_back(0);
    return b;
}
inline Bytes idn_message(unsigned char tag) { return command_message(tag, "*IDN?\n"); }
inline uint32_t response_size(const Bytes &b, unsigned char tag, uint32_t limit) {
    if (b.size() < 12 || b[0] != 2 || b[1] != tag ||
        b[2] != static_cast<unsigned char>(~tag) || b[3] != 0)
        throw std::runtime_error("Malformed USBTMC response header/tag");
    uint32_t n = get32(b, 4);
    if (n > limit) throw std::runtime_error("USBTMC response exceeds requested size");
    return n;
}
inline std::string payload(const Bytes &b, unsigned char tag, uint32_t limit) {
    uint32_t n = response_size(b, tag, limit);
    if (b.size() < 12 + n || b.size() > 12 + n + 3)
        throw std::runtime_error("Incomplete or oversized USBTMC response");
    return std::string(b.begin() + 12, b.begin() + 12 + n);
}
inline void check(int result, const char *operation) {
    if (result < 0) throw std::runtime_error(std::string(operation) + ": " +
                                            libusb_error_name(result));
}
struct Context {
    libusb_context *value = nullptr;
    Context() { check(libusb_init(&value), "Initialize USB"); }
    ~Context() { libusb_exit(value); }
};
struct Devices {
    libusb_device **value = nullptr;
    ssize_t count;
    explicit Devices(libusb_context *ctx) : count(libusb_get_device_list(ctx, &value)) {
        check(static_cast<int>(count), "List USB devices");
    }
    ~Devices() { libusb_free_device_list(value, 1); }
};
struct Handle {
    libusb_device_handle *value = nullptr;
    int claimed = -1;
    explicit Handle(libusb_device *device) { check(libusb_open(device, &value), "Open USB device"); }
    ~Handle() {
        if (claimed >= 0) libusb_release_interface(value, claimed);
        libusb_close(value);
    }
};
struct Config {
    libusb_config_descriptor *value = nullptr;
    explicit Config(libusb_device *device) {
        check(libusb_get_active_config_descriptor(device, &value), "Get active USB configuration");
    }
    ~Config() { libusb_free_config_descriptor(value); }
};
struct Interface {
    int number = -1;
    unsigned char in = 0, out = 0;
    unsigned packet_size = 0;
};
inline Interface find_interface(libusb_device *device) {
    Config config(device);
    Interface selected;
    unsigned matches = 0;
    for (unsigned i = 0; i < config.value->bNumInterfaces; ++i) {
        auto &iface = config.value->interface[i];
        for (int a = 0; a < iface.num_altsetting; ++a) {
            auto &alt = iface.altsetting[a];
            if (alt.bAlternateSetting != 0 || alt.bInterfaceClass != 0xfe ||
                alt.bInterfaceSubClass != 3 || alt.bInterfaceProtocol > 1) continue;
            Interface candidate;
            candidate.number = alt.bInterfaceNumber;
            for (unsigned e = 0; e < alt.bNumEndpoints; ++e) {
                auto &endpoint = alt.endpoint[e];
                if ((endpoint.bmAttributes & LIBUSB_TRANSFER_TYPE_MASK) != LIBUSB_TRANSFER_TYPE_BULK) continue;
                if (endpoint.bEndpointAddress & LIBUSB_ENDPOINT_IN) {
                    candidate.in = endpoint.bEndpointAddress;
                    candidate.packet_size = endpoint.wMaxPacketSize & 0x7ff;
                } else candidate.out = endpoint.bEndpointAddress;
            }
            if (candidate.in && candidate.out && candidate.packet_size) {
                selected = candidate; ++matches;
            }
        }
    }
    if (matches != 1) throw std::runtime_error("Expected one USBTMC interface with bulk IN/OUT endpoints");
    return selected;
}
inline void send(libusb_device_handle *device, unsigned char endpoint, Bytes b,
                 unsigned timeout = timeout_ms) {
    int transferred = 0;
    check(libusb_bulk_transfer(device, endpoint, b.data(), static_cast<int>(b.size()),
                               &transferred, timeout), "USB bulk OUT");
    if (transferred != static_cast<int>(b.size())) throw std::runtime_error("Short USB bulk OUT transfer");
}
inline std::string identify(Handle &device, const Interface &iface) {
    check(libusb_claim_interface(device.value, iface.number), "Claim USBTMC interface");
    device.claimed = iface.number;
    send(device.value, iface.out, idn_message(1));
    std::string result;
    constexpr uint32_t limit = 4096;
    const unsigned receive_size = ((limit + 12) / iface.packet_size + 1) * iface.packet_size;
    for (unsigned char tag = 2; tag <= 5; ++tag) {
        send(device.value, iface.out, header(2, tag, limit));
        Bytes message, buffer(receive_size);
        uint32_t expected = 0;
        auto deadline = std::chrono::steady_clock::now() + std::chrono::milliseconds(timeout_ms);
        do {
            auto remaining = std::chrono::duration_cast<std::chrono::milliseconds>(
                deadline - std::chrono::steady_clock::now()).count();
            if (remaining <= 0) throw std::runtime_error("Timed out receiving USBTMC response");
            int transferred = 0;
            check(libusb_bulk_transfer(device.value, iface.in, buffer.data(), buffer.size(),
                                       &transferred, static_cast<unsigned>(remaining)), "USB bulk IN");
            if (transferred <= 0) throw std::runtime_error("Empty USB bulk IN transfer");
            message.insert(message.end(), buffer.begin(), buffer.begin() + transferred);
            expected = response_size(message, tag, limit);
        } while (message.size() < 12 + expected);
        result += payload(message, tag, limit);
        if (message[8] & 1) {
            if (result.empty()) throw std::runtime_error("Instrument returned an empty identification");
            while (!result.empty() && (result.back() == '\r' || result.back() == '\n')) result.pop_back();
            if (result.empty() || std::any_of(result.begin(), result.end(), [](unsigned char c) {
                return c < 32 || c > 126;
            })) throw std::runtime_error("Instrument returned a non-printable identification");
            return result;
        }
    }
    throw std::runtime_error("USBTMC response did not end within four transfers");
}
