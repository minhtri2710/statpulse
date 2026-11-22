/*
 * Native regression fixture for src-tauri/src/smc/smc.c fan restore logic.
 *
 * Compiled with -DIOConnectCallStructMethod=statpulse_fixture_smc so every SMC
 * transport call is answered by the synthetic firmware model below. No IOKit
 * user client is ever opened: the fixture implements the exact SDK signature
 * and fills in the SMCKeyData output struct the way AppleSMC would.
 */

#include <stddef.h>
#include <stdio.h>
#include <string.h>
#include <IOKit/IOKitLib.h>
#include "../src/smc/smc.h"

/* Firmware model knobs */
static int mode_value = 1;      /* current F0Md value: 0=AUTO 1=MANUAL 3=SYSTEM */
static int f1_mode_value = 1;   /* independent F1Md value */
static int mode_present = 1;
static int f1_mode_present = 1;
static int f1_mode_apply_write = 1;
static int f1_mode_reject_write = 0;
static int mode_reject_write = 0;   /* firmware returns 0x82 on mode write */
static int mode_apply_write = 1;    /* whether accepted writes actually stick */
static int mode_probe_transport_fail = 0; /* transport error on F0Md keyinfo */
static int mode_read_fail = 0;      /* transport error on F0Md READ_BYTES */
static int ftst_present = 0;
static int ftst_reject_write = 0;
static int fnum_value = 1;
static int fnum_size = 1;
static int fnum_count = -1; /* explicit FNum byte value, -1 = use fnum_value */
static int truncate_read_reply = 0;
static int ftst_value = 1;

static int mode_writes = 0;
static int ftst_writes = 0;

static UInt32 fourcc(const char *s)
{
    return ((UInt32)s[0] << 24) | ((UInt32)s[1] << 16) | ((UInt32)s[2] << 8) | (UInt32)s[3];
}

static void reset_model(void)
{
    mode_value = 1;
    f1_mode_value = 1;
    mode_present = 1;
    f1_mode_present = 1;
    f1_mode_apply_write = 1;
    f1_mode_reject_write = 0;
    mode_reject_write = 0;
    mode_apply_write = 1;
    mode_probe_transport_fail = 0;
    mode_read_fail = 0;
    ftst_present = 0;
    ftst_reject_write = 0;
    fnum_value = 1;
    fnum_size = 1;
    fnum_count = -1;
    truncate_read_reply = 0;
    ftst_value = 1;
    mode_writes = 0;
    ftst_writes = 0;
}

