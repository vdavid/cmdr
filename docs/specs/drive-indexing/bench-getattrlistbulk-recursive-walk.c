#include <sys/attr.h>
#include <sys/vnode.h>
#include <sys/param.h>
#include <fcntl.h>
#include <unistd.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <errno.h>
#include <stdint.h>

/*
 * getattrlistbulk with both logical and physical size.
 *
 * Packed order (file attrs by bit):
 *   ATTR_FILE_DATALENGTH    0x200 → off_t
 *   ATTR_FILE_DATAALLOCSIZE 0x400 → off_t
 */

typedef struct {
    u_int32_t       length;
    attribute_set_t returned;
    attrreference_t name;
    fsobj_type_t    objtype;
    /* For VREG: off_t datalength, off_t dataallocsize follow */
} EntryHeader;

typedef struct { char **items; size_t count, cap; } Stack;
static void spush(Stack *s, const char *p) {
    if (s->count >= s->cap) { s->cap = s->cap ? s->cap*2 : 8192; s->items = realloc(s->items, s->cap * sizeof(char*)); }
    s->items[s->count++] = strdup(p);
}
static char *spop(Stack *s) { return s->count ? s->items[--s->count] : NULL; }
static int skip(const char *p) {
    return strcmp(p, "/Volumes/naspi") == 0 || strncmp(p, "/Volumes/naspi/", 15) == 0 ||
           strcmp(p, "/System/Volumes/Data") == 0 || strncmp(p, "/System/Volumes/Data/", 21) == 0;
}

int main(void) {
    struct attrlist al;
    memset(&al, 0, sizeof(al));
    al.bitmapcount = ATTR_BIT_MAP_COUNT;
    al.commonattr  = ATTR_CMN_RETURNED_ATTRS | ATTR_CMN_NAME | ATTR_CMN_OBJTYPE;
    al.fileattr    = ATTR_FILE_DATALENGTH | ATTR_FILE_DATAALLOCSIZE;

    char buf[256*1024];
    Stack dirs = {0};
    spush(&dirs, "/");

    unsigned long files = 0, dircnt = 0, symlinks = 0, other = 0, errors = 0, walked = 0;
    uint64_t logical = 0, physical = 0;

    char *dp;
    while ((dp = spop(&dirs))) {
        if (skip(dp)) { free(dp); continue; }
        int fd = open(dp, O_RDONLY | O_DIRECTORY);
        if (fd < 0) { errors++; free(dp); continue; }
        walked++;
        int n;
        while ((n = getattrlistbulk(fd, &al, buf, sizeof(buf), 0)) > 0) {
            char *p = buf;
            for (int i = 0; i < n; i++) {
                EntryHeader *e = (EntryHeader *)p;
                char *name = ((char *)&e->name) + e->name.attr_dataoffset;
                switch (e->objtype) {
                case VREG: {
                    files++;
                    char *after = p + sizeof(EntryHeader);
                    if (e->returned.fileattr & ATTR_FILE_DATALENGTH) {
                        logical += (uint64_t)*(off_t *)after;
                        after += sizeof(off_t);
                    }
                    if (e->returned.fileattr & ATTR_FILE_DATAALLOCSIZE) {
                        physical += (uint64_t)*(off_t *)after;
                    }
                    break;
                }
                case VDIR:
                    dircnt++;
                    if (!(name[0]=='.'&&(name[1]==0||(name[1]=='.'&&name[2]==0)))) {
                        size_t dl=strlen(dp), nl=strlen(name);
                        char *c=malloc(dl+1+nl+1);
                        if (dl==1&&dp[0]=='/') snprintf(c,dl+1+nl+1,"/%s",name);
                        else snprintf(c,dl+1+nl+1,"%s/%s",dp,name);
                        spush(&dirs,c); free(c);
                    }
                    break;
                case VLNK: symlinks++; break;
                default: other++; break;
                }
                p += e->length;
            }
        }
        if (n<0) errors++;
        close(fd); free(dp);
        if (walked%10000==0) fprintf(stderr,"\r  %lu dirs, %lu files, %.1f / %.1f GB...",
            walked, files, (double)logical/(1024.0*1024*1024), (double)physical/(1024.0*1024*1024));
    }
    fprintf(stderr,"\r                                                              \r");
    fprintf(stderr,"  Files:     %lu\n  Dirs:      %lu\n  Symlinks:  %lu\n  Other:     %lu\n  Errors:    %lu\n",
            files,dircnt,symlinks,other,errors);
    fprintf(stderr,"  Logical:   %.2f GB\n  Physical:  %.2f GB\n",
            (double)logical/(1024.0*1024*1024), (double)physical/(1024.0*1024*1024));
    printf("%lu files, logical=%.2f GB, physical=%.2f GB\n", files,
           (double)logical/(1024.0*1024*1024), (double)physical/(1024.0*1024*1024));
    for (size_t i=0;i<dirs.count;i++) free(dirs.items[i]);
    free(dirs.items);
    return 0;
}
