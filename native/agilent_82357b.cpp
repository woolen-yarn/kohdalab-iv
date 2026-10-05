// SPDX-License-Identifier: GPL-2.0-only
#include "agilent_82357b.hpp"
#ifdef _WIN32
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <bcrypt.h>
#else
#include <CommonCrypto/CommonDigest.h>
#endif
#include <cstdlib>

static std::string firmwarePath() {
    if (const char *path = std::getenv("KOHDALAB_IV_82357B_FIRMWARE")) return path;
#ifdef _WIN32
    if (const char *directory = std::getenv("LOCALAPPDATA"))
        return std::string(directory) + "/KohdaLab IV/measat_releaseX1.8.hex";
#else
    if (const char *home = std::getenv("HOME"))
        return std::string(home) + "/Library/Application Support/KohdaLab IV/measat_releaseX1.8.hex";
#endif
    throw std::runtime_error("82357B firmware path unavailable");
}

void Agilent82357B::loadFirmware(libusb_device_handle *handle, const std::string &path) {
    std::ifstream input(path, std::ios::binary);
    if (!input) throw std::runtime_error("82357B needs firmware. Run the included 82357B firmware setup first: " + path);
    std::string contents((std::istreambuf_iterator<char>(input)), {});
    if (contents.size() > 65536) throw std::runtime_error("82357B firmware file is too large");
    unsigned char digest[32];
#ifdef _WIN32
    BCRYPT_ALG_HANDLE algorithm = nullptr;
    if (BCryptOpenAlgorithmProvider(&algorithm, BCRYPT_SHA256_ALGORITHM, nullptr, 0) < 0)
        throw std::runtime_error("Cannot initialize firmware SHA256 verification");
    NTSTATUS hash_result = BCryptHash(algorithm, nullptr, 0,
        reinterpret_cast<PUCHAR>(contents.data()), static_cast<ULONG>(contents.size()),
        digest, sizeof(digest));
    BCryptCloseAlgorithmProvider(algorithm, 0);
    if (hash_result < 0) throw std::runtime_error("Cannot verify firmware SHA256");
#else
    CC_SHA256(contents.data(), static_cast<CC_LONG>(contents.size()), digest);
#endif
    const unsigned char expected[] = {0x2c,0x40,0x21,0x3a,0x3d,0x0d,0x8b,0x7d,0x7c,0xc0,0x83,0xb1,0x55,0xa5,0xed,0x09,
        0x4e,0x29,0x76,0x72,0x14,0xa9,0xb8,0xaa,0x74,0x54,0x86,0xf3,0x5e,0xf5,0x86,0x64};
    if (!std::equal(std::begin(expected), std::end(expected), digest)) throw std::runtime_error("82357B firmware checksum mismatch");
    // Validate the whole image before halting the CPU or writing any RAM.
    struct Block { unsigned address; Bytes data; };
    std::vector<Block> blocks;
    size_t start = 0;
    bool eof = false;
    while (start < contents.size()) {
        size_t end = contents.find('\n', start);
        if (end == std::string::npos) end = contents.size();
        std::string line = contents.substr(start, end - start);
        start = end + 1;
        if (!line.empty() && line.back() == '\r') line.pop_back();
        if (line.empty()) continue;
        if (eof || line[0] != ':' || line.size() < 11 || line.size() % 2 != 1)
            throw std::runtime_error("Invalid 82357B firmware record");
        Bytes record;
        auto digit = [](char c) -> unsigned {
            if (c >= '0' && c <= '9') return c - '0';
            if (c >= 'A' && c <= 'F') return c - 'A' + 10;
            if (c >= 'a' && c <= 'f') return c - 'a' + 10;
            throw std::runtime_error("Invalid firmware hex digit");
        };
        unsigned sum = 0;
        for (size_t i = 1; i < line.size(); i += 2) {
            unsigned byte = (digit(line[i]) << 4) | digit(line[i + 1]);
            record.push_back(byte); sum += byte;
        }
        if ((sum & 255) || record.size() != static_cast<size_t>(record[0]) + 5)
            throw std::runtime_error("Invalid firmware record checksum or length");
        unsigned address = (record[1] << 8) | record[2];
        if (record[3] == 1 && !record[0] && !address) { eof = true; continue; }
        if (record[3] != 0 || address + record[0] > 0x1b00)
            throw std::runtime_error("Unsupported firmware address or record type");
        blocks.push_back({address, Bytes(record.begin() + 4, record.end() - 1)});
    }
    if (!eof || blocks.empty()) throw std::runtime_error("Incomplete 82357B firmware");
    auto writeRam = [&](unsigned address, Bytes &data) {
        int result = libusb_control_transfer(handle, 0x40, 0xa0, address, 0,
            data.data(), static_cast<uint16_t>(data.size()), 1000);
        check(result, "firmware upload");
        if (result != static_cast<int>(data.size())) throw std::runtime_error("Short 82357B firmware upload");
    };
    Bytes cpu{1};
    writeRam(0xe600, cpu);
    // If upload fails, leave CPU halted; unplug/replug before retrying.
    for (auto &block : blocks) writeRam(block.address, block.data);
    cpu[0] = 0;
    int result = libusb_control_transfer(handle, 0x40, 0xa0, 0xe600, 0, cpu.data(), 1, 1000);
    if (result != 1 && result != LIBUSB_ERROR_NO_DEVICE) {
        check(result, "firmware CPU restart");
        throw std::runtime_error("Short 82357B CPU restart");
    }
}

