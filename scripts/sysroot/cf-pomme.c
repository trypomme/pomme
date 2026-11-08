// pomme: definitions CoreFoundation references but whose sources aren't part
// of swift-corelibs-foundation (Mach ports, thread naming). Under pomme there
// are no Mach ports and thread names are cosmetic.
#include "CFInternal.h"
#include "CFRuntime_Internal.h"
#include <mach/mach.h>

const CFRuntimeClass __CFMachPortClass = {
    .version = 0,
    .className = "CFMachPort",
};

CF_EXPORT void __CFMachMessageCheckForAndDestroyUnsentMessage(kern_return_t const kr, mach_msg_header_t *const msg) {
    (void)kr;
    (void)msg;
}

int _CFThreadSetName(_CFThreadRef thread, const char *name) {
    (void)thread;
    (void)name;
    return 0;
}
