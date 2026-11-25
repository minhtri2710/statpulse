#import <Foundation/Foundation.h>
#import <CoreWLAN/CoreWLAN.h>
#import <CoreAudio/CoreAudio.h>
#import <AudioToolbox/AudioServices.h>
#import <Carbon/Carbon.h>
#import <IOKit/IOKitLib.h>
#import <UserNotifications/UserNotifications.h>
#include <pthread.h>
#include <math.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

typedef struct IOReportSubscriptionRef* IOReportSubscriptionRef;
extern CFDictionaryRef IOReportCopyChannelsInGroup(CFStringRef group, CFStringRef subgroup, uint64_t a, uint64_t b, uint64_t c);
extern void IOReportMergeChannels(CFDictionaryRef a, CFDictionaryRef b, CFTypeRef null);
extern IOReportSubscriptionRef IOReportCreateSubscription(void* a, CFMutableDictionaryRef channels, CFMutableDictionaryRef* subbed, uint64_t cid, CFTypeRef b);
extern CFDictionaryRef IOReportCreateSamples(IOReportSubscriptionRef sub, CFMutableDictionaryRef subbed, CFTypeRef a);
extern CFDictionaryRef IOReportCreateSamplesDelta(CFDictionaryRef prev, CFDictionaryRef curr, CFTypeRef a);
extern CFStringRef IOReportChannelGetGroup(CFDictionaryRef ch);
extern CFStringRef IOReportChannelGetChannelName(CFDictionaryRef ch);
extern CFStringRef IOReportChannelGetUnitLabel(CFDictionaryRef ch);
extern int64_t IOReportSimpleGetIntegerValue(CFDictionaryRef ch, int32_t idx);
extern int32_t IOReportStateGetCount(CFDictionaryRef ch);
extern CFStringRef IOReportStateGetNameForIndex(CFDictionaryRef ch, int32_t idx);
extern int64_t IOReportStateGetResidency(CFDictionaryRef ch, int32_t idx);

typedef struct {
    double gpu_watts;
    double cpu_watts;
    double ane_watts;
    double dram_watts;
    uint32_t e_mhz;
    uint32_t p_mhz;
    uint32_t gpu_mhz;
    uint8_t e_active_pct;
    uint8_t p_active_pct;
    uint8_t gpu_active_pct;
    uint8_t valid;
} AppleSiliconMetricsC;

typedef struct {
    char channel[48];
    char phy_mode[24];
    char security[32];
    uint32_t tx_rate_mbps;
    int32_t rssi_dbm;
    int32_t noise_dbm;
    int32_t snr_db;
    uint8_t connected;
} WifiNativeC;

static pthread_mutex_t g_ior_mutex = PTHREAD_MUTEX_INITIALIZER;
static IOReportSubscriptionRef g_sub = NULL;
static CFMutableDictionaryRef g_subbed = NULL;
static CFDictionaryRef g_prev_sample = NULL;
static uint64_t g_prev_ns = 0;
static AppleSiliconMetricsC g_last_metrics = {0};

static uint32_t g_e_freqs_mhz[32];
static int g_e_freq_count = 0;
static uint32_t g_p_freqs_mhz[32];
static int g_p_freq_count = 0;
static uint32_t g_gpu_freqs_mhz[32];
static int g_gpu_freq_count = 0;
static int g_freqs_loaded = 0;

static int load_pmgr_freqs(io_object_t obj, CFStringRef prop, uint32_t *out_mhz, int max_n) {
    CFTypeRef val = IORegistryEntryCreateCFProperty(obj, prop, kCFAllocatorDefault, 0);
    if (!val) return 0;
    int count = 0;
    if (CFGetTypeID(val) == CFDataGetTypeID()) {
        CFDataRef data = (CFDataRef)val;
        CFIndex len = CFDataGetLength(data);
        const uint32_t *words = (const uint32_t *)CFDataGetBytePtr(data);
        CFIndex pairs = len / 8;
        for (CFIndex i = 0; i < pairs && count < max_n; i++) {
            uint32_t raw = words[i * 2];
            if (raw == 0) continue;
            uint32_t mhz = (raw > 10000000U) ? (raw / 1000000U) : (raw / 1000U);
            if (mhz > 0) {
                out_mhz[count++] = mhz;
            }
        }
    }
    CFRelease(val);
    return count;
}

