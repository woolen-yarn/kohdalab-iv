// SPDX-License-Identifier: MIT
// Dedicated setup: a fixed USB allowlist, inbox WinUSB, and libwdi package signing.
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <shellapi.h>
#include <cfgmgr32.h>
#include <filesystem>
#include <fstream>
#include <sstream>
#include <set>
#include <algorithm>
#include <cstdio>
#include <vector>
#include <stdexcept>
#include "libwdi.h"
#include "usb_setup_policy.hpp"

namespace fs = std::filesystem;
static std::string log_text;
static void progress(const std::string& text) {
    log_text += text + "\n";
    // QProcess supplies a stdout pipe even for this windowed executable.
    std::string line = text + "\n";
    DWORD written = 0;
    WriteFile(GetStdHandle(STD_OUTPUT_HANDLE), line.data(), DWORD(line.size()), &written, nullptr);
}
static std::wstring wide(const std::string& s) {
    if (s.empty()) return {};
    int size = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, s.data(), int(s.size()), nullptr, 0);
    if (!size) throw std::runtime_error("Invalid UTF-8 setup path");
    std::wstring result(size, 0);
    MultiByteToWideChar(CP_UTF8, 0, s.data(), int(s.size()), result.data(), size);
    return result;
}
static std::string utf8(const std::wstring& s) {
    int size = WideCharToMultiByte(CP_UTF8, 0, s.data(), int(s.size()), nullptr, 0, nullptr, nullptr);
    std::string result(size, 0);
    WideCharToMultiByte(CP_UTF8, 0, s.data(), int(s.size()), result.data(), size, nullptr, nullptr);
    return result;
}
static fs::path data_directory() {
    wchar_t value[32768];
    DWORD size = GetEnvironmentVariableW(L"LOCALAPPDATA", value, 32768);
    if (!size || size >= 32768) throw std::runtime_error("LOCALAPPDATA unavailable");
    fs::path result = fs::path(value) / L"KohdaLab IV";
    fs::create_directories(result);
    return result;
}
static void check(int code, const char* operation) {
    if (code != WDI_SUCCESS) throw std::runtime_error(std::string(operation) + ": " + wdi_strerror(code));
}
struct DeviceList {
    wdi_device_info* head = nullptr;
    explicit DeviceList(bool include_parents = false) {
        wdi_options_create_list options{};
        options.list_all = TRUE;
        options.list_hubs = include_parents;
        options.trim_whitespaces = TRUE;
        int code = wdi_create_list(&head, &options);
        if (code != WDI_ERROR_NO_DEVICE) check(code, "USB discovery");
    }
    ~DeviceList() { if (head) wdi_destroy_list(head); }
};
static const char* name(wdi_device_info* device) {
    return setup_device_name(device->vid, device->pid, device->is_composite,
        device->mi, device->compatible_id ? device->compatible_id : "");
}
static bool ready(wdi_device_info* device) {
    if (!device->driver || _stricmp(device->driver, "WinUSB") || !device->device_id) return false;
    DEVINST node = 0; ULONG status = 0, problem = 0;
    auto id = wide(device->device_id);
    if (CM_Locate_DevNodeW(&node, id.data(), CM_LOCATE_DEVNODE_NORMAL) != CR_SUCCESS ||
        CM_Get_DevNode_Status(&status, &problem, node, 0) != CR_SUCCESS ||
        problem != 0 || !(status & DN_STARTED)) return false;
    std::wstring key = L"SYSTEM\\CurrentControlSet\\Enum\\" + wide(device->device_id) + L"\\Device Parameters";
    wchar_t value[4096]{}; DWORD bytes = sizeof(value);
    auto flags = RRF_RT_REG_SZ | RRF_RT_REG_MULTI_SZ;
    LONG code = RegGetValueW(HKEY_LOCAL_MACHINE, key.c_str(), L"DeviceInterfaceGUIDs", flags, nullptr, value, &bytes);
    if (code != ERROR_SUCCESS) {
        bytes = sizeof(value);
        code = RegGetValueW(HKEY_LOCAL_MACHINE, key.c_str(), L"DeviceInterfaceGUID", flags, nullptr, value, &bytes);
    }
    GUID guid{};
    return code == ERROR_SUCCESS && value[0] && CLSIDFromString(value, &guid) == S_OK;
}
static bool administrator() {
    SID_IDENTIFIER_AUTHORITY authority = SECURITY_NT_AUTHORITY;
    PSID group = nullptr; BOOL member = FALSE;
    if (!AllocateAndInitializeSid(&authority, 2, SECURITY_BUILTIN_DOMAIN_RID,
        DOMAIN_ALIAS_RID_ADMINS, 0,0,0,0,0,0, &group)) return false;
    CheckTokenMembership(nullptr, group, &member); FreeSid(group);
    return member;
}
static std::string inf_name(wdi_device_info* device) {
    char value[64]; std::snprintf(value, sizeof(value), "kiv-%04x-%04x-%02x.inf", device->vid, device->pid, device->mi);
    return value;
}
static void prepare(wdi_device_info* device, const fs::path& directory) {
    wdi_options_prepare_driver options{};
    options.driver_type = WDI_WINUSB;
    char vendor[] = "KohdaLab";
    options.vendor_name = vendor;
    // Use an ASCII description from the allowlist, never device-provided INF text.
    wdi_device_info target = *device;
    target.desc = const_cast<char*>(name(device) ? name(device) : "Agilent 82357B");
    check(wdi_prepare_driver(&target, utf8(directory.wstring()).c_str(), inf_name(device).c_str(), &options), "WinUSB package preparation");
}
static int elevate(bool quiet, bool rescan = false) {
    wchar_t executable[32768]; GetModuleFileNameW(nullptr, executable, 32768);
    SHELLEXECUTEINFOW info{}; info.cbSize = sizeof(info); info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = L"runas"; info.lpFile = executable;
    info.lpParameters = rescan ? L"--install --ensure --rescan" :
        (quiet ? L"--install --ensure" : L"--install"); info.nShow = SW_SHOWNORMAL;
    if (!ShellExecuteExW(&info)) return 1;
    WaitForSingleObject(info.hProcess, INFINITE); DWORD code = 1;
    GetExitCodeProcess(info.hProcess, &code); CloseHandle(info.hProcess); return int(code);
}
struct SetupLock {
    HANDLE handle;
    SetupLock() : handle(CreateMutexW(nullptr, FALSE, L"Global\\KohdaLabIVUSBSetup")) {
        if (!handle) throw std::runtime_error("Cannot lock USB setup");
        DWORD result = WaitForSingleObject(handle, 0);
        if (result != WAIT_OBJECT_0 && result != WAIT_ABANDONED) {
            CloseHandle(handle);
            throw std::runtime_error("Another USB setup is still running; wait for it to finish");
        }
    }
    ~SetupLock() { ReleaseMutex(handle); CloseHandle(handle); }
};
static bool preserve_driver(wdi_device_info* device) {
    return setup_preserves_driver(device->driver ? device->driver : "");
}
static bool configured_device(wdi_device_info* device) {
    return ready(device) || preserve_driver(device);
}
int WINAPI wWinMain(HINSTANCE, HINSTANCE, PWSTR arguments, int) {
    bool quiet = std::wstring(arguments).find(L"--ensure") != std::wstring::npos;
    bool rescan = std::wstring(arguments).find(L"--rescan") != std::wstring::npos;
    if (std::wstring(arguments) == L"--self-test") {
        return setup_device_name(0x3923,0x7618,false,0,"") &&
            !setup_device_name(0x0957,0x0518,false,0,"") &&
            setup_device_name(0x0957,0x0718,false,0,"") &&
            !setup_device_name(0x0b21,0x0039,false,0,"USB\\Class_08") &&
            wdi_is_driver_supported(WDI_WINUSB, nullptr) &&
            wdi_is_file_embedded(nullptr, "installer_x64.exe") &&
            wdi_is_file_embedded(nullptr, "winusb.inf.in") &&
            wdi_is_file_embedded(nullptr, "winusb.cat.in") ? 0 : 1;
    }
    try {
        // Removing a device in Device Manager can leave the NI parent visible
        // while its communication child no longer exists. Driver enumeration
        // alone cannot reinstall a missing devnode; re-enumerate hardware first.
        {
            DeviceList initial(true);
            bool ni_parent = false, ni_communication = false, ni_vendor_parent = false;
            bool supported = false, configured = true;
            for (auto* device = initial.head; device; device = device->next) {
                bool parent = device->vid == 0x3923 && device->pid == 0x7618 &&
                    device->device_id && std::string(device->device_id).find("&MI_") == std::string::npos;
                if (!parent && name(device)) { supported = true; configured &= configured_device(device); }
                if (device->vid != 0x3923 || device->pid != 0x7618) continue;
                ni_parent |= parent;
                ni_vendor_parent |= parent && preserve_driver(device);
                ni_communication |= !parent && name(device) != nullptr;
            }
            // A normal Refresh must not prompt for administrator access when
            // all visible communication devices are already operational.
            if (supported && configured && !administrator()) rescan = false;
            rescan |= ni_parent && !ni_communication && !ni_vendor_parent;
        }
        if (rescan) {
            progress("Re-detecting removed USB devices");
            if (!administrator()) return elevate(quiet, true);
            DEVINST root = 0;
            if (CM_Locate_DevNodeW(&root, nullptr, CM_LOCATE_DEVNODE_NORMAL) != CR_SUCCESS ||
                CM_Reenumerate_DevNode(root, CM_REENUMERATE_SYNCHRONOUS) != CR_SUCCESS)
                throw std::runtime_error("Windows hardware rescan failed; reconnect the USB devices and retry");
        }
        progress("Enumerating Windows USB devices");
        DeviceList devices; bool pending = false, boot = false, found = false;
        for (auto* device = devices.head; device; device = device->next) {
            if (device->vid == 0x0957 && device->pid == 0x0518) { boot = true; continue; }
            if (!name(device)) continue;
            found = true; pending |= !configured_device(device);
            if (preserve_driver(device)) progress(std::string("Preserving assigned driver for ") + name(device) + "; use installed 64-bit VISA.");
        }
        if (!found) {
            if (boot) {
                progress("82357B boot mode (0957:0518) is unsupported; obtain authorized initialization separately, then retry without disconnecting adapter power");
                if (!quiet) MessageBoxW(nullptr,
                    L"The Agilent 82357B is in boot mode (0957:0518). KohdaLab IV supports only a pre-initialized 82357B (0957:0718). Obtain authorized initialization separately, then retry without disconnecting adapter power.",
                    L"KohdaLab USB Setup", MB_OK | MB_ICONINFORMATION);
                return 0;
            }
            progress("No supported USB instruments found; connect devices and retry Refresh");
            if (!quiet) MessageBoxW(nullptr, L"Connect supported instruments first. GS210 USB must be in TMC mode.", L"KohdaLab USB Setup", MB_OK);
            return 0;
        }
        progress("Checking WinUSB configuration");
        auto directory = data_directory();
        if (boot) progress("82357B boot mode (0957:0518) was skipped; other supported USB devices will still be configured");
        if (!pending) {
            progress("Existing driver assignments were preserved; WinUSB devices are ready and other assignments require a working official 64-bit VISA runtime.");
            if (!quiet) MessageBoxW(nullptr, L"Existing drivers were preserved. Open KohdaLab IV and click Refresh. Manufacturer drivers require an installed 64-bit VISA runtime.", L"KohdaLab USB Setup", MB_OK);
            return 0;
        }
        if (!administrator()) {
            progress("Waiting for Windows administrator approval and driver setup");
            return elevate(quiet);
        }
        SetupLock lock;
        auto drivers = directory / L"usb-drivers"; fs::create_directories(drivers);
        wdi_set_log_level(WDI_LOG_LEVEL_WARNING);
        std::set<std::string> installed;
        for (auto* device = devices.head; device; device = device->next) {
            auto* model = name(device);
            if (!model || configured_device(device) || !device->device_id || !installed.insert(device->device_id).second) continue;
            log_text += std::string("Configuring ") + model + ": " + device->device_id +
                "; previous driver=" + (device->driver ? device->driver : "none") + "\n";
            progress(std::string("Installing WinUSB for ") + model);
            ULONGLONG started = GetTickCount64();
            prepare(device, drivers);
            progress("WinUSB package prepared in " + std::to_string(GetTickCount64() - started) + " ms");
            wdi_options_install_driver options{}; options.pending_install_timeout = 60000;
            check(wdi_install_driver(device, utf8(drivers.wstring()).c_str(), inf_name(device).c_str(), &options), "WinUSB assignment");
            progress("WinUSB installation completed in " + std::to_string(GetTickCount64() - started) + " ms");
        }
        progress("Verifying installed USB drivers");
        bool verified = false;
        // Driver installation can finish before PnP starts the device and
        // publishes its interface. Never enable discovery during that interval.
        for (int attempt = 0; attempt < 30; ++attempt) {
            DeviceList refreshed;
            std::set<std::string> active;
            bool pending_device = false;
            for (auto* device = refreshed.head; device; device = device->next) {
                if (!name(device)) continue;
                pending_device |= !configured_device(device);
                if (ready(device) && device->device_id) active.insert(device->device_id);
            }
            verified = !pending_device &&
                std::includes(active.begin(), active.end(), installed.begin(), installed.end());
            if (verified) break;
            Sleep(500);
        }
        if (!verified) throw std::runtime_error("WinUSB is not active yet; reconnect the device or restart Windows, then retry Refresh");
        log_text += "Setup completed. Windows may require reconnect or restart.\n";
        std::ofstream(directory / L"usb-setup.log", std::ios::app) << log_text;
        if (!quiet) MessageBoxW(nullptr, wide(log_text + "Open KohdaLab IV and click Refresh.").c_str(), L"KohdaLab USB Setup", MB_OK);
        return 0;
    } catch (const std::exception& error) {
        log_text += std::string("Setup failed: ") + error.what() + "\n";
        try { std::ofstream(data_directory() / L"usb-setup.log", std::ios::app) << log_text; } catch (...) {}
        MessageBoxW(nullptr, wide(log_text + "Details: LOCALAPPDATA/KohdaLab IV/usb-setup.log").c_str(), L"KohdaLab USB Setup", MB_OK | MB_ICONERROR);
        return 1;
    }
}
