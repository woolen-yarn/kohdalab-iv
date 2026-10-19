// SPDX-License-Identifier: MIT
#pragma once
#include <string>
#include <algorithm>
#include <cctype>

inline const char* setup_device_name(unsigned vid, unsigned pid, bool composite,
                                     unsigned interface_number, const std::string& compatible) {
    if (composite && interface_number != 0) return nullptr;
    if (vid == 0x3923 && pid == 0x7618) return "NI GPIB-USB-HS+";
    // 0957:0518 is the Cypress boot mode. Never bind it to WinUSB: the native
    // helper accepts only an already-initialized 0957:0718 adapter.
    if (vid == 0x0957 && pid == 0x0718) return "Agilent 82357B";
    // The same Yokogawa product ID must never authorize storage-mode replacement.
    std::string ids = compatible;
    std::transform(ids.begin(), ids.end(), ids.begin(), [](unsigned char c) { return char(std::toupper(c)); });
    if (ids.find("CLASS_FE&SUBCLASS_03") == std::string::npos &&
        ids.find("MS_COMP_WINUSB") == std::string::npos) return nullptr;
    if (vid == 0x0b21 && pid == 0x0039) return "YOKOGAWA GS210 USBTMC";
    if (vid == 0x0957 && pid == 0x0a07) return "Agilent 34411A USBTMC";
    if (vid == 0x2a8d && pid == 0x0101) return "Keysight 34465A USBTMC";
    return nullptr;
}

// Never replace an assigned manufacturer/third-party driver implicitly. A
// missing driver may be configured with WinUSB; an existing WinUSB may be fixed.
inline bool setup_preserves_driver(const std::string& driver) {
    if (driver.empty()) return false;
    std::string service = driver;
    std::transform(service.begin(), service.end(), service.begin(), [](unsigned char c) { return char(std::toupper(c)); });
    return service != "WINUSB";
}

inline bool setup_self_test_request(std::wstring arguments) {
    auto first = arguments.find_first_not_of(L" \t\r\n");
    if (first == std::wstring::npos) return false;
    auto last = arguments.find_last_not_of(L" \t\r\n");
    return arguments.substr(first, last - first + 1) == L"--self-test";
}
