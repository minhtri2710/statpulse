/*
 * Apple System Management Control (SMC) Tool
 * Based on smcFanControl by devnull & Hendrik Holtmann
 * GPL License
 * 
 * Modified for standalone fan speed control helper
 */

#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <unistd.h>
#include <pthread.h>
#include <IOKit/IOKitLib.h>
#include <IOKit/ps/IOPowerSources.h>
#include <IOKit/ps/IOPSKeys.h>
#include <CoreFoundation/CoreFoundation.h>
#include "smc.h"

UInt32 _strtoul(char *str, int size, int base)
{
    UInt32 total = 0;
    int i;

    for (i = 0; i < size; i++)
    {
        if (base == 16)
            total += str[i] << (size - 1 - i) * 8;
        else
           total += ((unsigned char) (str[i]) << (size - 1 - i) * 8);
    }
    return total;
}

void _ultostr(char *str, UInt32 val)
{
    str[0] = '\0';
    sprintf(str, "%c%c%c%c",
            (unsigned int) val >> 24,
            (unsigned int) val >> 16,
            (unsigned int) val >> 8,
            (unsigned int) val);
}

float _strtof(unsigned char *str, int size, int e)
{
    float total = 0;
    int i;
    
    for (i = 0; i < size; i++)
    {
        if (i == (size - 1))
            total += (str[i] & 0xff) >> e;
        else
            total += str[i] << (size - 1 - i) * (8 - e);
    }
    
    total += (str[size-1] & 0x03) * 0.25;
    return total;
}

float getFloatFromVal(SMCVal_t val)
{
    float fval = -1.0f;

    if (val.dataSize > 0)
    {
        if (strcmp(val.dataType, DATATYPE_FLT) == 0 && val.dataSize == 4) {
             memcpy(&fval, val.bytes, sizeof(float));
        }
        else if (strcmp(val.dataType, DATATYPE_FPE2) == 0 && val.dataSize == 2) {
             fval = _strtof(val.bytes, val.dataSize, 2);
        }
        else if (strcmp(val.dataType, DATATYPE_SP78) == 0 && val.dataSize == 2) {
             short raw = (short)(((unsigned short)val.bytes[0] << 8) | (unsigned short)val.bytes[1]);
             fval = (float)raw / 256.0f;
        }
        else if (strcmp(val.dataType, DATATYPE_UINT16) == 0 && val.dataSize == 2) {
             fval = (float)_strtoul((char *)val.bytes, val.dataSize, 10);
        }
        else if (strcmp(val.dataType, DATATYPE_UINT8) == 0 && val.dataSize == 1) {
             fval = (float)_strtoul((char *)val.bytes, val.dataSize, 10);
        }
    }
    return fval;
}

// Central transport check: a call succeeds only when the IOKit transport AND
// the SMC firmware both report success. The firmware result byte is folded into
// the return value so no caller can mistake a rejected command for success:
//   0x00 -> kIOReturnSuccess, 0x84 (key not found) -> kIOReturnNotFound,
//   any other nonzero byte -> kIOReturnError.
kern_return_t SMCCall(int index, SMCKeyData_t *inputStructure, SMCKeyData_t *outputStructure, io_connect_t conn)
{
    size_t structureInputSize = sizeof(SMCKeyData_t);
    size_t structureOutputSize = sizeof(SMCKeyData_t);

    kern_return_t result = IOConnectCallStructMethod(conn, index, inputStructure, structureInputSize, outputStructure, &structureOutputSize);
    if (result != kIOReturnSuccess)
        return result;

    if (structureOutputSize < offsetof(SMCKeyData_t, result) + sizeof(outputStructure->result))
        return kIOReturnError;

    if (inputStructure->data8 == SMC_CMD_READ_BYTES)
    {
        // A short reply that never delivered the requested bytes cannot
        // confirm anything; reject it instead of trusting zero-filled data.
        if (inputStructure->keyInfo.dataSize > sizeof(outputStructure->bytes))
            return kIOReturnError;
        if (structureOutputSize < offsetof(SMCKeyData_t, bytes) + inputStructure->keyInfo.dataSize)
            return kIOReturnError;
    }

    switch ((uint8_t)outputStructure->result)
    {
        case 0x00:
            return kIOReturnSuccess;
        case 0x84:
            return kIOReturnNotFound;
        default:
            return kIOReturnError;
    }
}

