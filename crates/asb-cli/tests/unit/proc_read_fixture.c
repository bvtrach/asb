/* Test-only, child-scoped read mapping. No production binary loads this shim. */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static const char *fixture_path(const char *path, int flags, char buffer[PATH_MAX]) {
    const char *name = NULL;
    if (!strcmp(path, "/proc/sys/kernel/perf_event_paranoid")) name = "paranoid";
    else if (!strcmp(path, "/proc/sys/kernel/sched_schedstats")) name = "schedstats";
    else if (!strcmp(path, "/proc/self/cgroup")) name = "cgroup";
    if (!name) return path;
    /* Fail closed rather than permitting a new direct host write in a test. */
    if ((flags & O_ACCMODE) != O_RDONLY) { errno = EACCES; return NULL; }
    const char *root = getenv("ASB_PROC_FIXTURE_ROOT");
    if (!root) { errno = EINVAL; return NULL; }
    if (snprintf(buffer, PATH_MAX, "%s/%s", root, name) >= PATH_MAX) {
        errno = ENAMETOOLONG; return NULL;
    }
    char marker[PATH_MAX];
    if (snprintf(marker, PATH_MAX, "%s/shim-active", root) >= PATH_MAX) {
        errno = ENAMETOOLONG; return NULL;
    }
    FILE *receipt = fopen(marker, "ab");
    if (!receipt) { errno = EIO; return NULL; }
    fputs("mapped-read\n", receipt);
    fclose(receipt);
    return buffer;
}

#define OPEN_WRAPPER(name) \
int name(const char *path, int flags, ...) { \
    mode_t mode = 0; \
    if (flags & O_CREAT) { va_list args; va_start(args, flags); mode = va_arg(args, mode_t); va_end(args); } \
    char buffer[PATH_MAX]; \
    const char *selected = fixture_path(path, flags, buffer); \
    if (!selected) return -1; \
    int (*original)(const char *, int, ...) = dlsym(RTLD_NEXT, #name); \
    if (!original) { errno = ENOSYS; return -1; } \
    return original(selected, flags, mode); \
}

#define OPENAT_WRAPPER(name) \
int name(int directory, const char *path, int flags, ...) { \
    mode_t mode = 0; \
    if (flags & O_CREAT) { va_list args; va_start(args, flags); mode = va_arg(args, mode_t); va_end(args); } \
    char buffer[PATH_MAX]; \
    const char *selected = fixture_path(path, flags, buffer); \
    if (!selected) return -1; \
    int (*original)(int, const char *, int, ...) = dlsym(RTLD_NEXT, #name); \
    if (!original) { errno = ENOSYS; return -1; } \
    return original(directory, selected, flags, mode); \
}

OPEN_WRAPPER(open)
OPEN_WRAPPER(open64)
OPENAT_WRAPPER(openat)
OPENAT_WRAPPER(openat64)
