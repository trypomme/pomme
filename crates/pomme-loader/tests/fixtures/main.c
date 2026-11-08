// Fixture: every kind of fixup the loader must handle.
#include <stdio.h>

int answer = 42;
int *answer_ptr = &answer;                 // rebase within the executable
int (*puts_ptr)(const char *) = puts;      // bind to a system library -> HLE stub
extern int lib_value;
int *lib_ptr = &lib_value;                 // bind to a bundled dylib's export
extern int lib_reexported;                 // defined in libinner, re-exported by libfix
int *reexported_ptr = &lib_reexported;
extern int maybe_missing __attribute__((weak_import));
int *weak_ptr = &maybe_missing;            // weak import that resolves to nothing
const char *greeting = "hello pomme";      // rebase to a C string in __TEXT

int main(void) { return *answer_ptr + *lib_ptr; }