kern_return_t SMCOpen(io_connect_t *conn)
{
    kern_return_t result;
    mach_port_t   masterPort;
    io_iterator_t iterator;
    io_object_t   device;
    
    IOMasterPort(MACH_PORT_NULL, &masterPort);
    
    CFMutableDictionaryRef matchingDictionary = IOServiceMatching("AppleSMC");
    result = IOServiceGetMatchingServices(masterPort, matchingDictionary, &iterator);
    if (result != kIOReturnSuccess)
    {
        fprintf(stderr, "Error: IOServiceGetMatchingServices() = %08x\n", result);
        return result;
    }
    
    device = IOIteratorNext(iterator);
    IOObjectRelease(iterator);
    if (device == 0)
    {
        fprintf(stderr, "Error: no SMC found\n");
        return kIOReturnNotFound;
    }
    
    result = IOServiceOpen(device, mach_task_self(), 0, conn);
    IOObjectRelease(device);
    if (result != kIOReturnSuccess)
    {
        fprintf(stderr, "Error: IOServiceOpen() = %08x\n", result);
        return result;
    }
    
    return kIOReturnSuccess;
}

#define SMC_KEYINFO_CACHE_CAP 128

static struct {
    UInt32 key;
    SMCKeyData_keyInfo_t info;
} g_key_info_cache[SMC_KEYINFO_CACHE_CAP];
static int g_key_info_count = 0;
static pthread_mutex_t g_key_info_mutex = PTHREAD_MUTEX_INITIALIZER;
static char g_fan_mode_tmpl[8] = "";

void SMCClearKeyInfoCache(void)
{
    pthread_mutex_lock(&g_key_info_mutex);
    g_key_info_count = 0;
    g_fan_mode_tmpl[0] = '\0';
    pthread_mutex_unlock(&g_key_info_mutex);
}

kern_return_t SMCGetKeyInfo(UInt32 key, SMCKeyData_keyInfo_t *keyInfo, io_connect_t conn)
{
    pthread_mutex_lock(&g_key_info_mutex);
    for (int i = 0; i < g_key_info_count; i++)
    {
        if (g_key_info_cache[i].key == key)
        {
            *keyInfo = g_key_info_cache[i].info;
            pthread_mutex_unlock(&g_key_info_mutex);
            return kIOReturnSuccess;
        }
    }
    pthread_mutex_unlock(&g_key_info_mutex);

    SMCKeyData_t inputStructure;
    SMCKeyData_t outputStructure;
    kern_return_t result;

    memset(&inputStructure, 0, sizeof(SMCKeyData_t));
    memset(&outputStructure, 0, sizeof(SMCKeyData_t));

    inputStructure.key = key;
    inputStructure.data8 = SMC_CMD_READ_KEYINFO;

    result = SMCCall(KERNEL_INDEX_SMC, &inputStructure, &outputStructure, conn);
    if (result == kIOReturnSuccess)
    {
        *keyInfo = outputStructure.keyInfo;
        if (outputStructure.keyInfo.dataSize > 0)
        {
            pthread_mutex_lock(&g_key_info_mutex);
            int found = 0;
            for (int i = 0; i < g_key_info_count; i++)
            {
                if (g_key_info_cache[i].key == key)
                {
                    found = 1;
                    break;
                }
            }
            if (!found && g_key_info_count < SMC_KEYINFO_CACHE_CAP)
            {
                g_key_info_cache[g_key_info_count].key = key;
                g_key_info_cache[g_key_info_count].info = outputStructure.keyInfo;
                g_key_info_count++;
            }
            pthread_mutex_unlock(&g_key_info_mutex);
        }
    }

    return result;
}