kern_return_t statpulse_fixture_smc(io_connect_t connection,
                                    uint32_t selector,
                                    const void *inputStruct,
                                    size_t inputStructCnt,
                                    void *outputStruct,
                                    size_t *outputStructCnt)
{
    const SMCKeyData_t *in = (const SMCKeyData_t *)inputStruct;
    SMCKeyData_t *out = (SMCKeyData_t *)outputStruct;
    (void)connection;
    (void)inputStructCnt;

    if (selector != KERNEL_INDEX_SMC)
        return kIOReturnError;

    memset(out, 0, sizeof(*out));
    *outputStructCnt = sizeof(*out);

    if (in->data8 == SMC_CMD_READ_KEYINFO)
    {
        if (in->key == fourcc("F0Md") && mode_probe_transport_fail)
            return kIOReturnError;
        if (in->key == fourcc("F0Md") || in->key == fourcc("F0md"))
        {
            if (!mode_present)
            {
                out->result = (char)0x84;
                return kIOReturnSuccess;
            }
            out->keyInfo.dataSize = 1;
            out->keyInfo.dataType = fourcc("ui8 ");
            return kIOReturnSuccess;
        }
        if (in->key == fourcc("F1Md") || in->key == fourcc("F1md"))
        {
            if (!f1_mode_present)
            {
                out->result = (char)0x84;
                return kIOReturnSuccess;
            }
            out->keyInfo.dataSize = 1;
            out->keyInfo.dataType = fourcc("ui8 ");
            return kIOReturnSuccess;
        }
        if (in->key == fourcc("Ftst"))
        {
            if (!ftst_present)
            {
                out->result = (char)0x84;
                return kIOReturnSuccess;
            }
            out->keyInfo.dataSize = 1;
            out->keyInfo.dataType = fourcc("ui8 ");
            return kIOReturnSuccess;
        }
        if (in->key == fourcc("FNum"))
        {
            out->keyInfo.dataSize = fnum_size;
            out->keyInfo.dataType = fourcc("ui8 ");
            return kIOReturnSuccess;
        }
        out->result = (char)0x84;
        return kIOReturnSuccess;
    }

    if (in->data8 == SMC_CMD_READ_BYTES)
    {
        if (in->key == fourcc("F0Md") || in->key == fourcc("F0md"))
        {
            if (mode_read_fail)
                return kIOReturnError;
            if (truncate_read_reply)
            {
                *outputStructCnt = offsetof(SMCKeyData_t, bytes);
                out->result = 0;
                return kIOReturnSuccess;
            }
            out->bytes[0] = (UInt8)mode_value;
            return kIOReturnSuccess;
        }
        if (in->key == fourcc("F1Md") || in->key == fourcc("F1md"))
        {
            out->bytes[0] = (UInt8)f1_mode_value;
            return kIOReturnSuccess;
        }
        if (in->key == fourcc("FNum"))
        {
            out->bytes[0] = (UInt8)(fnum_count >= 0 ? fnum_count : fnum_value);
            return kIOReturnSuccess;
        }
        if (in->key == fourcc("Ftst"))
        {
            out->bytes[0] = (UInt8)ftst_value;
            return kIOReturnSuccess;
        }
        out->result = (char)0x84;
        return kIOReturnSuccess;
    }

    if (in->data8 == SMC_CMD_WRITE_BYTES)
    {
        if (in->key == fourcc("F0Md") || in->key == fourcc("F0md"))
        {
            mode_writes++;
            if (mode_reject_write)
            {
                out->result = (char)0x82;
                return kIOReturnSuccess;
            }
            if (mode_apply_write)
                mode_value = in->bytes[0];
            return kIOReturnSuccess;
        }
        if (in->key == fourcc("F1Md") || in->key == fourcc("F1md"))
        {
            mode_writes++;
            if (f1_mode_reject_write)
            {
                out->result = (char)0x82;
                return kIOReturnSuccess;
            }
            if (f1_mode_apply_write)
                f1_mode_value = in->bytes[0];
            return kIOReturnSuccess;
        }
        if (in->key == fourcc("Ftst"))
        {
            ftst_writes++;
            if (ftst_reject_write)
            {
                out->result = (char)0x82;
                return kIOReturnSuccess;
            }
            ftst_value = in->bytes[0];
            return kIOReturnSuccess;
        }
        out->result = (char)0x84;
        return kIOReturnSuccess;
    }

    return kIOReturnError;
}

static int failures = 0;

static void check(const char *name, int condition)
{
    if (condition)
    {
        printf("ok: %s\n", name);
    }
    else
    {
        printf("FAIL: %s\n", name);
        failures++;
    }
}

