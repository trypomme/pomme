// pomme: stands in for Dispatch.mm in Apple's old Dispatch overlay.
#include <stdlib.h>

void _swift_dispatch_source_create_abort(void) {
    abort();
}