kern_return_t SMCReadKey(UInt32Char_t key, SMCVal_t *val, io_connect_t conn)
{
    kern_return_t result;
    SMCKeyData_t  inputStructure;
    SMCKeyData_t  outputStructure;
    
    memset(&inputStructure, 0, sizeof(SMCKeyData_t));
    memset(&outputStructure, 0, sizeof(SMCKeyData_t));
    memset(val, 0, sizeof(SMCVal_t));
    
    inputStructure.key = _strtoul(key, 4, 16);
    sprintf(val->key, "%s", key);
    
    result = SMCGetKeyInfo(inputStructure.key, &outputStructure.keyInfo, conn);
    if (result != kIOReturnSuccess)
    {
        return result;
    }
    
    val->dataSize = outputStructure.keyInfo.dataSize;
    if (val->dataSize > sizeof(val->bytes))
        return kIOReturnError;
    _ultostr(val->dataType, outputStructure.keyInfo.dataType);
    inputStructure.keyInfo.dataSize = val->dataSize;
    inputStructure.data8 = SMC_CMD_READ_BYTES;
    
    result = SMCCall(KERNEL_INDEX_SMC, &inputStructure, &outputStructure, conn);
    if (result != kIOReturnSuccess)
    {
        return result;
    }
    
    memcpy(val->bytes, outputStructure.bytes, sizeof(outputStructure.bytes));
    
    return kIOReturnSuccess;
}

kern_return_t SMCWriteKey(SMCVal_t writeVal, io_connect_t conn)
{
    kern_return_t result;
    SMCKeyData_t  inputStructure;
    SMCKeyData_t  outputStructure;
    SMCVal_t      readVal;
    
    result = SMCReadKey(writeVal.key, &readVal, conn);
    if (result != kIOReturnSuccess)
    {
        fprintf(stderr, "Error: SMCReadKey failed for %s: %08x\n", writeVal.key, result);
        return result;
    }
    
    if (readVal.dataSize != writeVal.dataSize)
    {
        fprintf(stderr, "Error: dataSize mismatch (read=%u, write=%u)\n", readVal.dataSize, writeVal.dataSize);
        return kIOReturnError;
    }
    
    memset(&inputStructure, 0, sizeof(SMCKeyData_t));
    memset(&outputStructure, 0, sizeof(SMCKeyData_t));
    
    inputStructure.key = _strtoul(writeVal.key, 4, 16);
    inputStructure.data8 = SMC_CMD_WRITE_BYTES;
    inputStructure.keyInfo.dataSize = writeVal.dataSize;
    memcpy(inputStructure.bytes, writeVal.bytes, sizeof(writeVal.bytes));
    
    result = SMCCall(KERNEL_INDEX_SMC, &inputStructure, &outputStructure, conn);
    if (result != kIOReturnSuccess)
    {
        fprintf(stderr, "Error: SMCCall write failed: %08x\n", result);
        return result;
    }
    
    return kIOReturnSuccess;
}

// -1 means the FNum read failed or was malformed; a successfully read 0 is a
// real "zero fans" answer and must not be conflated with an error. Only known
// unsigned widths are decoded, and the count must name addressable fan IDs.
int getFanCount(io_connect_t conn)
{
    SMCVal_t val;
    kern_return_t result = SMCReadKey("FNum", &val, conn);
    if (result != kIOReturnSuccess)
        return -1;

    UInt32 count;
    if (strcmp(val.dataType, DATATYPE_UINT8) == 0 && val.dataSize == 1)
        count = val.bytes[0];
    else if (strcmp(val.dataType, DATATYPE_UINT16) == 0 && val.dataSize == 2)
        count = ((UInt32)val.bytes[0] << 8) | (UInt32)val.bytes[1];
    else if (strcmp(val.dataType, DATATYPE_UINT32) == 0 && val.dataSize == 4)
        count = ((UInt32)val.bytes[0] << 24) | ((UInt32)val.bytes[1] << 16)
              | ((UInt32)val.bytes[2] << 8) | (UInt32)val.bytes[3];
    else
        return -1;

    if (count > 10)
        return -1;
    return (int)count;
}