int main(void)
{
    /* Mode write accepted, applied, readback system-owned: success. */
    reset_model();
    check("confirmed write restores auto",
          setFanAuto(0, 0) == kIOReturnSuccess && mode_value == 0);

    /* Firmware rejects the write at the result byte despite OK transport. */
    reset_model();
    mode_reject_write = 1;
    check("firmware-rejected write fails", setFanAuto(0, 0) != kIOReturnSuccess);

    /* Transport ok, write accepted, but readback still manual. */
    reset_model();
    mode_apply_write = 0;
    check("manual readback after write fails", setFanAuto(0, 0) != kIOReturnSuccess);

    /* Missing mode key: cannot confirm -> error. */
    reset_model();
    mode_present = 0;
    check("missing mode key fails", setFanAuto(0, 0) != kIOReturnSuccess);

    /* Transient key-read error fails this attempt but must not poison the
     * casing cache: a retry after recovery succeeds. */
    reset_model();
    mode_read_fail = 1;
    check("transient mode read error fails", setFanAuto(0, 0) != kIOReturnSuccess);
    mode_read_fail = 0;
    check("retry after transient error succeeds",
          setFanAuto(0, 0) == kIOReturnSuccess && mode_value == 0);

    /* Transient transport failure on the casing probe: fails, then retries. */
    reset_model();
    mode_probe_transport_fail = 1;
    check("transient probe error fails", setFanAuto(0, 0) != kIOReturnSuccess);
    mode_probe_transport_fail = 0;
    check("retry after probe error succeeds",
          setFanAuto(0, 0) == kIOReturnSuccess);

    /* Already system-owned: no write issued at all. */
    reset_model();
    mode_value = 0;
    check("auto mode needs no write",
          setFanAuto(0, 0) == kIOReturnSuccess && mode_writes == 0);
    mode_value = 3;
    check("system mode needs no write",
          setFanAuto(0, 0) == kIOReturnSuccess && mode_writes == 0);

    /* Ftst present: clear is attempted and honored. */
    reset_model();
    ftst_present = 1;
    check("present ftst is cleared",
          setFanAuto(0, 0) == kIOReturnSuccess && ftst_writes == 1 && ftst_value == 0);

    /* Ftst present but rejecting its clear: error propagates. */
    reset_model();
    ftst_present = 1;
    ftst_reject_write = 1;
    check("ftst clear failure propagates", setFanAuto(0, 0) != kIOReturnSuccess);

    /* Truncated READ_BYTES reply: transport ok, result 0, but no mode data
     * delivered. Cannot confirm auto -> setFanAuto fails. */
    reset_model();
    truncate_read_reply = 1;
    check("truncated read reply fails", setFanAuto(0, 0) != kIOReturnSuccess);

    /* getFanCount: 0 is a real answer, malformed read is -1. */
    reset_model();
    fnum_value = 0;
    check("zero fan count is a real value", getFanCount(0) == 0);
    fnum_size = 0;
    check("malformed fan count is -1", getFanCount(0) == -1);
    fnum_size = 8;
    check("oversized fan count is -1", getFanCount(0) == -1);
    fnum_size = 1;
    fnum_count = 42;
    check("unaddressable fan count is -1", getFanCount(0) == -1);
    fnum_count = 2;
    check("bounded fan count decodes", getFanCount(0) == 2);

    /* setAllFansAuto: both manual fans written once and read back. */
    reset_model();
    fnum_count = 2;
    check("all fans restored together",
          setAllFansAuto(0) == kIOReturnSuccess && mode_value == 0
              && f1_mode_value == 0 && mode_writes == 2);

    /* First fan rejects the write: second still restored, Ftst cleared once,
     * and the failure propagates. */
    reset_model();
    fnum_count = 2;
    mode_reject_write = 1;
    ftst_present = 1;
    check("rejected fan does not stop the rest",
          setAllFansAuto(0) != kIOReturnSuccess && f1_mode_value == 0
              && ftst_writes == 1 && ftst_value == 0);

    /* Second fan accepts the write but stays manual on readback: fails while
     * the first fan is still restored. */
    reset_model();
    fnum_count = 2;
    f1_mode_apply_write = 0;
    check("stuck second fan fails",
          setAllFansAuto(0) != kIOReturnSuccess && mode_value == 0
              && f1_mode_value == 1);

    /* Zero declared fans is a real answer: success with no mode or Ftst
     * probes at all. */
    reset_model();
    fnum_count = 0;
    check("zero fans succeeds without probing",
          setAllFansAuto(0) == kIOReturnSuccess && mode_writes == 0
              && ftst_writes == 0);

    /* Malformed fan count fails before any restore work. */
    reset_model();
    fnum_size = 8;
    check("malformed fan count fails restore",
          setAllFansAuto(0) != kIOReturnSuccess && mode_writes == 0);

    if (failures)
    {
        printf("%d fixture checks failed\n", failures);
        return 1;
    }
    printf("all fixture checks passed\n");
    return 0;
}
