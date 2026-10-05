// SPDX-License-Identifier: GPL-2.0-only
// In-process fake 82357B for exercising the real userspace driver without hardware.
#include <libusb-1.0/libusb.h>
#include <cassert>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <set>
#include <stdexcept>
#include <string>
#include <vector>
#include "agilent_82357b.hpp"

struct libusb_context {};
struct libusb_device {};
struct libusb_device_handle {};

enum class Failure { None, ShortWrite, BadStatus, ReadTimeout, IncompleteRead, NoAtn, ShortStatus, StatusUsbError, LostResetReleaseAck, NoStandby };
static libusb_device device;
static libusb_device *devices[] = {&device, nullptr};
static std::vector<unsigned short> products;
static size_t enumeration = 0;
static bool claimed = false;
static Failure failure = Failure::None;
static bool write_completion = false;
static bool atn = false;
static unsigned receive_register_reads = 0;
static bool controller_listener = false, first_byte_consumed = false;
static bool ifc = false, ren = false;
static unsigned char hardware_control = 0x3e;
static bool configured_chip = false;
static bool lost_reset_ack = false;
static unsigned char status_prefix = 1;
static int status_length = 6;
static unsigned expected_write_count = 0;
static unsigned firmware_restarts = 0;
static std::set<int> listeners{5, 9, 17};
static std::vector<std::vector<unsigned char>> gpib_messages;
static std::vector<unsigned char> pending_reply;
static std::vector<unsigned char> auxiliary_values;

static void reset(std::vector<unsigned short> listed_products = {0x0718}) {
    products = std::move(listed_products);
    enumeration = 0;
    claimed = false;
    failure = Failure::None;
    write_completion = false;
    atn = false;
    controller_listener = first_byte_consumed = false;
    receive_register_reads = 0;
    ifc = ren = false;
    hardware_control = 0x3e;
    configured_chip = false;
    lost_reset_ack = false;
    status_prefix = 1;
    status_length = 6;
    expected_write_count = 0;
    firmware_restarts = 0;
    listeners = {5, 9, 17};
    gpib_messages.clear();
    pending_reply.clear();
    auxiliary_values.clear();
}

static unsigned le32(const unsigned char *bytes) {
    return unsigned(bytes[0]) | (unsigned(bytes[1]) << 8) |
        (unsigned(bytes[2]) << 16) | (unsigned(bytes[3]) << 24);
}

template <class Function>
static void expect_throw(Function &&fn, const char *contains) {
    bool rejected = false;
    try { fn(); }
    catch (const std::runtime_error &error) {
        rejected = true;
        assert(std::string(error.what()).find(contains) != std::string::npos);
    }
    assert(rejected);
}

int libusb_init(libusb_context **context) { *context = new libusb_context; return 0; }
void libusb_exit(libusb_context *context) { delete context; }
ssize_t libusb_get_device_list(libusb_context *, libusb_device ***list) {
    *list = devices;
    if (products.empty()) return 0;
    return 1;
}
void libusb_free_device_list(libusb_device **, int) {}
int libusb_get_device_descriptor(libusb_device *, libusb_device_descriptor *descriptor) {
    *descriptor = {};
    descriptor->idVendor = 0x0957;
    descriptor->idProduct = products.at(enumeration);
    if (enumeration + 1 < products.size()) ++enumeration;
    return 0;
}
int libusb_open(libusb_device *, libusb_device_handle **handle) { *handle = new libusb_device_handle; return 0; }
void libusb_close(libusb_device_handle *handle) { delete handle; }
int libusb_set_auto_detach_kernel_driver(libusb_device_handle *, int enable) { return enable == 1 ? 0 : LIBUSB_ERROR_INVALID_PARAM; }
int libusb_claim_interface(libusb_device_handle *, int interface) {
    if (interface || claimed) return LIBUSB_ERROR_BUSY;
    claimed = true;
    return 0;
}
int libusb_release_interface(libusb_device_handle *, int interface) {
    if (interface || !claimed) return LIBUSB_ERROR_INVALID_PARAM;
    claimed = false;
    return 0;
}

