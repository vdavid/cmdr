//! Real `df -k` output from a Pixel 9 Pro XL: Android 17 (SDK 37), build
//! `CP2A.260805.005`, toybox 0.8.13, captured with `adb shell df -k <path>` on
//! 2026-09-10. Each constant is the command's stdout, verbatim.
//!
//! What the captures show, and what the fake's `df` reproduces from them:
//!
//! - toybox sizes every column to the widest cell of the whole table, header
//!   included, with the `Filesystem` column at least 14 wide, so the header's
//!   spacing changes from one invocation to the next.
//! - The last column is the MOUNT POINT: `/sdcard` (a link to
//!   `/storage/self/primary`, itself a link to `/storage/emulated/0`) reports
//!   `/storage/emulated`, and `/data` reports its bind mount `/data/user/0`.
//! - A missing path still prints the header on stdout, puts its reason on
//!   stderr, and exits 1; a found path beside it keeps its row.
//! - `Use%` is `used / (used + available)`, rounded up.

/// `df -k /`: the read-only system image, 0 available.
pub const DF_K_ROOT: &str = "\
Filesystem       1K-blocks   Used Available Use% Mounted on
/dev/block/dm-12    904496 901736         0 100% /
";

/// `df -k /sdcard`, byte-identical for `/sdcard/`, `/storage/emulated/0`,
/// `/sdcard/Download`, and `/sdcard/DCIM/Camera`.
pub const DF_K_SHARED_STORAGE: &str = "\
Filesystem     1K-blocks     Used Available Use% Mounted on
/dev/fuse      114786388 87698840  26956476  77% /storage/emulated
";

/// `df -k /nonexistent/path`: exit 1, and stderr says
/// `df: '/nonexistent/path': No such file or directory`.
pub const DF_K_MISSING: &str = "\
Filesystem     1K-blocks Used Available Use% Mounted on
";

/// `df -k /sdcard /nonexistent/path`: exit 1, the missing path's reason on
/// stderr. Captured minutes after the rest, so the figures moved.
pub const DF_K_FOUND_AND_MISSING: &str = "\
Filesystem     1K-blocks     Used Available Use% Mounted on
/dev/fuse      114786388 87704028  26951288  77% /storage/emulated
";

/// `df -k /storage/emulated /storage/self/primary /data/local/tmp`: one table
/// for all three, sized to its widest row. Captured with the one above.
pub const DF_K_THREE_PATHS: &str = "\
Filesystem       1K-blocks     Used Available Use% Mounted on
/dev/fuse        114786388 87703204  26952112  77% /storage/emulated
/dev/fuse        114786388 87703204  26952112  77% /storage/emulated
/dev/block/dm-66 114786388 87703204  26952112  77% /data/user/0
";

