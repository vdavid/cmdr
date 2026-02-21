#include <sys/attr.h>
#include <sys/vnode.h>
#include <sys/param.h>
#include <sys/mount.h>
#include <sys/fsgetpath.h>
#include <unistd.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <errno.h>

#define MAX_MATCHES 64

struct packed_name_attr {
    u_int32_t           size;
    struct attrreference ref;
    char                name[PATH_MAX];
};
struct packed_attr_ref {
    u_int32_t           size;
    struct attrreference ref;
};
struct packed_result {
    u_int32_t       size;
    struct fsid     fs_id;
    struct fsobj_id obj_id;
};

static void do_search(const char *vol, const char *match, int exact) {
    struct fssearchblock    sb;
    struct attrlist         rl;
    struct searchstate      state;
    struct packed_name_attr info1;
    struct packed_attr_ref  info2;
    struct packed_result    rbuf[MAX_MATCHES];
    unsigned long           matches;
    unsigned int            opts;
    int                     err, ebusy = 0;

restart:
    memset(&sb, 0, sizeof(sb));
    sb.searchattrs.bitmapcount = ATTR_BIT_MAP_COUNT;
    sb.searchattrs.commonattr  = ATTR_CMN_NAME;

    memset(&rl, 0, sizeof(rl));
    rl.bitmapcount = ATTR_BIT_MAP_COUNT;
    rl.commonattr  = ATTR_CMN_FSID | ATTR_CMN_OBJID;
    sb.returnattrs      = &rl;
    sb.returnbuffer     = rbuf;
    sb.returnbuffersize = sizeof(rbuf);

    memset(&info1, 0, sizeof(info1));
    strcpy(info1.name, match);
    info1.ref.attr_dataoffset = sizeof(struct attrreference);
    info1.ref.attr_length     = (u_int32_t)strlen(info1.name) + 1;
    info1.size = sizeof(struct attrreference) + info1.ref.attr_length;
    sb.searchparams1       = &info1;
    sb.sizeofsearchparams1 = info1.size + sizeof(u_int32_t);

    memset(&info2, 0, sizeof(info2));
    info2.size = sizeof(struct attrreference);
    info2.ref.attr_dataoffset = sizeof(struct attrreference);
    info2.ref.attr_length = 0;
    sb.searchparams2       = &info2;
    sb.sizeofsearchparams2 = sizeof(info2);

    sb.maxmatches       = MAX_MATCHES;
    sb.timelimit.tv_sec = 1;

    memset(&state, 0, sizeof(state));
    opts = SRCHFS_START | SRCHFS_MATCHFILES | SRCHFS_MATCHDIRS;
    if (!exact) opts |= SRCHFS_MATCHPARTIALNAMES;

    do {
        matches = 0;
        err = searchfs(vol, &sb, &matches, 0, opts, &state);
        if (err == -1) err = errno; else err = 0;

        /* resolve paths for matches */
        char *ptr = (char *)rbuf;
        for (unsigned long i = 0; i < matches; i++) {
            struct packed_result *r = (struct packed_result *)ptr;
            char path[PATH_MAX];
            ssize_t sz = fsgetpath(path, sizeof(path), &r->fs_id,
                        (uint64_t)r->obj_id.fid_objno |
                        ((uint64_t)r->obj_id.fid_generation << 32));
            if (sz > -1) printf("%s\n", path);
            ptr += r->size;
        }

        opts &= ~SRCHFS_START;
        if (err == EBUSY) {
            if (++ebusy > 5) break;
            opts |= SRCHFS_START;
            goto restart;
        }
    } while (err == EAGAIN);
}

int main(int argc, char **argv) {
    const char *match = argc > 1 ? argv[1] : "test";
    int exact = argc > 2 && strcmp(argv[2], "--exact") == 0;
    do_search("/", match, exact);
    do_search("/System/Volumes/Data", match, exact);
    return 0;
}