int libusb_control_transfer(libusb_device_handle *, uint8_t type, uint8_t request, uint16_t value,
                            uint16_t, unsigned char *buffer, uint16_t length, unsigned timeout) {
    assert(timeout);
    // Cypress FX2 firmware RAM uploads during boot-PID initialization.
    if (type == 0x40 && request == 0xa0) {
        assert(length);
        if (value == 0xe600 && buffer[0] == 0) ++firmware_restarts;
        return length;
    }
    if (type != 0xc0 || request != 4 || !claimed) return LIBUSB_ERROR_INVALID_PARAM;
    if (value == 0xa0 && length == 2) { buffer[0] = 0x5f; buffer[1] = 0; return 2; }
    if (value == 0xb0 && length == 8) {
        if (failure == Failure::StatusUsbError) return LIBUSB_ERROR_IO;
        std::memset(buffer, 0, 8);
        buffer[0] = status_prefix;
        buffer[1] = 0x20; // Physical 82357B: completed-by-count flag.
        unsigned count = failure == Failure::BadStatus ? expected_write_count - 1 : expected_write_count;
        buffer[2] = count & 255; buffer[3] = (count >> 8) & 255;
        buffer[4] = (count >> 16) & 255; buffer[5] = (count >> 24) & 255;
        return failure == Failure::ShortStatus ? 5 : status_length;
    }
    return LIBUSB_ERROR_INVALID_PARAM;
}

int libusb_interrupt_transfer(libusb_device_handle *, unsigned char endpoint, unsigned char *buffer,
                              int length, int *transferred, unsigned timeout) {
    assert(endpoint == 0x88 && length == 8 && timeout);
    if (!write_completion) { *transferred = 0; return LIBUSB_ERROR_TIMEOUT; }
    buffer[0] = 2; *transferred = 1; write_completion = false;
    return 0;
}

int libusb_bulk_transfer(libusb_device_handle *, unsigned char endpoint, unsigned char *buffer,
                         int length, int *transferred, unsigned timeout) {
    assert(claimed && timeout);
    *transferred = 0;
    if (endpoint == 0x06) {
        assert(length >= 1);
        if (failure == Failure::ShortWrite) { *transferred = length - 1; return 0; }
        if (buffer[0] == 4) { // register write
            assert(length == 2 + int(buffer[1]) * 2);
            for (int i = 0; i < buffer[1]; ++i) {
                if (buffer[2 + i * 2] == 3) {
                    unsigned value = buffer[3 + i * 2];
                    auxiliary_values.push_back(value);
                    if (!(hardware_control & 1)) continue; // Writes ignored while external reset held.
                    if (value == 0x80) { atn = ifc = ren = false; configured_chip = false; }
                    if (value == 0) configured_chip = true; // complete setup leaves software reset
                    if (value == 0x0c && failure != Failure::NoAtn && configured_chip) atn = true;
                    if (value == 0x0b && failure != Failure::NoStandby) atn = false;
                    if (value == 0x8f) ifc = true;
                    if (value == 0x0f) ifc = false;
                    if (value == 0x90) ren = true;
                    if (value == 0x10) ren = false;
                }
                if (buffer[2 + i * 2] == 0x0a) {
                    hardware_control = buffer[3 + i * 2];
                    if (failure == Failure::LostResetReleaseAck && (hardware_control & 1)) lost_reset_ack = true;
                }
                if (buffer[2 + i * 2] == 0x0c && buffer[3 + i * 2] == 1) {
                    ifc = ren = atn = false;
                    configured_chip = false;
                    // Real diagnostic: RESET_TO_POWERUP does not release external TI reset.
                }
            }
            pending_reply = {0xfb, 0};
        } else if (buffer[0] == 5) { // register read
            if (!atn && controller_listener) ++receive_register_reads;
            assert(length == 3 && buffer[1] == 1);
            unsigned char value = 0;
            if (buffer[2] == 2) {
                // TI9914 register alias bug: ADSR read while listener-active
                // consumes the first pending data byte before the USB read.
                if (!atn && controller_listener) first_byte_consumed = true;
                value = 0;
            } // Deliberately unreliable ADSR must not block I/O.
            if (buffer[2] == 3) {
                value = (atn ? 0x80 : 0) | (ifc ? 2 : 0) | (ren ? 1 : 0);
                if (!gpib_messages.empty()) {
                    const auto &command = gpib_messages.back();
                    if (command.size() == 3 && command[0] == 0x3f && command[1] == 0x40)
                        value |= listeners.count(command[2] - 0x20) ? 0x20 : 0;
                }
            }
            if (buffer[2] == 0x0a) value = hardware_control;
            pending_reply = {0xfa, 0, value};
        } else if (buffer[0] == 1) { // GPIB command or data write
            assert(length >= 8);
            unsigned count = le32(buffer + 4);
            assert(length == int(8 + count));
            std::vector<unsigned char> payload(buffer + 8, buffer + 8 + count);
            gpib_messages.push_back(payload);
            if (buffer[3] == 0x0b) { // EOI is set on ordinary SCPI writes.
                assert(!payload.empty());
            } else {
                assert(buffer[3] == 0x1e); // command frames use ATN plus fast-write flags.
                assert(atn);
                controller_listener = payload.size() == 3 && payload[0] == 0x3f && payload[1] == 0x20;
            }
            expected_write_count = count;
            // Physical NO_ADDRESS data cannot handshake while ATN remains asserted.
            write_completion = (buffer[3] & 0x10) || !atn;
        } else if (buffer[0] == 3) { // read request
            assert(length == 9 && buffer[3] == 3 && le32(buffer + 4) == 4096);
            if (atn) pending_reply.clear();
            else if (failure == Failure::IncompleteRead) pending_reply = {'x', '\n', 0};
            else pending_reply = first_byte_consumed ? std::vector<unsigned char>{'2', '\n', 1} : std::vector<unsigned char>{'4', '2', '\n', 1}; // EOI
            first_byte_consumed = false;
            controller_listener = false;
        } else {
            assert(false && "unexpected 82357B bulk-out packet");
        }
        *transferred = length;
        return 0;
    }
    if (endpoint == 0x82) {
        if (lost_reset_ack) {
            lost_reset_ack = false;
            failure = Failure::None;
            pending_reply.clear();
            return LIBUSB_ERROR_TIMEOUT;
        }
        if (failure == Failure::ReadTimeout) return LIBUSB_ERROR_TIMEOUT;
        if (pending_reply.empty()) return LIBUSB_ERROR_TIMEOUT;
        assert(length >= int(pending_reply.size()) && !pending_reply.empty());
        std::memcpy(buffer, pending_reply.data(), pending_reply.size());
        *transferred = static_cast<int>(pending_reply.size());
        pending_reply.clear();
        return 0;
    }
    return LIBUSB_ERROR_INVALID_PARAM;
}