static void ensure_pmgr_freqs_loaded(void) {
    if (g_freqs_loaded) return;
    g_freqs_loaded = 1;
    io_iterator_t iter = 0;
    if (IOServiceGetMatchingServices(kIOMainPortDefault, IOServiceMatching("AppleARMIODevice"), &iter) != KERN_SUCCESS) {
        return;
    }
    io_object_t obj;
    while ((obj = IOIteratorNext(iter)) != 0) {
        io_name_t name = {0};
        IORegistryEntryGetName(obj, name);
        if (strcmp(name, "pmgr") == 0) {
            g_e_freq_count = load_pmgr_freqs(obj, CFSTR("voltage-states1-sram"), g_e_freqs_mhz, 32);
            g_p_freq_count = load_pmgr_freqs(obj, CFSTR("voltage-states5-sram"), g_p_freqs_mhz, 32);
            g_gpu_freq_count = load_pmgr_freqs(obj, CFSTR("voltage-states9-sram"), g_gpu_freqs_mhz, 32);
            IOObjectRelease(obj);
            break;
        }
        IOObjectRelease(obj);
    }
    IOObjectRelease(iter);
}

static void accumulate_cluster_states(
    CFDictionaryRef ch,
    const uint32_t *freqs_mhz,
    int freq_count,
    int64_t *idle_acc,
    int64_t *active_acc,
    double *weighted_mhz_acc
) {
    int32_t cnt = IOReportStateGetCount(ch);
    int active_idx = 0;
    for (int32_t i = 0; i < cnt; i++) {
        int64_t res = IOReportStateGetResidency(ch, i);
        CFStringRef sname_cf = IOReportStateGetNameForIndex(ch, i);
        NSString *sname = (__bridge NSString *)sname_cf;
        BOOL is_idle = sname && ([sname isEqualToString:@"IDLE"] || [sname isEqualToString:@"OFF"] || [sname isEqualToString:@"DOWN"]);
        if (is_idle) {
            if (res > 0) *idle_acc += res;
        } else {
            if (res > 0) {
                *active_acc += res;
                if (freq_count > 0) {
                    int idx = active_idx < freq_count ? active_idx : (freq_count - 1);
                    *weighted_mhz_acc += (double)res * (double)freqs_mhz[idx];
                }
            }
            active_idx++;
        }
    }
}