float getFanMinSpeed(int fanNum, io_connect_t conn)
{
    SMCVal_t val;
    char key[8];
    snprintf(key, sizeof key, "F%dMn", fanNum);

    kern_return_t result = SMCReadKey(key, &val, conn);
    if (result != kIOReturnSuccess)
        return -1;

    return getFloatFromVal(val);
}

float getFanMaxSpeed(int fanNum, io_connect_t conn)
{
    SMCVal_t val;
    char key[8];
    snprintf(key, sizeof key, "F%dMx", fanNum);

    kern_return_t result = SMCReadKey(key, &val, conn);
    if (result != kIOReturnSuccess)
        return -1;

    return getFloatFromVal(val);
}

// Mode-key casing varies by silicon: F%dMd (Intel/M1-M4), F%dmd (M5).
// Only a positively recognized casing is cached: a transient probe failure
// returns NULL so the next call can retry instead of pinning a wrong guess.
static const char *fanModeTemplate(io_connect_t conn)
{
    pthread_mutex_lock(&g_key_info_mutex);
    if (g_fan_mode_tmpl[0] == '\0')
    {
        pthread_mutex_unlock(&g_key_info_mutex);
        SMCKeyData_keyInfo_t ki;
        if (SMCGetKeyInfo(_strtoul("F0Md", 4, 16), &ki, conn) == kIOReturnSuccess && ki.dataSize > 0)
        {
            pthread_mutex_lock(&g_key_info_mutex);
            strcpy(g_fan_mode_tmpl, "F%dMd");
        }
        else if (SMCGetKeyInfo(_strtoul("F0md", 4, 16), &ki, conn) == kIOReturnSuccess && ki.dataSize > 0)
        {
            pthread_mutex_lock(&g_key_info_mutex);
            strcpy(g_fan_mode_tmpl, "F%dmd");
        }
        else
        {
            return NULL;
        }
    }
    pthread_mutex_unlock(&g_key_info_mutex);
    return g_fan_mode_tmpl;
}

static int fanModeKey(char *buf, int fanNum, io_connect_t conn)
{
    const char *tmpl = fanModeTemplate(conn);
    if (tmpl == NULL || fanNum < 0 || fanNum > 9)
        return 0;
    return snprintf(buf, 8, tmpl, fanNum) == 4;
}

// Whether this machine exposes the Ftst force-test key (absent on M5).
// kIOReturnNotFound on the presence probe is the only answer treated as
// "absent"; every other probe result keeps retrying instead of caching.
static int ftstPresent(io_connect_t conn)
{
    SMCKeyData_keyInfo_t ki;
    kern_return_t result = SMCGetKeyInfo(_strtoul("Ftst", 4, 16), &ki, conn);
    if (result == kIOReturnNotFound)
        return 0;
    if (result != kIOReturnSuccess)
        return -1;
    return ki.dataSize == 1 ? 1 : -1;
}

// Write the optional Ftst force-test key with a checked result: a present key
// that rejects the write is a real error, never "optional and absent".
static kern_return_t writeFtst(int value, io_connect_t conn)
{
    SMCKeyData_keyInfo_t ki;
    UInt32 key = _strtoul("Ftst", 4, 16);
    kern_return_t result = SMCGetKeyInfo(key, &ki, conn);
    if (result != kIOReturnSuccess)
        return result;
    if (ki.dataSize != 1)
        return kIOReturnError;

    SMCKeyData_t in, out;
    memset(&in, 0, sizeof(in));
    memset(&out, 0, sizeof(out));
    in.key = key;
    in.data8 = SMC_CMD_WRITE_BYTES;
    in.keyInfo.dataSize = 1;
    in.bytes[0] = (UInt8)value;
    return SMCCall(KERNEL_INDEX_SMC, &in, &out, conn);
}