void Agilent82357B::open() {
    check(libusb_init(&context_), "initialization");
    int uploads = 0;
    auto deadline = Clock::now() + std::chrono::seconds(15);
    while (!handle_ && Clock::now() < deadline) {
        unsigned product = 0;
        libusb_device **devices = nullptr;
        ssize_t count = libusb_get_device_list(context_, &devices);
        check(static_cast<int>(count), "enumeration");
        for (ssize_t i = 0; i < count; ++i) {
            libusb_device_descriptor descriptor{};
            if (libusb_get_device_descriptor(devices[i], &descriptor) != 0 || descriptor.idVendor != 0x0957 ||
                (descriptor.idProduct != 0x0518 && descriptor.idProduct != 0x0718)) continue;
            int result = libusb_open(devices[i], &handle_);
            if (result < 0) { libusb_free_device_list(devices, 1); check(result, "open (close other GPIB software)"); }
            product = descriptor.idProduct;
            break;
        }
        libusb_free_device_list(devices, 1);
        if (!handle_) { std::this_thread::sleep_for(std::chrono::milliseconds(100)); continue; }
        if (product == 0x0718) break;
        if (++uploads > 2) throw std::runtime_error("82357B firmware did not initialize after two uploads; unplug/replug the adapter.");
        loadFirmware(handle_, firmwarePath());
        libusb_close(handle_); handle_ = nullptr;
        // Allow old bootloader device to disappear before the next enumeration.
        std::this_thread::sleep_for(std::chrono::milliseconds(1000));
    }
    if (!handle_) throw std::runtime_error("82357B did not reconnect after firmware upload");
    libusb_set_auto_detach_kernel_driver(handle_, 1);
    check(libusb_claim_interface(handle_, 0), "claim interface (close other GPIB software)");
    claimed_ = true;
    traceState("before hardware reset");
    registers({{0x0b, 0x20}, {0x0c, 1}}); // failure LED, hardware reset
    std::this_thread::sleep_for(std::chrono::milliseconds(2));
    traceState("after hardware reset");
    // close() asserts the external TI reset (NOT_TI_RESET=0). On the remote
    // adapter RESET_TO_POWERUP preserves HW_CONTROL=0x3e, so AUX writes made
    // before deasserting that reset were ignored. Release it before chip setup.
    hardware_ = (reg(0x0a) & ~7) | 5;
    // The write may take effect even if its USB acknowledgement is lost.
    initialized_ = true;
    registers({{0x0a, hardware_}});
    std::this_thread::sleep_for(std::chrono::milliseconds(2));
    traceState("after hardware reset release");
    registers({{3, 0x80}, {3, 5}, {3, 4}, {3, 0x0a}, {3, 9}, {3, 0x18}, {3, 1}, {3, 0x0e},
        {3, 0x15}, {3, 0x17}, {0x0e, 38}, {4, 0}, {6, 0}, {5, 0}, {0x0d, 1},
        {0, 0x30}, {1, 2}, {3, 0}, {0x0b, 1}});
    traceState("after register initialization");
    hardware_ |= 2;
    registers({{3, 0x11}, {0x0a, hardware_}}); // system controller
    traceState("after system controller request");
    auxiliary(0x8f); // assert IFC, become controller in charge
    std::this_thread::sleep_for(std::chrono::microseconds(150));
    traceState("IFC asserted");
    auxiliary(0x0f);
    traceState("IFC released");
}