int sp_sample_apple_silicon_metrics(AppleSiliconMetricsC *out) {
    if (!out) return 0;
    memset(out, 0, sizeof(*out));

    pthread_mutex_lock(&g_ior_mutex);
    ensure_pmgr_freqs_loaded();

    if (!g_sub) {
        CFDictionaryRef energy = IOReportCopyChannelsInGroup(CFSTR("Energy Model"), NULL, 0, 0, 0);
        if (!energy) {
            pthread_mutex_unlock(&g_ior_mutex);
            return 0;
        }
        CFMutableDictionaryRef merged = CFDictionaryCreateMutableCopy(kCFAllocatorDefault, 0, energy);
        CFRelease(energy);
        if (!merged) {
            pthread_mutex_unlock(&g_ior_mutex);
            return 0;
        }
        CFDictionaryRef cpuStats = IOReportCopyChannelsInGroup(CFSTR("CPU Stats"), CFSTR("CPU Complex Performance States"), 0, 0, 0);
        if (cpuStats) {
            IOReportMergeChannels(merged, cpuStats, NULL);
            CFRelease(cpuStats);
        }
        CFDictionaryRef gpuStats = IOReportCopyChannelsInGroup(CFSTR("GPU Stats"), CFSTR("GPU Performance States"), 0, 0, 0);
        if (gpuStats) {
            IOReportMergeChannels(merged, gpuStats, NULL);
            CFRelease(gpuStats);
        }
        g_sub = IOReportCreateSubscription(NULL, merged, &g_subbed, 0, NULL);
        CFRelease(merged);
        if (!g_sub || !g_subbed) {
            pthread_mutex_unlock(&g_ior_mutex);
            return 0;
        }
    }

    uint64_t now_ns = clock_gettime_nsec_np(CLOCK_UPTIME_RAW);
    if (!g_prev_sample) {
        g_prev_sample = IOReportCreateSamples(g_sub, g_subbed, NULL);
        g_prev_ns = now_ns;
        pthread_mutex_unlock(&g_ior_mutex);
        return 0;
    }

    if (now_ns > g_prev_ns && (now_ns - g_prev_ns) < 120000000ULL && g_last_metrics.valid) {
        *out = g_last_metrics;
        pthread_mutex_unlock(&g_ior_mutex);
        return 1;
    }

    CFDictionaryRef curr = IOReportCreateSamples(g_sub, g_subbed, NULL);
    if (!curr) {
        pthread_mutex_unlock(&g_ior_mutex);
        return 0;
    }

    double dt_sec = (now_ns > g_prev_ns) ? ((double)(now_ns - g_prev_ns) / 1e9) : 0.0;
    if (dt_sec < 0.02) {
        CFRelease(curr);
        if (g_last_metrics.valid) {
            *out = g_last_metrics;
            pthread_mutex_unlock(&g_ior_mutex);
            return 1;
        }
        pthread_mutex_unlock(&g_ior_mutex);
        return 0;
    }

    CFDictionaryRef delta = IOReportCreateSamplesDelta(g_prev_sample, curr, NULL);
    CFRelease(g_prev_sample);
    g_prev_sample = curr;
    g_prev_ns = now_ns;
    if (!delta) {
        pthread_mutex_unlock(&g_ior_mutex);
        return 0;
    }

    AppleSiliconMetricsC res = {0};
    int64_t e_idle = 0, e_active = 0;
    double e_wmhz = 0.0;
    int64_t p_idle = 0, p_active = 0;
    double p_wmhz = 0.0;
    int64_t g_idle = 0, g_active = 0;
    double g_wmhz = 0.0;

    @autoreleasepool {
        NSArray *channels = [(__bridge NSDictionary *)delta objectForKey:@"IOReportChannels"];
        for (id item in channels) {
            CFDictionaryRef ch = (__bridge CFDictionaryRef)item;
            NSString *grp = (__bridge NSString *)IOReportChannelGetGroup(ch);
            NSString *name = (__bridge NSString *)IOReportChannelGetChannelName(ch);
            if (!grp || !name) continue;

            if ([grp isEqualToString:@"Energy Model"]) {
                int64_t raw = IOReportSimpleGetIntegerValue(ch, 0);
                if (raw <= 0) continue;
                NSString *unit = [(__bridge NSString *)IOReportChannelGetUnitLabel(ch) stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceAndNewlineCharacterSet]];
                double scale = 1e-3;
                if ([unit isEqualToString:@"nJ"]) scale = 1e-9;
                else if ([unit isEqualToString:@"uJ"]) scale = 1e-6;
                else if ([unit isEqualToString:@"mJ"]) scale = 1e-3;
                double watts = ((double)raw * scale) / dt_sec;

                if ([name isEqualToString:@"GPU Energy"]) {
                    res.gpu_watts += watts;
                } else if ([name isEqualToString:@"CPU Energy"]) {
                    res.cpu_watts += watts;
                } else if ([name hasPrefix:@"ANE"]) {
                    res.ane_watts += watts;
                } else if ([name hasPrefix:@"DRAM"]) {
                    res.dram_watts += watts;
                }
            } else if ([grp isEqualToString:@"CPU Stats"]) {
                if ([name isEqualToString:@"ECPU"] || [name isEqualToString:@"ECPU0"]) {
                    accumulate_cluster_states(ch, g_e_freqs_mhz, g_e_freq_count, &e_idle, &e_active, &e_wmhz);
                } else if ([name hasPrefix:@"PCPU"]) {
                    accumulate_cluster_states(ch, g_p_freqs_mhz, g_p_freq_count, &p_idle, &p_active, &p_wmhz);
                }
            } else if ([grp isEqualToString:@"GPU Stats"]) {
                if ([name isEqualToString:@"GPUPH"]) {
                    accumulate_cluster_states(ch, g_gpu_freqs_mhz, g_gpu_freq_count, &g_idle, &g_active, &g_wmhz);
                }
            }
        }
    }
    CFRelease(delta);

    if (e_active + e_idle > 0) {
        res.e_active_pct = (uint8_t)fmin(100.0, fmax(0.0, llround((double)e_active * 100.0 / (double)(e_active + e_idle))));
    }
    if (e_active > 0 && e_wmhz > 0.0) {
        res.e_mhz = (uint32_t)llround(e_wmhz / (double)e_active);
    }
    if (p_active + p_idle > 0) {
        res.p_active_pct = (uint8_t)fmin(100.0, fmax(0.0, llround((double)p_active * 100.0 / (double)(p_active + p_idle))));
    }
    if (p_active > 0 && p_wmhz > 0.0) {
        res.p_mhz = (uint32_t)llround(p_wmhz / (double)p_active);
    }
    if (g_active + g_idle > 0) {
        res.gpu_active_pct = (uint8_t)fmin(100.0, fmax(0.0, llround((double)g_active * 100.0 / (double)(g_active + g_idle))));
    }
    if (g_active > 0 && g_wmhz > 0.0) {
        res.gpu_mhz = (uint32_t)llround(g_wmhz / (double)g_active);
    }

    res.valid = 1;
    g_last_metrics = res;
    *out = res;
    pthread_mutex_unlock(&g_ior_mutex);
    return 1;
}