// Write the fan mode key and return the checked call status. SMCCall folds the
// firmware result byte into the return value, so a firmware rejection (e.g.
// 0x82, thermalmonitord holding SYSTEM mode) surfaces as an error instead of a
// silent no-op.
static kern_return_t writeFanModeRaw(int fanNum, int mode, io_connect_t conn)
{
    char keyStr[8];
    if (!fanModeKey(keyStr, fanNum, conn))
        return kIOReturnError;
    UInt32 key = _strtoul(keyStr, 4, 16);

    SMCKeyData_keyInfo_t ki;
    kern_return_t result = SMCGetKeyInfo(key, &ki, conn);
    if (result != kIOReturnSuccess)
        return result;
    if (ki.dataSize != 1)
        return kIOReturnError;

    SMCKeyData_t in, out;
    memset(&in, 0, sizeof(in));
    memset(&out, 0, sizeof(out));
    in.key = key;
    in.data8 = SMC_CMD_WRITE_BYTES;
    in.keyInfo.dataSize = 1;
    in.bytes[0] = (UInt8)mode;
    return SMCCall(KERNEL_INDEX_SMC, &in, &out, conn);
}

// Read the fan mode key, returning the raw byte (0/1/3) or -1 on failure.
static int readFanModeRaw(int fanNum, io_connect_t conn)
{
    char key[8];
    if (!fanModeKey(key, fanNum, conn))
        return -1;

    SMCVal_t val;
    if (SMCReadKey(key, &val, conn) != kIOReturnSuccess || val.dataSize != 1)
        return -1;
    return (int)(uint8_t)val.bytes[0];
}

// Take manual control of a fan so F%dTg writes stick, at any temperature.
//
// A direct mode=1 write is enough from AUTO. From SYSTEM (mode 3) the firmware
// either rejects the write (0x82) or accepts it but thermalmonitord reclaims it
// within a polling cycle, so the fan never actually leaves system control. We
// therefore VERIFY the mode actually stuck rather than trusting the write
// result; if it did not, we set Ftst=1 to suppress thermalmonitord and retry
// mode=1 until it holds (the daemon yields a few seconds after Ftst=1).
//
// Mechanism from agoodkind/macos-smc-fan (MIT).
kern_return_t unlockFanManual(int fanNum, io_connect_t conn)
{
    if (readFanModeRaw(fanNum, conn) == 1)
        return kIOReturnSuccess;

    // Phase 1: direct write, then confirm it took.
    writeFanModeRaw(fanNum, 1, conn);
    usleep(200000); // 0.2s: long enough for a reclaim to show up
    if (readFanModeRaw(fanNum, conn) == 1)
        return kIOReturnSuccess;

    // Phase 2: thermalmonitord is holding system mode. Suppress it via Ftst,
    // then retry mode=1 until it sticks.
    if (ftstPresent(conn) == 1)
    {
        writeFtst(1, conn);
        usleep(500000); // 0.5s
        for (int i = 0; i < 100; i++) // up to ~10s
        {
            writeFanModeRaw(fanNum, 1, conn);
            usleep(100000); // 0.1s
            if (readFanModeRaw(fanNum, conn) == 1)
                return kIOReturnSuccess;
        }
        writeFtst(0, conn);
    }

    return kIOReturnError;
}

kern_return_t setFanSpeed(int fanNum, int speed, io_connect_t conn)
{
    SMCVal_t val;
    char key[8];

    // Clamp to the fan's own reported envelope so a bad caller can't push a
    // nonsense target through the root helper. F{n}Mx is the hardware ceiling.
    if (speed < 0)
        speed = 0;
    float fmax = getFanMaxSpeed(fanNum, conn);
    if (fmax > 0 && speed > (int)fmax)
        speed = (int)fmax;

    // Take manual control. Direct mode=1 works from AUTO; if the firmware holds
    // SYSTEM mode (0x82) it falls back to the Ftst force-test unlock.
    if (unlockFanManual(fanNum, conn) != kIOReturnSuccess)
    {
        fprintf(stderr, "Error: could not take manual control of fan %d\n", fanNum);
        return kIOReturnError;
    }

    snprintf(key, sizeof key, "F%dTg", fanNum);
    
    kern_return_t result = SMCReadKey(key, &val, conn);
    if (result != kIOReturnSuccess)
    {
        fprintf(stderr, "Error: Cannot read %s\n", key);
        return result;
    }
    
    if (strcmp(val.dataType, DATATYPE_FLT) == 0 && val.dataSize == 4)
    {
        // float type (Apple Silicon)
        float fspeed = (float)speed;
        memcpy(val.bytes, &fspeed, sizeof(float));
    }
    else if (strcmp(val.dataType, DATATYPE_FPE2) == 0 && val.dataSize == 2)
    {
        // fpe2 encoding (Intel): value << 2, big endian
        UInt16 encoded = (UInt16)(speed << 2);
        val.bytes[0] = (encoded >> 8) & 0xFF;
        val.bytes[1] = encoded & 0xFF;
    }
    else
    {
        fprintf(stderr, "Error: Unknown type %s for %s\n", val.dataType, key);
        return kIOReturnError;
    }
    
    sprintf(val.key, "%s", key);
    
    result = SMCWriteKey(val, conn);
    return result;
}

