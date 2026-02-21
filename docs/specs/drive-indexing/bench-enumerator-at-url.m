#import <Foundation/Foundation.h>
#include <stdio.h>
#include <stdint.h>

static int should_skip(NSString *path) {
    return [path hasPrefix:@"/Volumes/naspi"] ||
           [path hasPrefix:@"/System/Volumes/Data"];
}

int main(void) { @autoreleasepool {
    NSFileManager *fm = [NSFileManager defaultManager];
    NSURL *root = [NSURL fileURLWithPath:@"/"];

    NSArray<NSURLResourceKey> *keys = @[
        NSURLIsDirectoryKey, NSURLIsSymbolicLinkKey,
        NSURLFileSizeKey, NSURLTotalFileAllocatedSizeKey
    ];

    NSDirectoryEnumerator<NSURL *> *en = [fm
        enumeratorAtURL:root
        includingPropertiesForKeys:keys
        options:0
        errorHandler:^BOOL(NSURL *u, NSError *e) { return YES; }];

    unsigned long files = 0, dirs = 0, symlinks = 0;
    uint64_t logical = 0, physical = 0;

    for (NSURL *url in en) {
        NSString *path = [url path];
        if (should_skip(path)) { [en skipDescendants]; continue; }

        NSNumber *isDir = nil, *isSym = nil;
        [url getResourceValue:&isDir forKey:NSURLIsDirectoryKey error:nil];
        [url getResourceValue:&isSym forKey:NSURLIsSymbolicLinkKey error:nil];

        if (isSym && [isSym boolValue]) { symlinks++; }
        else if (isDir && [isDir boolValue]) { dirs++; }
        else {
            files++;
            NSNumber *sz = nil, *asz = nil;
            if ([url getResourceValue:&sz forKey:NSURLFileSizeKey error:nil] && sz)
                logical += [sz unsignedLongLongValue];
            if ([url getResourceValue:&asz forKey:NSURLTotalFileAllocatedSizeKey error:nil] && asz)
                physical += [asz unsignedLongLongValue];
        }

        if ((files+dirs+symlinks)%100000==0)
            fprintf(stderr,"\r  %lu files, %.1f / %.1f GB...",
                    files, (double)logical/(1024.0*1024*1024), (double)physical/(1024.0*1024*1024));
    }

    fprintf(stderr,"\r                                                              \r");
    fprintf(stderr,"  Files:     %lu\n  Dirs:      %lu\n  Symlinks:  %lu\n", files, dirs, symlinks);
    fprintf(stderr,"  Logical:   %.2f GB\n  Physical:  %.2f GB\n",
            (double)logical/(1024.0*1024*1024), (double)physical/(1024.0*1024*1024));
    printf("%lu files, logical=%.2f GB, physical=%.2f GB\n", files,
           (double)logical/(1024.0*1024*1024), (double)physical/(1024.0*1024*1024));
    return 0;
}}