int sp_read_thermal_pressure_level(void) {
    @autoreleasepool {
        return (int)[[NSProcessInfo processInfo] thermalState];
    }
}

int sp_read_wifi_native(WifiNativeC *out) {
    if (!out) return 0;
    memset(out, 0, sizeof(*out));
    @autoreleasepool {
        CWInterface *iface = [[CWWiFiClient sharedWiFiClient] interface];
        if (!iface || !iface.powerOn || !iface.wlanChannel || iface.rssiValue == 0) {
            return 0;
        }
        CWChannel *ch = iface.wlanChannel;
        const char *band = "";
        switch (ch.channelBand) {
            case kCWChannelBand2GHz: band = "2GHz"; break;
            case kCWChannelBand5GHz: band = "5GHz"; break;
            case kCWChannelBand6GHz: band = "6GHz"; break;
            default: band = ""; break;
        }
        const char *width = "";
        switch (ch.channelWidth) {
            case kCWChannelWidth20MHz: width = "20MHz"; break;
            case kCWChannelWidth40MHz: width = "40MHz"; break;
            case kCWChannelWidth80MHz: width = "80MHz"; break;
            case kCWChannelWidth160MHz: width = "160MHz"; break;
            default: width = ""; break;
        }
        if (band[0] != '\0' && width[0] != '\0') {
            snprintf(out->channel, sizeof(out->channel), "%ld (%s, %s)", (long)ch.channelNumber, band, width);
        } else {
            snprintf(out->channel, sizeof(out->channel), "%ld", (long)ch.channelNumber);
        }

        const char *phy = "Wi-Fi";
        switch (iface.activePHYMode) {
            case kCWPHYMode11ax: phy = "802.11ax"; break;
            case kCWPHYMode11ac: phy = "802.11ac"; break;
            case kCWPHYMode11n: phy = "802.11n"; break;
            case kCWPHYMode11g: phy = "802.11g"; break;
            case kCWPHYMode11a: phy = "802.11a"; break;
            default: phy = "Wi-Fi"; break;
        }
        snprintf(out->phy_mode, sizeof(out->phy_mode), "%s", phy);

        const char *sec = "PROTECTED";
        switch (iface.security) {
            case kCWSecurityNone: sec = "OPEN"; break;
            case kCWSecurityWPA2Personal: sec = "WPA2 PERSONAL"; break;
            case kCWSecurityWPA3Personal: sec = "WPA3 PERSONAL"; break;
            case kCWSecurityWPA3Transition:
            case kCWSecurityWPAPersonalMixed: sec = "WPA2/WPA3 PERSONAL"; break;
            case kCWSecurityWPA2Enterprise:
            case kCWSecurityWPA3Enterprise: sec = "WPA ENTERPRISE"; break;
            default: sec = "PROTECTED"; break;
        }
        snprintf(out->security, sizeof(out->security), "%s", sec);

        out->tx_rate_mbps = (uint32_t)llround(iface.transmitRate);
        out->rssi_dbm = (int32_t)iface.rssiValue;
        out->noise_dbm = (int32_t)iface.noiseMeasurement;
        out->snr_db = out->rssi_dbm - out->noise_dbm;
        out->connected = 1;
        return 1;
    }
}