// Restore a contiguous range of fan IDs to thermalmonitord control. Each fan
// is serviced independently: a fan already system-owned (0=AUTO, 3=SYSTEM)
// needs no write, an unreadable or rejecting fan records failure without
// stopping the rest. One takeover wait follows the last successful write, then
// every written fan is read back. The optional global Ftst force-test key is
// always released once, on success and on failure; a confirmed-absent probe is
// skipped while probe and clear failures propagate.
static kern_return_t restoreFanRange(int first, int count, io_connect_t conn)
{
    if (first < 0 || first > 9 || count < 0 || count > 10 - first)
        return kIOReturnError;

    kern_return_t result = kIOReturnSuccess;
    unsigned written = 0;
    for (int i = 0; i < count; i++)
    {
        int fan = first + i;
        int mode = readFanModeRaw(fan, conn);
        if (mode == 0 || mode == 3)
            continue;
        if (mode < 0 || writeFanModeRaw(fan, 0, conn) != kIOReturnSuccess)
        {
            result = kIOReturnError;
            continue;
        }
        written |= (1u << fan);
    }

    if (written != 0)
    {
        usleep(200000); // let thermalmonitord take over before readback
        for (int fan = first; fan < first + count; fan++)
        {
            if (!(written & (1u << fan)))
                continue;
            int mode = readFanModeRaw(fan, conn);
            if (mode != 0 && mode != 3)
                result = kIOReturnError;
        }
    }

    int ftst = ftstPresent(conn);
    kern_return_t clear = kIOReturnSuccess;
    if (ftst == 1)
        clear = writeFtst(0, conn);
    else if (ftst < 0)
        clear = kIOReturnError;
    if (result == kIOReturnSuccess)
        result = clear;
    return result;
}

kern_return_t setFanAuto(int fanNum, io_connect_t conn)
{
    return restoreFanRange(fanNum, 1, conn);
}

kern_return_t setAllFansAuto(io_connect_t conn)
{
    // A checked count bounds the range; zero fans is a real answer, not a
    // failure, and needs no mode or Ftst probes at all.
    int count = getFanCount(conn);
    if (count < 0)
        return kIOReturnError;
    if (count == 0)
        return kIOReturnSuccess;
    return restoreFanRange(0, count, conn);
}

static CFNumberRef battery_number(CFDictionaryRef properties, CFStringRef key)
{
    if (!properties) return NULL;

    CFTypeRef value = CFDictionaryGetValue(properties, key);
    if (value && CFGetTypeID(value) == CFNumberGetTypeID()) {
        return (CFNumberRef)value;
    }

    CFTypeRef battery_data = CFDictionaryGetValue(properties, CFSTR("BatteryData"));
    if (!battery_data || CFGetTypeID(battery_data) != CFDictionaryGetTypeID()) {
        return NULL;
    }

    value = CFDictionaryGetValue((CFDictionaryRef)battery_data, key);
    return value && CFGetTypeID(value) == CFNumberGetTypeID() ? (CFNumberRef)value : NULL;
}