/// `df -k` with no path: every mount.
pub const DF_K_ALL: &str = "\
Filesystem        1K-blocks     Used Available Use% Mounted on
/dev/block/dm-12     904496   901736         0 100% /
tmpfs               7848516     2184   7846332   1% /dev
tmpfs               7848516        0   7848516   0% /mnt
/dev/block/dm-14     440856   439536         0 100% /system_ext
/dev/block/dm-15    4540448  4526784         0 100% /product
/dev/block/dm-16    1068444  1065132         0 100% /vendor
/dev/block/dm-17      24908    24832         0 100% /vendor_dlkm
tmpfs               7862340       28   7862312   1% /apex
tmpfs               7862340        4   7862336   1% /bootstrap-apex
/dev/block/loop3      60288    60288         0 100% /bootstrap-apex/com.android.virt@370399999
/dev/block/loop0       8620     8580         0 100% /bootstrap-apex/com.android.runtime@1
/dev/block/loop2        804      772        16  98% /bootstrap-apex/com.android.tzdata@370546200
/dev/block/loop1      38088    38056         0 100% /bootstrap-apex/com.android.i18n@1
tmpfs               7862340        0   7862340   0% /tmp
/dev/block/dm-66  114786388 87698840  26956476  77% /data
/dev/block/loop4      80152    80120         0 100% /apex/com.google.android.hardware.biometrics.face@1
/dev/block/loop5     141856   141856         0 100% /apex/com.google.android.gmssystem@13
/dev/block/dm-34      14192    14164         0 100% /apex/com.android.ondevicepersonalization@370547060
/dev/block/dm-47       2688     2660         0 100% /apex/com.android.uprobestats@371508080
/dev/block/loop8     293748   293696         0 100% /apex/com.google.pixel.camera.hal@1915316753
/dev/block/dm-38       2380     2348         0 100% /apex/com.android.profiling@371020780
/dev/block/dm-43      31576    31544         0 100% /apex/com.android.mediaprovider@371513040
/dev/block/loop6       8416     8384         0 100% /apex/com.google.pixel.wifi.ext@1
/dev/block/dm-56        796      764        16  98% /apex/com.android.tzdata@371021100
/dev/block/dm-29       5624     5596         0 100% /apex/com.android.configinfrastructure@371017160
/dev/block/loop14       232      152        76  67% /apex/com.android.hardware.cas@1
/dev/block/dm-22        232       24       204  11% /apex/com.android.resolv@371020260
/dev/block/loop16       628      600        16  98% /apex/com.android.hardware.biometrics.fingerprint.virtual@1
/dev/block/dm-51       5964     5936         0 100% /apex/com.android.webapp@370552000
/dev/block/loop10       232      108       120  48% /apex/com.android.apex.cts.shim@1
/dev/block/dm-64      18600    18568         0 100% /apex/com.android.wifi@371020840
/dev/block/dm-59       3296     3264         0 100% /apex/com.android.os.statsd@371020260
/dev/block/dm-26      29312    29284         0 100% /apex/com.android.permission@371017160
/dev/block/dm-44      21824    21792         0 100% /apex/com.android.adservices@370547060
/dev/block/dm-24      35212    35176         0 100% /apex/com.android.media.swcodec@371018040
/dev/block/dm-39       6240     6208         0 100% /apex/com.android.media@371018040
/dev/block/dm-31      16724    16692         0 100% /apex/com.android.healthfitness@370547060
/dev/block/dm-27       1424     1396         0 100% /apex/com.android.rkpd@370549040
/dev/block/dm-32       6832     6804         0 100% /apex/com.android.uwb@371020500
/dev/block/loop26      5860     5832         0 100% /apex/com.android.devicelock@1
/dev/block/dm-41       4372     4340         0 100% /apex/com.android.appsearch@370549320
/dev/block/dm-33       4744     4716         0 100% /apex/com.android.neuralnetworks@370547060
/dev/block/dm-36      29040    29012         0 100% /apex/com.android.tethering@371021120
/dev/block/dm-55       5528     5500         0 100% /apex/com.android.npumanager@370399999
/dev/block/loop24       364      332        28  93% /apex/com.android.hardware.biometrics.face.virtual@2
/dev/block/dm-42      23160    23132         0 100% /apex/com.android.cellbroadcast@371017160
/dev/block/loop22      8620     8580         0 100% /apex/com.android.runtime@1
/dev/block/dm-50       1136     1104        12  99% /apex/com.android.sdkext@370547060
/dev/block/dm-35        836      808        12  99% /apex/com.android.ipsec@370547060
/dev/block/loop39     60288    60288         0 100% /apex/com.android.virt@370399999
/dev/block/dm-54       6204     6172         0 100% /apex/com.android.extservices@370549343
/dev/block/dm-48      22040    22012         0 100% /apex/com.android.bt@370549400
/dev/block/loop34     38088    38056         0 100% /apex/com.android.i18n@1
/dev/block/dm-57       5560     5532         0 100% /apex/com.android.conscrypt@370549380
/dev/block/loop38      4528     4500         0 100% /apex/com.android.compos@370399999
/dev/block/dm-46       8040     8008         0 100% /apex/com.android.adbd@370547060
/dev/block/dm-58        232      104       124  46% /apex/com.android.scheduling@370549100
/dev/block/dm-53       5012     4984         0 100% /apex/com.android.telephonycore@371021080
/dev/block/dm-52      30600    30560         0 100% /apex/com.android.art@371000140
/dev/block/loop41       232       60       168  27% /apex/com.google.pixel.euicc.update@370399999
/dev/block/loop40     23316    23288         0 100% /apex/com.google.android.widevine@190260313
/dev/block/dm-63      11132    11100         0 100% /apex/com.android.nfcservices@371020820
/dev/block/dm-65        336      304        28  92% /apex/com.android.crashrecovery@370547060
/dev/fuse         114786388 87698840  26956476  77% /storage/emulated
";

/// Data row `n` (0-based, below the header) of a capture.
pub fn data_row(capture: &str, n: usize) -> &str {
    capture
        .lines()
        .nth(n + 1)
        .unwrap_or_else(|| panic!("the capture has no data row {n}"))
}