typedef struct {
    uint32_t id;
    char name[96];
    uint8_t is_input;
    uint8_t is_output;
    uint8_t is_default_output;
    uint8_t is_default_input;
} AudioDeviceNativeC;

static AudioDeviceID default_audio_device(AudioObjectPropertySelector selector) {
    AudioObjectPropertyAddress addr = {
        selector,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain
    };
    AudioDeviceID dev = kAudioObjectUnknown;
    UInt32 sz = sizeof(dev);
    if (AudioObjectGetPropertyData(kAudioObjectSystemObject, &addr, 0, NULL, &sz, &dev) == noErr) {
        return dev;
    }
    return kAudioObjectUnknown;
}

static uint8_t audio_device_has_channels(AudioDeviceID dev, AudioObjectPropertyScope scope) {
    AudioObjectPropertyAddress addr = {
        kAudioDevicePropertyStreamConfiguration,
        scope,
        kAudioObjectPropertyElementMain
    };
    UInt32 sz = 0;
    if (AudioObjectGetPropertyDataSize(dev, &addr, 0, NULL, &sz) != noErr || sz < sizeof(AudioBufferList)) {
        return 0;
    }
    AudioBufferList *list = (AudioBufferList *)malloc(sz);
    if (!list) return 0;
    uint8_t has = 0;
    if (AudioObjectGetPropertyData(dev, &addr, 0, NULL, &sz, list) == noErr) {
        for (UInt32 i = 0; i < list->mNumberBuffers; i++) {
            if (list->mBuffers[i].mNumberChannels > 0) {
                has = 1;
                break;
            }
        }
    }
    free(list);
    return has;
}

int sp_list_audio_devices_native(AudioDeviceNativeC *out, int max_count) {
    if (!out || max_count <= 0) return 0;
    AudioObjectPropertyAddress addr = {
        kAudioHardwarePropertyDevices,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain
    };
    UInt32 sz = 0;
    if (AudioObjectGetPropertyDataSize(kAudioObjectSystemObject, &addr, 0, NULL, &sz) != noErr || sz == 0) {
        return 0;
    }
    int count = (int)(sz / sizeof(AudioDeviceID));
    if (count <= 0) return 0;
    AudioDeviceID *devs = (AudioDeviceID *)malloc(sz);
    if (!devs) return 0;
    if (AudioObjectGetPropertyData(kAudioObjectSystemObject, &addr, 0, NULL, &sz, devs) != noErr) {
        free(devs);
        return 0;
    }
    AudioDeviceID def_out = default_audio_device(kAudioHardwarePropertyDefaultOutputDevice);
    AudioDeviceID def_in = default_audio_device(kAudioHardwarePropertyDefaultInputDevice);

    int written = 0;
    for (int i = 0; i < count && written < max_count; i++) {
        AudioDeviceID dev = devs[i];
        uint8_t is_out = audio_device_has_channels(dev, kAudioDevicePropertyScopeOutput);
        uint8_t is_in = audio_device_has_channels(dev, kAudioDevicePropertyScopeInput);
        if (!is_out && !is_in) continue;

        AudioObjectPropertyAddress name_addr = {
            kAudioObjectPropertyName,
            kAudioObjectPropertyScopeGlobal,
            kAudioObjectPropertyElementMain
        };
        CFStringRef cf_name = NULL;
        UInt32 nsz = sizeof(cf_name);
        if (AudioObjectGetPropertyData(dev, &name_addr, 0, NULL, &nsz, &cf_name) != noErr || !cf_name) {
            continue;
        }
        AudioDeviceNativeC item = {0};
        item.id = (uint32_t)dev;
        item.is_output = is_out;
        item.is_input = is_in;
        item.is_default_output = (is_out && dev == def_out) ? 1 : 0;
        item.is_default_input = (is_in && dev == def_in) ? 1 : 0;
        if (!CFStringGetCString(cf_name, item.name, sizeof(item.name), kCFStringEncodingUTF8)) {
            snprintf(item.name, sizeof(item.name), "Device %u", (unsigned int)dev);
        }
        CFRelease(cf_name);
        out[written++] = item;
    }
    free(devs);
    return written;
}

