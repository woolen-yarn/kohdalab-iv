// SPDX-License-Identifier: MIT
#include "../native/usb_setup_policy.hpp"
#include <cassert>
int main() {
    assert(setup_device_name(0x3923, 0x7618, false, 0, ""));
    assert(setup_device_name(0x0957, 0x0518, false, 0, ""));
    assert(setup_device_name(0x0957, 0x0718, false, 0, ""));
    for (auto pair : {std::pair<unsigned,unsigned>{0x0957,0x0a07}, {0x0b21,0x0039}}) {
        assert(setup_device_name(pair.first, pair.second, false, 0, "USB\\Class_FE&SubClass_03&Prot_01"));
        assert(setup_device_name(pair.first, pair.second, true, 0, "USB\\Class_FE&SubClass_03&Prot_01"));
        assert(!setup_device_name(pair.first, pair.second, false, 0, "USB\\Class_08&SubClass_06&Prot_50"));
        assert(!setup_device_name(pair.first, pair.second, true, 1, "USB\\Class_FE&SubClass_03&Prot_01"));
        assert(!setup_device_name(pair.first, pair.second, false, 0, ""));
    }
    assert(!setup_device_name(0x1234, 0x5678, false, 0, "USB\\Class_FE&SubClass_03"));
    assert(!setup_device_name(0x3923, 0x7619, false, 0, ""));
    assert(!setup_device_name(0x0957, 0x0717, false, 0, ""));
}
