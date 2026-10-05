// SPDX-License-Identifier: MIT
#pragma once
#include <string>
#include <algorithm>
#include <cctype>

inline const char* setup_device_name(unsigned vid, unsigned pid, bool composite,
                                     unsigned interface_number, const std::string& compatible) {
    if (composite && interface_number != 0) return nullptr;
    if (vid == 0x3923 && pid == 0x7618) return "NI GPIB-USB-HS+";
    if (vid == 0x0957 && (pid == 0x0518 || pid == 0x0718)) return "Agilent 82357B";
    // The same Yokogawa product ID must never authorize storage-mode replacement.
    std::string ids = compatible;
    std::transform(ids.begin(), ids.end(), ids.begin(), [](unsigned char c) { return char(std::toupper(c)); });
    if (ids.find("CLASS_FE&SUBCLASS_03") == std::string::npos &&
        ids.find("MS_COMP_WINUSB") == std::string::npos) return nullptr;
    if (vid == 0x0b21 && pid == 0x0039) return "YOKOGAWA GS210 USBTMC";
    if (vid == 0x0957 && pid == 0x0a07) return "Agilent 34411A USBTMC";
    return nullptr;
}