int sp_set_default_audio_device_native(uint32_t device_id, int is_input) {
    AudioDeviceID dev = (AudioDeviceID)device_id;
    AudioObjectPropertyAddress addr = {
        is_input ? kAudioHardwarePropertyDefaultInputDevice : kAudioHardwarePropertyDefaultOutputDevice,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain
    };
    if (AudioObjectSetPropertyData(kAudioObjectSystemObject, &addr, 0, NULL, sizeof(dev), &dev) != noErr) {
        return 0;
    }
    if (!is_input) {
        AudioObjectPropertyAddress sys_addr = {
            kAudioHardwarePropertyDefaultSystemOutputDevice,
            kAudioObjectPropertyScopeGlobal,
            kAudioObjectPropertyElementMain
        };
        AudioObjectSetPropertyData(kAudioObjectSystemObject, &sys_addr, 0, NULL, sizeof(dev), &dev);
    }
    return 1;
}

int sp_get_input_mute_native(uint8_t *muted) {
    if (!muted) return -1;
    AudioDeviceID in_dev = default_audio_device(kAudioHardwarePropertyDefaultInputDevice);
    if (in_dev == kAudioObjectUnknown) return -1;
    AudioObjectPropertyAddress m_addr = {
        kAudioDevicePropertyMute,
        kAudioDevicePropertyScopeInput,
        kAudioObjectPropertyElementMain
    };
    if (!AudioObjectHasProperty(in_dev, &m_addr)) return -1;
    UInt32 mute_val = 0;
    UInt32 msz = sizeof(mute_val);
    if (AudioObjectGetPropertyData(in_dev, &m_addr, 0, NULL, &msz, &mute_val) != noErr) {
        return -1;
    }
    *muted = mute_val ? 1 : 0;
    return 1;
}

int sp_set_input_mute_native(int muted) {
    AudioDeviceID in_dev = default_audio_device(kAudioHardwarePropertyDefaultInputDevice);
    if (in_dev == kAudioObjectUnknown) return 0;
    AudioObjectPropertyAddress m_addr = {
        kAudioDevicePropertyMute,
        kAudioDevicePropertyScopeInput,
        kAudioObjectPropertyElementMain
    };
    if (!AudioObjectHasProperty(in_dev, &m_addr)) return 0;
    Boolean settable = false;
    if (AudioObjectIsPropertySettable(in_dev, &m_addr, &settable) != noErr || !settable) {
        return 0;
    }
    UInt32 m = muted ? 1 : 0;
    return AudioObjectSetPropertyData(in_dev, &m_addr, 0, NULL, sizeof(m), &m) == noErr ? 1 : 0;
}

int sp_get_audio_volume_native(uint8_t *out_vol, uint8_t *in_vol, uint8_t *muted) {
    if (!out_vol || !in_vol || !muted) return 0;
    AudioDeviceID out_dev = default_audio_device(kAudioHardwarePropertyDefaultOutputDevice);
    if (out_dev == kAudioObjectUnknown) return 0;

    Float32 ovol = 0.0f;
    UInt32 osz = sizeof(ovol);
    AudioObjectPropertyAddress ov_addr = {
        kAudioHardwareServiceDeviceProperty_VirtualMainVolume,
        kAudioDevicePropertyScopeOutput,
        kAudioObjectPropertyElementMain
    };
    if (AudioObjectGetPropertyData(out_dev, &ov_addr, 0, NULL, &osz, &ovol) != noErr) {
        return 0;
    }
    long o_pct = llround((double)ovol * 100.0);
    *out_vol = (uint8_t)(o_pct < 0 ? 0 : (o_pct > 100 ? 100 : o_pct));

    UInt32 mute_val = 0;
    UInt32 msz = sizeof(mute_val);
    AudioObjectPropertyAddress m_addr = {
        kAudioDevicePropertyMute,
        kAudioDevicePropertyScopeOutput,
        kAudioObjectPropertyElementMain
    };
    if (AudioObjectGetPropertyData(out_dev, &m_addr, 0, NULL, &msz, &mute_val) == noErr) {
        *muted = mute_val ? 1 : 0;
    } else {
        *muted = 0;
    }

    *in_vol = 50;
    AudioDeviceID in_dev = default_audio_device(kAudioHardwarePropertyDefaultInputDevice);
    if (in_dev != kAudioObjectUnknown) {
        Float32 ivol = 0.5f;
        UInt32 isz = sizeof(ivol);
        AudioObjectPropertyAddress iv_addr = {
            kAudioHardwareServiceDeviceProperty_VirtualMainVolume,
            kAudioDevicePropertyScopeInput,
            kAudioObjectPropertyElementMain
        };
        if (AudioObjectGetPropertyData(in_dev, &iv_addr, 0, NULL, &isz, &ivol) == noErr) {
            long i_pct = llround((double)ivol * 100.0);
            *in_vol = (uint8_t)(i_pct < 0 ? 0 : (i_pct > 100 ? 100 : i_pct));
        }
    }
    return 1;
}

