// pomme: extra declarations for building swift-corelibs-foundation's
// CoreFoundation for arm64 iOS (it normally targets Linux/Windows/WASI).
#include <objc/objc-api.h>

// With DEPLOYMENT_RUNTIME_SWIFT=0 (Apple's object layout, which the app's
// compiled-in constant CFStrings use), CF skips ForSwiftFoundationOnly.h but
// still uses a few of its typedefs.
#include <pthread.h>
typedef pthread_t _CFThreadRef;
typedef pthread_key_t _CFThreadSpecificKey;
typedef pthread_attr_t _CFThreadAttributes;
#define OS_LOG_SUBSYSTEM_RUNTIME_ISSUES "com.apple.runtime-issues"