static void read_battery_measurements(io_service_t service, BatteryInfoC *info)
{
    CFMutableDictionaryRef properties = NULL;
    if (!service || IORegistryEntryCreateCFProperties(service, &properties, kCFAllocatorDefault, 0) != KERN_SUCCESS) {
        return;
    }

    if (!info->has_cycle_count) {
        CFNumberRef cycle = battery_number(properties, CFSTR("CycleCount"));
        if (cycle && CFNumberGetValue(cycle, kCFNumberIntType, &info->cycle_count)) {
            info->has_cycle_count = 1;
        }
    }

    if (!info->has_temperature) {
        CFNumberRef temp = battery_number(properties, CFSTR("Temperature"));
        if (!temp) temp = battery_number(properties, CFSTR("VirtualTemperature"));
        if (temp) {
            int raw_temp = 0;
            if (CFNumberGetValue(temp, kCFNumberIntType, &raw_temp)) {
                // AppleSmartBattery reports Temperature in centi-Celsius (0.01°C).
                double celsius = (double)raw_temp / 100.0;
                if (celsius < -20.0 || celsius > 100.0) {
                    // Fallback for legacy deci-Kelvin (0.1K) if needed
                    celsius = ((double)raw_temp / 10.0) - 273.15;
                }
                if (celsius >= -20.0 && celsius <= 100.0) {
                    info->temperature = celsius;
                    info->has_temperature = 1;
                }
            }
        }
    }

    if (!info->has_power_watts) {
        CFNumberRef volt = battery_number(properties, CFSTR("Voltage"));
        if (!volt) volt = battery_number(properties, CFSTR("AppleRawBatteryVoltage"));
        CFNumberRef amp = battery_number(properties, CFSTR("InstantAmperage"));
        if (!amp) amp = battery_number(properties, CFSTR("Amperage"));

        int v_mv = 0;
        long long a_ma = 0;
        if (volt && amp
            && CFNumberGetValue(volt, kCFNumberIntType, &v_mv)
            && CFNumberGetValue(amp, kCFNumberLongLongType, &a_ma)
            && v_mv > 0) {
            double watts = ((double)v_mv / 1000.0) * ((double)llabs(a_ma) / 1000.0);
            info->power_watts = round(watts * 10.0) / 10.0;
            info->has_power_watts = 1;
        }
    }

    CFRelease(properties);
}

int fetch_battery_info(BatteryInfoC *info)
{
    if (!info) return 0;
    memset(info, 0, sizeof(BatteryInfoC));

    CFTypeRef snapshot = IOPSCopyPowerSourcesInfo();
    if (!snapshot) return -1;

    CFArrayRef sources = IOPSCopyPowerSourcesList(snapshot);
    if (!sources) {
        CFRelease(snapshot);
        return -1;
    }
    if (CFArrayGetCount(sources) == 0) {
        CFRelease(sources);
        CFRelease(snapshot);
        return 0;
    }

    CFDictionaryRef ps = IOPSGetPowerSourceDescription(snapshot, CFArrayGetValueAtIndex(sources, 0));
    if (!ps) {
        CFRelease(sources);
        CFRelease(snapshot);
        return -1;
    }
    {
        CFNumberRef cap = (CFNumberRef)CFDictionaryGetValue(ps, CFSTR(kIOPSCurrentCapacityKey));
        if (cap && CFNumberGetValue(cap, kCFNumberIntType, &info->percentage)) {
            info->has_percentage = 1;
        }

        CFBooleanRef charging = (CFBooleanRef)CFDictionaryGetValue(ps, CFSTR(kIOPSIsChargingKey));
        if (charging) {
            info->is_charging = CFBooleanGetValue(charging) ? 1 : 0;
            info->has_charging = 1;
        }
    }

    CFRelease(sources);
    CFRelease(snapshot);

    io_service_t service = IOServiceGetMatchingService(kIOMainPortDefault, IOServiceMatching("AppleSmartBattery"));
    if (service) {
        read_battery_measurements(service, info);
        IOObjectRelease(service);
    }

    if (!info->has_temperature || !info->has_power_watts || !info->has_cycle_count) {
        io_service_t pack = IOServiceGetMatchingService(kIOMainPortDefault, IOServiceMatching("AppleSmartBatteryPack"));
        if (pack) {
            read_battery_measurements(pack, info);
            IOObjectRelease(pack);
        }
    }

    return 1;
}