int sp_set_audio_volume_native(int out_vol, int in_vol, int muted) {
    if (out_vol >= 0 || muted >= 0) {
        AudioDeviceID out_dev = default_audio_device(kAudioHardwarePropertyDefaultOutputDevice);
        if (out_dev == kAudioObjectUnknown) return 0;

        if (out_vol >= 0) {
            int clamped = out_vol > 100 ? 100 : out_vol;
            Float32 v = (Float32)clamped / 100.0f;
            AudioObjectPropertyAddress ov_addr = {
                kAudioHardwareServiceDeviceProperty_VirtualMainVolume,
                kAudioDevicePropertyScopeOutput,
                kAudioObjectPropertyElementMain
            };
            if (AudioObjectSetPropertyData(out_dev, &ov_addr, 0, NULL, sizeof(v), &v) != noErr) {
                return 0;
            }
            if (clamped > 0 && muted < 0) {
                UInt32 m = 0;
                AudioObjectPropertyAddress m_addr = {
                    kAudioDevicePropertyMute,
                    kAudioDevicePropertyScopeOutput,
                    kAudioObjectPropertyElementMain
                };
                AudioObjectSetPropertyData(out_dev, &m_addr, 0, NULL, sizeof(m), &m);
            }
        }
        if (muted >= 0) {
            UInt32 m = muted ? 1 : 0;
            AudioObjectPropertyAddress m_addr = {
                kAudioDevicePropertyMute,
                kAudioDevicePropertyScopeOutput,
                kAudioObjectPropertyElementMain
            };
            if (AudioObjectSetPropertyData(out_dev, &m_addr, 0, NULL, sizeof(m), &m) != noErr) {
                return 0;
            }
        }
    }

    if (in_vol >= 0) {
        AudioDeviceID in_dev = default_audio_device(kAudioHardwarePropertyDefaultInputDevice);
        if (in_dev != kAudioObjectUnknown) {
            int clamped = in_vol > 100 ? 100 : in_vol;
            Float32 v = (Float32)clamped / 100.0f;
            AudioObjectPropertyAddress iv_addr = {
                kAudioHardwareServiceDeviceProperty_VirtualMainVolume,
                kAudioDevicePropertyScopeInput,
                kAudioObjectPropertyElementMain
            };
            AudioObjectSetPropertyData(in_dev, &iv_addr, 0, NULL, sizeof(v), &v);
        }
    }
    return 1;
}

typedef void (*GlobalHotkeyCallback)(void);
static GlobalHotkeyCallback g_hotkey_cb = NULL;
static GlobalHotkeyCallback g_mic_hotkey_cb = NULL;
static EventHotKeyRef g_hotkey_ref = NULL;
static EventHotKeyRef g_mic_hotkey_ref = NULL;
static EventHandlerRef g_handler_ref = NULL;