const char *libusb_error_name(int error) {
    return error == LIBUSB_ERROR_TIMEOUT ? "LIBUSB_ERROR_TIMEOUT" : "LIBUSB_ERROR_IO";
}

int main() {
    // Reproduce the captured first-command status: 01 20 03 00 00 00.
    // Also support complete counts with trailing extension bytes.
    for (int response_length : {6, 7, 8}) {
        reset();
        Agilent82357B driver;
        driver.open();
        status_length = response_length;
        assert(driver.query(5, "*IDN?") == "42");
    }
    reset();
    {
        Agilent82357B driver;
        driver.open();
        failure = Failure::NoStandby;
        expect_throw([&] { driver.query(5, "*IDN?"); }, "could not release ATN");
        assert(gpib_messages.size() == 1 && gpib_messages[0].size() == 3);
        failure = Failure::None;
        expect_throw([&] { driver.send(5, "*IDN?"); }, "reconnect before further commands");
    }
    reset();
    {
        Agilent82357B driver;
        failure = Failure::LostResetReleaseAck;
        expect_throw([&] { driver.open(); }, "USB read");
        assert(hardware_control & 1); // Device accepted write but its ACK was lost.
    }
    assert(!(hardware_control & 1) && (hardware_control & ~7) == 0x38 && !claimed);
    reset();
    // Reopening the same physical adapter after close() leaves HW_CONTROL=0x3e.
    // Chip initialization must release that reset before AUX configuration.
    for (int cycle = 0; cycle < 4; ++cycle) {
        assert(hardware_control == 0x3e);
        {
            Agilent82357B driver;
            driver.open();
            assert(configured_chip && hardware_control == 0x3f);
            const std::vector<unsigned char> expected_init{0x80, 5, 4, 0x0a, 9, 0x18, 1, 0x0e,
                0x15, 0x17, 0, 0x11, 0x8f, 0x0f};
            assert(std::equal(expected_init.begin(), expected_init.end(), auxiliary_values.end() - expected_init.size()));
            assert(driver.query(9, "*IDN?") == "42");
        assert(receive_register_reads == 0); // No register requests between GTS and READ.
        }
        assert(hardware_control == 0x3e);
    }
    reset();
    {
        Agilent82357B driver;
        setenv("KOHDALAB_IV_82357B_TRACE", "1", 1);
        driver.open();
        driver.diagnose();
        unsetenv("KOHDALAB_IV_82357B_TRACE");
        assert(gpib_messages.empty()); // Diagnostic mode never sends GPIB data or SCPI.
    }
    reset();
    {
        Agilent82357B driver;
        driver.open();
        failure = Failure::NoAtn;
        expect_throw([&] { driver.diagnose(); }, "could not assert ATN");
        assert(gpib_messages.empty());
    }
    for (unsigned char prefix : {0x00, 0x01, 0x03, 0x4f, 0xff}) {
        reset();
        Agilent82357B driver;
        driver.open();
        status_prefix = prefix;
        // Different leading status bytes must not hide a valid complete count.
        assert(driver.query(9, "*IDN?") == "42");
        assert(receive_register_reads == 0); // No register requests between GTS and READ.
    }
    for (Failure status_failure : {Failure::ShortStatus, Failure::StatusUsbError}) {
        reset();
        Agilent82357B driver;
        driver.open();
        failure = status_failure;
        expect_throw([&] { driver.query(5, "*IDN?"); }, status_failure == Failure::ShortStatus ? "USB bytes=5, raw=" : "write status");
        failure = Failure::None;
        expect_throw([&] { driver.send(5, "*IDN?"); }, "reconnect before further commands");
    }
    reset();
    {
        Agilent82357B driver;
        driver.open();
        setenv("KOHDALAB_IV_82357B_TRACE", "1", 1);
        assert(driver.query(9, "*IDN?") == "42");
        assert(receive_register_reads == 0); // No register requests between GTS and READ.
        unsetenv("KOHDALAB_IV_82357B_TRACE");
    }
    reset();
    {
        Agilent82357B driver;
        driver.open();
        failure = Failure::NoAtn;
        expect_throw([&] { driver.query(5, "*IDN?"); }, "BSR=0x00, ADSR=skipped (ATN not asserted), HW_CONTROL=0x3f");
        assert(gpib_messages.empty()); // Still refuse to transmit when physical ATN is absent.
        failure = Failure::None;
        expect_throw([&] { driver.query(5, "*IDN?"); }, "reconnect before further commands");
    }
    reset();
    {
        Agilent82357B driver;
        driver.open();
        for (int address = 1; address <= 30; ++address)
            assert(driver.listenerPresent(address) == bool(listeners.count(address)));
        driver.enableRemote(true);
        assert(driver.query(9, "READ?\n") == "42");
        assert(receive_register_reads == 0);
        driver.deviceClear(5);
        driver.goToLocal(5);
        driver.enableRemote(false);
        driver.close();
    }
    assert(!claimed);
    assert(std::find(auxiliary_values.begin(), auxiliary_values.end(), 0x90) != auxiliary_values.end());
    assert(std::find(auxiliary_values.begin(), auxiliary_values.end(), 0x10) != auxiliary_values.end());

    reset();
    {
        Agilent82357B driver;
        driver.open();
        failure = Failure::ShortWrite;
        expect_throw([&] { driver.send(5, "READ?\n"); }, "short USB write");
        failure = Failure::None;
        expect_throw([&] { driver.read(5); }, "reconnect before further commands");
        driver.close();
    }
    assert(!claimed);
    assert(std::find(auxiliary_values.begin(), auxiliary_values.end(), 0x10) != auxiliary_values.end());

    reset();
    {
        Agilent82357B driver;
        driver.open();
        failure = Failure::BadStatus;
        expect_throw([&] { driver.send(5, "READ?\n"); }, "incomplete GPIB write");
        failure = Failure::None;
        expect_throw([&] { driver.deviceClear(5); }, "reconnect before further commands");
        driver.close();
    }

    for (Failure read_failure : {Failure::ReadTimeout, Failure::IncompleteRead}) {
        reset();
        Agilent82357B driver;
        driver.open();
        failure = read_failure;
        expect_throw([&] { driver.read(9); }, read_failure == Failure::ReadTimeout ? "USB read" : "incomplete GPIB response");
        failure = Failure::None;
        expect_throw([&] { driver.send(9, "READ?\n"); }, "reconnect before further commands");
        driver.goToLocal(9); // cleanup remains possible after a poisoned measurement.
        driver.close();
        assert(!claimed);
    }

    // 82357B firmware may need two uploads: the first re-enumerates as the boot PID again.
    reset({0x0518, 0x0518, 0x0718});
    setenv("KOHDALAB_IV_82357B_FIRMWARE",
        "build/agilent-82357/firmware-repo/agilent_82357a/measat_releaseX1.8.hex", 1);
    {
        Agilent82357B driver;
        driver.open();
        assert(firmware_restarts == 2);
        driver.close();
    }
    unsetenv("KOHDALAB_IV_82357B_FIRMWARE");
    std::puts("82357B mock tests passed: initialization, 30-address scan, SCPI/EOI, cleanup, failures, and two-stage firmware boot.");
}
