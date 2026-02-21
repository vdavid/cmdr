#include <sys/attr.h>
#include <sys/vnode.h>
#include <sys/param.h>
#include <sys/mount.h>
#include <unistd.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <errno.h>

#define MAX_MATCHES 4096

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

static unsigned long do_search(const char *vol, const char *match, int negate) {
    struct fssearchblock    sb;
    struct attrlist         rl;
    struct searchstate      state;
    struct packed_name_attr info1;
    struct packed_attr_ref  info2;
    struct packed_result    rbuf[MAX_MATCHES];
    unsigned long           matches, total = 0;
    unsigned int            opts;
    int                     err, ebusy = 0;

restart:
    total = 0;
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
    opts = SRCHFS_START | SRCHFS_MATCHFILES | SRCHFS_MATCHPARTIALNAMES;
    if (negate) opts |= SRCHFS_NEGATEPARAMS;

    do {
        matches = 0;
        err = searchfs(vol, &sb, &matches, 0, opts, &state);
        if (err == -1) err = errno; else err = 0;
        total += matches;
        opts &= ~SRCHFS_START;
        if (err == EBUSY) {
            if (++ebusy > 5) break;
            opts |= SRCHFS_START;
            goto restart;
        }
    } while (err == EAGAIN);

    return total;
}

int main(void) {
    unsigned long sys_with    = do_search("/", ".", 0);
    unsigned long sys_without = do_search("/", ".", 1);
    unsigned long dat_with    = do_search("/System/Volumes/Data", ".", 0);
    unsigned long dat_without = do_search("/System/Volumes/Data", ".", 1);

    unsigned long total = sys_with + sys_without + dat_with + dat_without;

    fprintf(stderr, "  /                    with dot: %10lu\n", sys_with);
    fprintf(stderr, "  /                 without dot: %10lu\n", sys_without);
    fprintf(stderr, "  /System/Volumes/Data with dot: %10lu\n", dat_with);
    fprintf(stderr, "  /System/Volumes/Data  no dot:  %10lu\n", dat_without);
    fprintf(stderr, "  ----------------------------------------\n");
    fprintf(stderr, "  TOTAL FILES:                   %10lu\n", total);

    printf("%lu\n", total);
    return 0;
}
