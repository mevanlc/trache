#import <Foundation/Foundation.h>
#include <dlfcn.h>
#include <string.h>
#include <unistd.h>

static void emit(NSDictionary *record) {
    NSData *json = [NSJSONSerialization dataWithJSONObject:record options:0 error:NULL];
    fwrite(json.bytes, 1, json.length, stdout);
    putchar('\n');
    fflush(stdout);
}

// A debugger can stop here after all moves, before normal process exit.
__attribute__((noinline)) void flush_point(void) { __asm__ volatile(""); }

int main(int argc, char **argv) {
    @autoreleasepool {
        if (argc != 5) return 2;
        const char *mode = argv[1];
        if (strcmp(mode, "baseline") && strcmp(mode, "linger") &&
            strcmp(mode, "FlushCatalogs") && strcmp(mode, "NodeContextFlush")) return 2;
        void *framework = dlopen("/System/Library/PrivateFrameworks/DesktopServicesPriv.framework/DesktopServicesPriv", RTLD_NOW | RTLD_LOCAL);
        if (!framework) return 3;
        emit(@{@"event": @"start", @"pid": @(getpid()), @"mode": @(mode)});
        for (int i = 2; i < argc; ++i) {
            NSURL *source = [NSURL fileURLWithPath:@(argv[i])];
            NSURL *destination = nil;
            NSError *error = nil;
            if (![[NSFileManager defaultManager] trashItemAtURL:source resultingItemURL:&destination error:&error]) {
                emit(@{@"event": @"error", @"description": error.description ?: @"unknown"});
                return 4;
            }
            emit(@{@"event": @"move", @"source": source.path, @"destination": destination.path});
        }
        flush_point();
        if (!strcmp(mode, "FlushCatalogs") || !strcmp(mode, "NodeContextFlush")) {
            // Signatures inferred from this build's disassembly. Private API experiment only.
            void (*flush)(void) = dlsym(framework, mode);
            if (!flush) return 5;
            CFAbsoluteTime before = CFAbsoluteTimeGetCurrent();
            flush();
            emit(@{@"event": @"flush_returned", @"symbol": @(mode),
                   @"seconds": @(CFAbsoluteTimeGetCurrent() - before)});
        }
        if (!strcmp(mode, "linger")) sleep(3);
        emit(@{@"event": @"done"});
    }
    return 0;
}
