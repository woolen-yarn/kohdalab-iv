// SPDX-License-Identifier: GPL-2.0-or-later
#include <cassert>
#include <cerrno>
#include <set>
#include <stdexcept>
#include <cstdio>
#include "NiGpibDriver.h"

static std::set<int> connected = {5, 9, 17, 30};
static int addressed = -1, failure = 0;
static bool standby = false;
static unsigned cleans = 0;
static unsigned busy_reads = 0;
static int command(gpib_board_t *, uint8_t *bytes, size_t length, size_t *written) {
    if (length == 2) {
        assert(bytes[0] == 0x3f && bytes[1] == 0x5f);
        addressed = -1; standby = false; ++cleans;
    } else {
        assert(length == 3 && bytes[0] == 0x3f && bytes[1] == 0x40);
        assert(bytes[2] >= 0x21 && bytes[2] <= 0x3e);
        addressed = bytes[2] - 0x20; standby = false;
        if (failure == 1) return -EIO;
    }
    *written = length; return 0;
}
static int go_to_standby(gpib_board_t *) {
    assert(addressed > 0 && !standby);
    if (failure == 2) return -EIO;
    standby = true; return 0;
}
static int line_status(const gpib_board_t *) {
    assert(standby && addressed > 0);
    if (failure == 3) return -EIO;
    if (failure == 4) return 0;
    if (failure == 5 && busy_reads++ < 2) return -EBUSY;
    if (failure == 6) return -EBUSY;
    return ValidNDAC | (connected.count(addressed) ? BusNDAC : 0);
}
static void detach(gpib_board_t *) { assert(addressed == -1); }
int main() {
    gpib_interface interface{};
    interface.command = command;
    interface.go_to_standby = go_to_standby;
    interface.line_status = line_status;
    interface.detach = detach;
    NiGpibDriver driver;
    assert(driver.openForTest(&interface));
    for (int address = 1; address <= 30; ++address) {
        assert(driver.listenerPresent(address) == bool(connected.count(address)));
        assert(addressed == -1 && !standby);
    }
    connected.clear();
    for (int address = 1; address <= 30; ++address) assert(!driver.listenerPresent(address));
    for (int mode : {1,2,3,4,6}) {
        failure = mode;
        unsigned before = cleans;
        bool rejected = false;
        try { driver.listenerPresent(5); } catch (const std::runtime_error &) { rejected = true; }
        assert(rejected && cleans == before + 1 && addressed == -1);
    }
    connected.insert(17);
    failure = 5;
    assert(driver.listenerPresent(17) && busy_reads == 3);
    failure = 0;
    for (int address : {0,31}) {
        bool rejected = false;
        try { driver.listenerPresent(address); } catch (const std::runtime_error &) { rejected = true; }
        assert(rejected);
    }
    interface.line_status = nullptr;
    bool rejected = false;
    try { driver.listenerPresent(5); } catch (const std::runtime_error &) { rejected = true; }
    assert(rejected);
    std::puts("GPIB listener tests passed: all 30 addresses, unregistered devices, empty bus, failure cleanup, no SCPI/read/write calls.");
}
