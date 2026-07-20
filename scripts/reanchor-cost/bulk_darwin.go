//go:build darwin

package main

import (
	"encoding/binary"
	"fmt"
	"os"
	"syscall"
	"unsafe"
)

// getattrlistbulk(2) returns names and attributes for many directory entries per
// syscall, which is why a re-anchor may not have to pay one lstat per child.
//
// The syscall has no wrapper in `syscall` or `golang.org/x/sys/unix`, so we
// invoke it by number. `syscall.Syscall6` on darwin routes through libSystem and
// is marked deprecated upstream; it works (verified on macOS 26.5.2 / Go 1.25.12
// / darwin-arm64), and a production implementation would call the libc symbol
// directly from Rust anyway. `TestBulkMatchesLstat` is the guard: it fails loudly
// if the call or the packed-attribute layout ever stops behaving.
const sysGetattrlistbulk = 461

// attrList mirrors `struct attrlist` from <sys/attr.h>.
type attrList struct {
	bitmapCount uint16
	reserved    uint16
	commonAttr  uint32
	volAttr     uint32
	dirAttr     uint32
	fileAttr    uint32
	forkAttr    uint32
}

const (
	attrBitMapCount = 5

	attrCmnName           = 0x00000001
	attrCmnObjType        = 0x00000008
	attrCmnReturnedAttrs  = 0x80000000
	attrFileDataLength    = 0x00000200
	attrFileDataAllocSize = 0x00000400

	fsoptNoFollow       = 0x00000001
	fsoptPackInvalAttrs = 0x00000008

	vdir = 2 // fsobj_type_t VDIR
)

// Layout of one packed entry:
//
//	 0  uint32   entry length (fixed fields + inline name data)
//	 4  5×uint32 attribute_set_t: which attrs this entry actually carries
//	24  …        the returned attributes, in bitmap order
//
// Only the attributes the entry reports are present, so the fixed part is not a
// constant size: a directory carries no file attributes even with
// FSOPT_PACK_INVAL_ATTRS (verified on macOS 26.5.2, 2026-07-20), so decoding at
// hardcoded offsets reads a directory's name bytes as its size. Walk the
// returned bitmap instead. The kernel packs without padding, so 8-byte values
// can land on 4-byte boundaries: read unaligned via encoding/binary, never by
// casting a struct over the buffer.
const (
	entryHeaderLen = 24 // uint32 length + attribute_set_t
	offReturned    = 4
)

func measureBulk(dir string, bufBytes int) (Result, error) {
	if bufBytes < entryHeaderLen*8 {
		return Result{}, fmt.Errorf("bulk buffer of %d bytes is too small", bufBytes)
	}
	f, err := os.Open(dir)
	if err != nil {
		return Result{}, err
	}
	defer func() { _ = f.Close() }()

	al := attrList{
		bitmapCount: attrBitMapCount,
		commonAttr:  attrCmnReturnedAttrs | attrCmnName | attrCmnObjType,
		fileAttr:    attrFileDataLength | attrFileDataAllocSize,
	}
	buf := make([]byte, bufBytes)

	var res Result
	for {
		n, errno := getattrlistbulk(int(f.Fd()), &al, buf, fsoptNoFollow|fsoptPackInvalAttrs)
		if errno != 0 {
			return Result{}, fmt.Errorf("getattrlistbulk: %w", syscall.Errno(errno))
		}
		if n == 0 {
			break
		}
		batch, err := parseBulkBatch(buf, n)
		if err != nil {
			return Result{}, err
		}
		res.Entries += batch.Entries
		res.Dirs += batch.Dirs
		res.LogicalBytes += batch.LogicalBytes
		res.PhysicalBytes += batch.PhysicalBytes
	}
	return res, nil
}

func getattrlistbulk(fd int, al *attrList, buf []byte, options uintptr) (int, syscall.Errno) {
	r1, _, errno := syscall.Syscall6(
		sysGetattrlistbulk,
		uintptr(fd),
		uintptr(unsafe.Pointer(al)),
		uintptr(unsafe.Pointer(&buf[0])),
		uintptr(len(buf)),
		options,
		0,
	)
	return int(r1), errno
}

// parseBulkBatch walks the `count` packed entries the kernel wrote into buf.
func parseBulkBatch(buf []byte, count int) (Result, error) {
	var res Result
	offset := 0
	for i := 0; i < count; i++ {
		if offset+entryHeaderLen > len(buf) {
			return Result{}, fmt.Errorf("bulk entry %d of %d runs past the buffer (offset %d)", i, count, offset)
		}
		entryLen := int(binary.LittleEndian.Uint32(buf[offset:]))
		if entryLen < entryHeaderLen || offset+entryLen > len(buf) {
			return Result{}, fmt.Errorf("bulk entry %d of %d has implausible length %d at offset %d", i, count, entryLen, offset)
		}
		entry, err := parseBulkEntry(buf[offset : offset+entryLen])
		if err != nil {
			return Result{}, fmt.Errorf("bulk entry %d of %d: %w", i, count, err)
		}
		res.Entries++
		if entry.isDir {
			res.Dirs++
		} else {
			res.LogicalBytes += entry.logical
			res.PhysicalBytes += entry.physical
		}
		offset += entryLen
	}
	return res, nil
}

type bulkEntry struct {
	isDir    bool
	logical  int64
	physical int64
}

// parseBulkEntry decodes one entry by walking the attributes it reports in
// bitmap order. An entry that carries no ATTR_CMN_OBJTYPE would silently count
// as a file, so that one is required rather than assumed.
func parseBulkEntry(entry []byte) (bulkEntry, error) {
	returnedCommon := binary.LittleEndian.Uint32(entry[offReturned:])
	returnedFile := binary.LittleEndian.Uint32(entry[offReturned+12:])

	var out bulkEntry
	cursor := entryHeaderLen
	take := func(n int) ([]byte, bool) {
		if cursor+n > len(entry) {
			return nil, false
		}
		field := entry[cursor : cursor+n]
		cursor += n
		return field, true
	}

	if returnedCommon&attrCmnName != 0 {
		if _, ok := take(8); !ok { // attrreference_t; the name itself is inline later
			return bulkEntry{}, fmt.Errorf("truncated name reference")
		}
	}
	if returnedCommon&attrCmnObjType == 0 {
		return bulkEntry{}, fmt.Errorf("no ATTR_CMN_OBJTYPE returned, cannot tell a directory from a file")
	}
	objType, ok := take(4)
	if !ok {
		return bulkEntry{}, fmt.Errorf("truncated object type")
	}
	out.isDir = binary.LittleEndian.Uint32(objType) == vdir

	if returnedFile&attrFileDataLength != 0 {
		field, ok := take(8)
		if !ok {
			return bulkEntry{}, fmt.Errorf("truncated data length")
		}
		out.logical = int64(binary.LittleEndian.Uint64(field))
	}
	if returnedFile&attrFileDataAllocSize != 0 {
		field, ok := take(8)
		if !ok {
			return bulkEntry{}, fmt.Errorf("truncated data alloc size")
		}
		out.physical = int64(binary.LittleEndian.Uint64(field))
	}
	return out, nil
}