static OSStatus sp_hotkey_handler(EventHandlerCallRef next, EventRef event, void *user_data) {
    (void)next;
    (void)user_data;
    if (GetEventClass(event) == kEventClassKeyboard && GetEventKind(event) == kEventHotKeyPressed) {
        EventHotKeyID hk_id = {0, 0};
        if (GetEventParameter(event, kEventParamDirectObject, typeEventHotKeyID, NULL, sizeof(hk_id), NULL, &hk_id) == noErr) {
            if (hk_id.id == 2) {
                if (g_mic_hotkey_cb) g_mic_hotkey_cb();
                return noErr;
            }
        }
        if (g_hotkey_cb) {
            g_hotkey_cb();
        }
    }
    return noErr;
}

static void ensure_hotkey_handler_installed(void) {
    if (!g_handler_ref) {
        EventTypeSpec spec = { kEventClassKeyboard, kEventHotKeyPressed };
        InstallApplicationEventHandler(&sp_hotkey_handler, 1, &spec, NULL, &g_handler_ref);
    }
}

int sp_register_global_hotkey(int preset_id, GlobalHotkeyCallback callback) {
    g_hotkey_cb = callback;
    void (^work)(void) = ^{
        if (g_hotkey_ref) {
            UnregisterEventHotKey(g_hotkey_ref);
            g_hotkey_ref = NULL;
        }
        if (preset_id <= 0 || !callback) {
            return;
        }
        ensure_hotkey_handler_installed();
        EventHotKeyID hk_id = { 'STPL', 1 };
        UInt32 key_code = 0;
        UInt32 mods = 0;
        if (preset_id == 1) {
            key_code = 0x31; // Space
            mods = optionKey;
        } else if (preset_id == 2) {
            key_code = 0x01; // S
            mods = cmdKey | shiftKey;
        } else {
            return;
        }
        RegisterEventHotKey(key_code, mods, hk_id, GetApplicationEventTarget(), 0, &g_hotkey_ref);
    };
    if ([NSThread isMainThread]) {
        work();
    } else {
        dispatch_async(dispatch_get_main_queue(), work);
    }
    return 1;
}

int sp_register_mic_mute_hotkey(int enabled, GlobalHotkeyCallback callback) {
    g_mic_hotkey_cb = callback;
    void (^work)(void) = ^{
        if (g_mic_hotkey_ref) {
            UnregisterEventHotKey(g_mic_hotkey_ref);
            g_mic_hotkey_ref = NULL;
        }
        if (enabled <= 0 || !callback) {
            return;
        }
        ensure_hotkey_handler_installed();
        EventHotKeyID hk_id = { 'STPL', 2 };
        RegisterEventHotKey(0x2E, cmdKey | shiftKey, hk_id, GetApplicationEventTarget(), 0, &g_mic_hotkey_ref);
    };
    if ([NSThread isMainThread]) {
        work();
    } else {
        dispatch_async(dispatch_get_main_queue(), work);
    }
    return 1;
}

int sp_post_user_notification(const char *title, const char *subtitle) {
    if (!title) return 0;
    @autoreleasepool {
        NSString *bundleId = [[NSBundle mainBundle] bundleIdentifier];
        if (!bundleId || bundleId.length == 0) {
            return 0;
        }
        NSString *nsTitle = [NSString stringWithUTF8String:title] ?: @"StatPulse";
        NSString *nsSub = (subtitle && subtitle[0] != '\0') ? [NSString stringWithUTF8String:subtitle] : nil;

        UNUserNotificationCenter *center = [UNUserNotificationCenter currentNotificationCenter];
        [center requestAuthorizationWithOptions:(UNAuthorizationOptionAlert | UNAuthorizationOptionSound)
                              completionHandler:^(BOOL granted, NSError * _Nullable error) {
            (void)error;
            if (!granted) return;
            UNMutableNotificationContent *content = [[UNMutableNotificationContent alloc] init];
            content.title = @"StatPulse";
            if (nsSub) {
                content.subtitle = nsTitle;
                content.body = nsSub;
            } else {
                content.body = nsTitle;
            }
            NSString *ident = [[NSUUID UUID] UUIDString];
            UNNotificationRequest *req = [UNNotificationRequest requestWithIdentifier:ident
                                                                              content:content
                                                                              trigger:nil];
            [center addNotificationRequest:req withCompletionHandler:nil];
        }];
        return 1;
    }
}
