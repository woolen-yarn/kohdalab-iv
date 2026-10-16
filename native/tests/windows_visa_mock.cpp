// SPDX-License-Identifier: MIT
// Minimal fake VISA DLL for visa_helper protocol tests.  It is never packaged.
#ifdef _WIN32
#include <windows.h>
#include <cstdint>
#include <cstring>

using ViStatus = std::int32_t;
using ViSession = std::uint32_t;
using ViFindList = std::uint32_t;
using ViUInt32 = std::uint32_t;
using ViAccessMode = std::uint32_t;
using ViAttr = std::uint32_t;
using ViAttrState = uintptr_t;
constexpr ViStatus VI_SUCCESS = 0;
constexpr ViStatus VI_ERROR_RSRC_NFOUND = static_cast<ViStatus>(0xBFFF0011L);

static char last_write[16385]{};
static ViUInt32 last_write_size = 0;
static bool active_usb = false;
static const char *resources[] = {
    "USB0::0x0957::0x0A07::MOCK34411::0::INSTR",
    "GPIB0::7::INSTR",
};

extern "C" __declspec(dllexport) ViStatus __stdcall viOpenDefaultRM(ViSession *out) { *out = 100; return VI_SUCCESS; }
extern "C" __declspec(dllexport) ViStatus __stdcall viFindRsrc(ViSession, const char *pattern, ViFindList *find, ViUInt32 *count, char *out) {
    if (std::strncmp(pattern, "USB", 3) == 0) { *find = 200; *count = 1; std::strcpy(out, resources[0]); return VI_SUCCESS; }
    if (std::strncmp(pattern, "GPIB", 4) == 0) { *find = 201; *count = 1; std::strcpy(out, resources[1]); return VI_SUCCESS; }
    return VI_ERROR_RSRC_NFOUND;
}
extern "C" __declspec(dllexport) ViStatus __stdcall viFindNext(ViFindList, char *) { return VI_ERROR_RSRC_NFOUND; }
extern "C" __declspec(dllexport) ViStatus __stdcall viOpen(ViSession, const char *resource, ViAccessMode mode, ViUInt32 timeout, ViSession *out) {
    if (mode != 1 || timeout != 2000 || (std::strcmp(resource, "USB0::0x0957::0x0A07::MOCK34411::INSTR") && std::strcmp(resource, resources[1]))) return VI_ERROR_RSRC_NFOUND;
    active_usb = resource[0] == 'U';
    *out = 300; return VI_SUCCESS;
}
extern "C" __declspec(dllexport) ViStatus __stdcall viClose(ViSession) { return VI_SUCCESS; }
extern "C" __declspec(dllexport) ViStatus __stdcall viSetAttribute(ViSession, ViAttr attr, ViAttrState state) {
    if ((attr == 0x3FFF0016 && state == 1) || (attr == 0x3FFF0038 && state == 0) || attr == 0x3FFF001A) return VI_SUCCESS;
    return static_cast<ViStatus>(0xBFFF0000L);
}
extern "C" __declspec(dllexport) ViStatus __stdcall viWrite(ViSession, unsigned char *data, ViUInt32 count, ViUInt32 *done) {
    if (count > sizeof(last_write) - 1) return static_cast<ViStatus>(0xBFFF0000L);
    std::memcpy(last_write, data, count); last_write[count] = '\0'; last_write_size = count;
    if (std::strcmp(last_write, "WRITEFAIL\n") == 0) return static_cast<ViStatus>(0xBFFF0000L);
    if (std::strcmp(last_write, "SHORTWRITE\n") == 0) { *done = count ? count - 1 : 0; return VI_SUCCESS; }
    *done = count; return VI_SUCCESS;
}
extern "C" __declspec(dllexport) ViStatus __stdcall viRead(ViSession, unsigned char *data, ViUInt32 count, ViUInt32 *done) {
    if (std::strcmp(last_write, "TIMEOUT?\n") == 0) return static_cast<ViStatus>(0xBFFF0015L);
    if (std::strcmp(last_write, "OVERSIZE?\n") == 0) { std::memset(data, 'X', count); *done = count; return 0x3FFF0006; }
    const char *response = std::strcmp(last_write, "*IDN?\n") == 0 ? "MOCK,VISA,1,1\n" : "MOCK\n";
    std::size_t size = std::strlen(response);
    if (count < size) return static_cast<ViStatus>(0xBFFF0000L);
    std::memcpy(data, response, size); *done = static_cast<ViUInt32>(size); return VI_SUCCESS;
}
extern "C" __declspec(dllexport) ViStatus __stdcall viClear(ViSession) { return VI_SUCCESS; }
extern "C" __declspec(dllexport) ViStatus __stdcall viGpibControlREN(ViSession, std::uint16_t mode) {
    if ((active_usb && (mode == 0 || mode == 1)) || (!active_usb && mode == 6)) return VI_SUCCESS;
    return static_cast<ViStatus>(0xBFFF0000L);
}

BOOL WINAPI DllMain(HINSTANCE, DWORD, LPVOID) { return TRUE; }
#endif
