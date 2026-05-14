# Total Commander plugin review

## Packer plugins

Reviewed and categorized the packer plugins from both sources. Deduplicated where the same plugin appears in both lists
or in multiple versions, and use these categories:

- **A**: Directly relevant: modern format/feature Cmdr should support natively or as a first-party plugin
- **B**: Inspiring paradigm: the concept ("treat X as an archive", or "use packer UX for non-archive ops") is worth
  borrowing even if the format isn't
- **C**: Niche-but-real: a real audience exists in 2026, but small; fine to leave to third-party plugins
- **D**: Ancient/dead: platform/software is gone (Amiga, Commodore, Spectrum, Atari, Psion, Palm, Outlook Express,
  Webshots, ICQ, Novell, FAR/Win9x-era…)
- **E**: Game/retro-specific: too niche for a general file manager
- **F**: Obscure compressor variants or near-duplicates: irrelevant

### Unified table

| #   | Plugin (deduped)                                                                                                                | Cat | Notes                                                                                                                                                                |
| --- | ------------------------------------------------------------------------------------------------------------------------------- | --- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | 7zip / Total7zip / 7z plugin (multiple versions)                                                                                | A   | 7z is mainstream. Cmdr should handle 7z natively.                                                                                                                    |
| 2   | xz                                                                                                                              | A   | Standard on Linux. Native support expected.                                                                                                                          |
| 3   | lzma                                                                                                                            | A   | Older variant of xz. Native.                                                                                                                                         |
| 4   | BZIP2                                                                                                                           | A   | Standard. Native.                                                                                                                                                    |
| 5   | GZip (implicit, supported by TC core)                                                                                           | A   | Native.                                                                                                                                                              |
| 6   | TAR (implicit, supported by TC core)                                                                                            | A   | Native.                                                                                                                                                              |
| 7   | Z (Unix .Z compress)                                                                                                            | A   | Standard, low effort. Native.                                                                                                                                        |
| 8   | Zstandard                                                                                                                       | A   | Modern, fast. Cmdr should have this from day one.                                                                                                                    |
| 9   | ZPAQ                                                                                                                            | C   | Journaling/dedup archiver. Cool but niche; plugin-worthy.                                                                                                            |
| 10  | PPMd                                                                                                                            | F   | Compression variant; covered by 7z/RAR.                                                                                                                              |
| 11  | hawcx / HA                                                                                                                      | D   | 1990s text archive format. Dead.                                                                                                                                     |
| 12  | LZOP / LZOPackTC                                                                                                                | C   | Real on embedded/Linux. Plugin tier.                                                                                                                                 |
| 13  | Total SQX                                                                                                                       | F   | Obscure proprietary archiver, dead community.                                                                                                                        |
| 14  | dar wcx                                                                                                                         | C   | Real among Linux backup users. Plugin tier.                                                                                                                          |
| 15  | RAR (via Multi-Arc/fhRAR)                                                                                                       | A   | Mainstream. Native (read at minimum).                                                                                                                                |
| 16  | Multi-Arc / MultiArc MVV                                                                                                        | B   | **Big idea**: bridge to external CLI archivers via config. Cmdr's plugin system should make this trivial.                                                            |
| 17  | UnArkWCX (Korean ALZ/EGG via Ark library)                                                                                       | C   | Real in Korea. Plugin.                                                                                                                                               |
| 18  | alz Unpacker                                                                                                                    | C   | Korean format. Plugin.                                                                                                                                               |
| 19  | RPM + CPIO                                                                                                                      | A   | Linux installs are alive and well. Native or first-party plugin.                                                                                                     |
| 20  | DEB                                                                                                                             | A   | Same as above.                                                                                                                                                       |
| 21  | MSI / MSI Plus                                                                                                                  | A   | Windows installer, still standard. Native plugin.                                                                                                                    |
| 22  | CAB / Cab Packer / CabCE                                                                                                        | A   | Still in use (Windows updates, drivers). Plugin.                                                                                                                     |
| 23  | InstallExplorer / IShield (Wise, Vise, Inno, NullSoft, SetupFactory, InstallShield)                                             | C   | Some setups still ship like this. Plugin tier.                                                                                                                       |
| 24  | TotalObserver (multi-format: installers, ISO, MIME, Outlook PST, MBox, game packs)                                              | B   | **Excellent paradigm**: one plugin "discovers" content across many container types. Cmdr could expose this as a unified "open as folder" capability.                 |
| 25  | ISO / TotalISO                                                                                                                  | A   | Standard disc image. Native (read+create).                                                                                                                           |
| 26  | CHMDir (compiled HTML help)                                                                                                     | C   | Still used; plugin-worthy.                                                                                                                                           |
| 27  | HLP / MVB (Windows 3.x help)                                                                                                    | D   | Defunct format.                                                                                                                                                      |
| 28  | ivtdir (Microsoft InfoViewer)                                                                                                   | D   | Dead since ~1999.                                                                                                                                                    |
| 29  | DiskDir / DiskDirW / DiskDir Extended / DiskDirCrc / Disc Maker                                                                 | B   | **Strong paradigm**: a "list of files" file you can browse like a folder. Useful for Cmdr's catalog/snapshot features (offline drives, search results, branch view). |
| 30  | CatalogMaker                                                                                                                    | B   | Same paradigm as DiskDir, customizable formatting.                                                                                                                   |
| 31  | Disk Explorer Professional Viewer                                                                                               | D   | Dead software.                                                                                                                                                       |
| 32  | Storage / decStorageWCX / wcx_storage (.doc, .xls, .msg, Thumbs.db)                                                             | B   | **Strong paradigm**: open compound/structured-storage files as folders. Useful for Office docs, Outlook, etc. Cmdr should expose this.                               |
| 33  | WordArc (open Word docs as archives → text/RTF/HTML)                                                                            | B   | **Strong paradigm**: extract textual representation of any doc through the packer UI. Aligns with Cmdr's AI-native "see inside any file" angle.                      |
| 34  | Thousand Types (quick text preview of any file as virtual archive)                                                              | B   | **Strongest paradigm match for Cmdr**: AI-style "what's in this thing" preview, but rule-based. Cmdr's AI version is the natural evolution.                          |
| 35  | gemini.wcx (Gemini AI Studio conversations as archives)                                                                         | B   | **Direct AI parallel**: open AI conversation exports as folders of txt/md/html + media. Cmdr could do this for Claude/ChatGPT/Gemini exports natively.               |
| 36  | Java Decompiler (open .class as archive of decompiled .java + structure)                                                        | B   | **Strong paradigm**: any binary that has structure can be presented as a folder.                                                                                     |
| 37  | Java Class Unpacker (older)                                                                                                     | B   | Same idea, simpler.                                                                                                                                                  |
| 38  | urlData (extract resources from HTML/CSS)                                                                                       | B   | Paradigm: "extract embedded resources" via packer UI.                                                                                                                |
| 39  | MhtUnPack / MHT Unpacker / MHTep                                                                                                | B   | MHT containers. Same paradigm as urlData. Useful for browser archives.                                                                                               |
| 40  | Mbox                                                                                                                            | C   | Real for email-archive users. Plugin tier.                                                                                                                           |
| 41  | DBX (Outlook Express 5/6)                                                                                                       | D   | Dead.                                                                                                                                                                |
| 42  | wcx_fb2 / fb2wcx / fb2wcx_64 (FictionBook2 e-books)                                                                             | C   | Niche real format (esp. Russia). Plugin.                                                                                                                             |
| 43  | PalmDB zTXT / WCX_PDB / PDB_PRC                                                                                                 | D   | Palm OS. Dead.                                                                                                                                                       |
| 44  | Webcompiler Unpacker                                                                                                            | D   | Dead.                                                                                                                                                                |
| 45  | M3U/M3U8 playlist plugin                                                                                                        | B   | Paradigm: list-of-pointers as archive. Useful pattern for Cmdr (saved searches, tag groups, etc.).                                                                   |
| 46  | Mozilla mozlz4 (Firefox bookmark backup)                                                                                        | C   | Real, very narrow. Plugin tier.                                                                                                                                      |
| 47  | btdir (.torrent as folder)                                                                                                      | B   | Paradigm: metadata as folder structure. Cute, not core.                                                                                                              |
| 48  | Checksum (MD5/SHA list as virtual browser)                                                                                      | B   | **Strong paradigm**: hashing UX through packer interface. Cmdr should have hashing as a first-class feature.                                                         |
| 49  | tthGen (TTH hashes)                                                                                                             | C   | Niche; same idea as Checksum.                                                                                                                                        |
| 50  | LineCount                                                                                                                       | F   | Dev metric, plugin tier.                                                                                                                                             |
| 51  | RegXtract (regex extract to file)                                                                                               | B   | Paradigm: extract-by-pattern as packer. Could be a Cmdr utility.                                                                                                     |
| 52  | decRegWCX / WCReg (REG files as folder)                                                                                         | B   | Paradigm: structured-text-file-as-folder. Could apply to YAML/JSON/TOML browsing in Cmdr.                                                                            |
| 53  | iniPacker (.ini sections as folders)                                                                                            | B   | Same paradigm as REG.                                                                                                                                                |
| 54  | WdxInfoPacker (use content-plugin info as virtual file structure)                                                               | B   | Paradigm: meta-listing.                                                                                                                                              |
| 55  | FileFactory (search + replace + report)                                                                                         | B   | Multi-purpose ops via packer UI.                                                                                                                                     |
| 56  | Total Converter (XML-driven CLI runner)                                                                                         | B   | Same idea as Multi-Arc but for converters.                                                                                                                           |
| 57  | executor.wcx (run command by extension)                                                                                         | F   | Already covered by file associations / open-with.                                                                                                                    |
| 58  | Wcrez / Resource Extractor (PE resources from .exe/.dll)                                                                        | B   | Paradigm: present binary internals as folder. Niche but elegant.                                                                                                     |
| 59  | Lib / LibView (Intel 8051/x86 object libs)                                                                                      | E   | Embedded-dev niche.                                                                                                                                                  |
| 60  | DSP Plugin (VS C++ projects as folder)                                                                                          | D   | VS6-era. Dead.                                                                                                                                                       |
| 61  | ert_wcx (1S:Enterprise meta/ert)                                                                                                | D   | Old Russian ERP. Dead-ish.                                                                                                                                           |
| 62  | Access Viewer (Microsoft Access DB)                                                                                             | C   | Real, narrow.                                                                                                                                                        |
| 63  | LISP packer plugin (AutoCAD)                                                                                                    | C   | Niche-real.                                                                                                                                                          |
| 64  | MATLab plugin (.mat files)                                                                                                      | C   | Real for researchers.                                                                                                                                                |
| 65  | TreeCopyPlus / TreeCopy / CopyTree / DirCopy                                                                                    | B   | **Useful paradigm** for Cmdr: "copy preserving folder structure" via packer UX (rather than as a separate command).                                                  |
| 66  | Mover / English Mover (separate files into folders by criteria)                                                                 | B   | Same paradigm: bulk file ops via packer.                                                                                                                             |
| 67  | SetFolderDate (recursively timestamp dirs from contents)                                                                        | B   | Same paradigm: utility op via packer.                                                                                                                                |
| 68  | Zip2Zero (zip dir-tree as 0-byte files)                                                                                         | B   | Cute paradigm: name-only structure exchange.                                                                                                                         |
| 69  | Hard Link meta-packer / CreateHardLink / CopyLinkTarget                                                                         | B   | **Useful paradigm** for Cmdr: create/follow hard/symlinks via packer UX. Cmdr should have native link ops anyway.                                                    |
| 70  | KillCopy / NSCopy / Sure Copy                                                                                                   | F   | Resilient-copy utilities. Cmdr should bake this in (already a David priority).                                                                                       |
| 71  | Wipe / fobia                                                                                                                    | F   | Secure delete / hide-on-FAT. Niche utility.                                                                                                                          |
| 72  | MakeBAT (write-only, generates .bat)                                                                                            | B   | Paradigm: write-only "packer" as a generator. Cool but Cmdr likely doesn't need it.                                                                                  |
| 73  | Audioconverter / Mod2Wav                                                                                                        | C   | Format-conversion via packer. Real workflow but better as a transform pipeline.                                                                                      |
| 74  | Graphic Converter / TotalRSZ / BMC                                                                                              | C   | Same paradigm: image conversion/resize via packer UX. Cmdr could surface this as a transform action.                                                                 |
| 75  | AVI plugin (frames + audio as archive)                                                                                          | B   | Paradigm: media internals as folder.                                                                                                                                 |
| 76  | GIF / GIFWCX (animated GIF frames)                                                                                              | B   | Same paradigm.                                                                                                                                                       |
| 77  | decJpegWCX (JPEG segments)                                                                                                      | B   | Same; very narrow.                                                                                                                                                   |
| 78  | decMpoWCX (3D JPEG / MPO)                                                                                                       | E   | Niche photography format.                                                                                                                                            |
| 79  | Red JPEG / StegoTC / StegoTC G2                                                                                                 | E   | Steganography. Too niche.                                                                                                                                            |
| 80  | DarkCrypt IV / Cryptonite / PUZZLE / AES plugin / BFC                                                                           | C   | Encryption plugins. Cmdr should have **modern AES/age encryption** native, but not 100+ legacy ciphers.                                                              |
| 81  | Kryptel                                                                                                                         | C   | Commercial encryption. Plugin.                                                                                                                                       |
| 82  | LowTraffic (FidoNet 8-bit-clean encoding)                                                                                       | D   | Dead.                                                                                                                                                                |
| 83  | Blat Mailer (SMTP send via packer)                                                                                              | F   | Better as a separate "send" action.                                                                                                                                  |
| 84  | WebArc                                                                                                                          | D   | Old custom web-upload thing.                                                                                                                                         |
| 85  | Webshots                                                                                                                        | D   | Software dead since 2012.                                                                                                                                            |
| 86  | ICQScheme                                                                                                                       | D   | ICQ dead.                                                                                                                                                            |
| 87  | Novell DSView                                                                                                                   | D   | Novell dead.                                                                                                                                                         |
| 88  | Far2wc (run FAR plugins inside TC)                                                                                              | D   | Bridge to a competitor's plugin system; not a fit for Cmdr.                                                                                                          |
| 89  | Power Packer (Amiga)                                                                                                            | D   | Amiga.                                                                                                                                                               |
| 90  | AmigaDX (.adf etc.)                                                                                                             | D   | Amiga emulator users only.                                                                                                                                           |
| 91  | LZX / UnLZX (Amiga)                                                                                                             | D   | Amiga.                                                                                                                                                               |
| 92  | Dircbm / D64 (Commodore disk images)                                                                                            | D   | Retro.                                                                                                                                                               |
| 93  | inSCL / inTRD / inMBD / inMBH / inTAP / inHrust / BS-DOS bundle (ZX Spectrum)                                                   | D   | Retro, all variants of Spectrum disk/tape images.                                                                                                                    |
| 94  | TRD / SCL (older Spectrum)                                                                                                      | D   | Same.                                                                                                                                                                |
| 95  | casMSXwcx (MSX tape)                                                                                                            | D   | Retro.                                                                                                                                                               |
| 96  | ATR.wcx (Atari 8-bit)                                                                                                           | D   | Retro.                                                                                                                                                               |
| 97  | Imaginator / IMG(1) / IMG(2) / UnImz                                                                                            | C   | Floppy disk images (1.44MB). Mostly retro now, occasionally needed. Plugin tier.                                                                                     |
| 98  | FATImage (FAT12/16/32 + MBR)                                                                                                    | C   | Modern, but still niche (mostly retro/embedded). Plugin.                                                                                                             |
| 99  | CPMImage (CP/M disk images)                                                                                                     | D   | Retro.                                                                                                                                                               |
| 100 | EnsoniqUnpacker / EnsoniqUnpackerEFE                                                                                            | D   | Dead instrument format.                                                                                                                                              |
| 101 | ICL / ICLRead / ICONew / ICO wcx                                                                                                | D   | Win9x icon library format. Effectively dead.                                                                                                                         |
| 102 | SFF (fax)                                                                                                                       | D   | Faxing is dead in 2026.                                                                                                                                              |
| 103 | SIS / PDUnSIS                                                                                                                   | D   | Symbian/Psion dead.                                                                                                                                                  |
| 104 | Windows Media Audio (WPD-based device access)                                                                                   | C   | WPD/MTP is real (mtp-rs in your stack already!). But this specific plugin is dated.                                                                                  |
| 105 | MS-Compress (.??\_)                                                                                                             | D   | Old Windows installer fragments.                                                                                                                                     |
| 106 | AlbumPack (AlbumWrap MP3)                                                                                                       | D   | Dead format.                                                                                                                                                         |
| 107 | UFO VFS / UFO Aftermath                                                                                                         | E   | Game.                                                                                                                                                                |
| 108 | RisenPak                                                                                                                        | E   | Game.                                                                                                                                                                |
| 109 | X3 (Egosoft games)                                                                                                              | E   | Game.                                                                                                                                                                |
| 110 | UMOD / Unreal Tournament                                                                                                        | E   | Game.                                                                                                                                                                |
| 111 | Operation Flashpoint PBO                                                                                                        | E   | Game.                                                                                                                                                                |
| 112 | RAF (League of Legends)                                                                                                         | E   | Game.                                                                                                                                                                |
| 113 | PSARC (PS3)                                                                                                                     | E   | Console.                                                                                                                                                             |
| 114 | LTAR (FEAR 2 / Condemned 2)                                                                                                     | E   | Game.                                                                                                                                                                |
| 115 | MPQ Plugin (Blizzard)                                                                                                           | E   | Game.                                                                                                                                                                |
| 116 | GCF (Valve Steam)                                                                                                               | E   | Game (legacy Steam).                                                                                                                                                 |
| 117 | RisenPak / GAUP / S.T.A.L.K.E.R. db / H4R / Heroes III pack / WADFile (Doom) / GPAK (Quake) / PACK (Quake) / GRP (Duke3D) / TOW | E   | Game-specific archives.                                                                                                                                              |
| 118 | Total7zip + 7z.dll wrapper                                                                                                      | A   | Already covered (#1).                                                                                                                                                |
| 119 | TotalZAR (zip-of-rars)                                                                                                          | F   | Edge case wrapper.                                                                                                                                                   |
| 120 | targzbz2 (read-only tar.gz/tar.bz2)                                                                                             | A   | Already covered by tar.                                                                                                                                              |
| 121 | Z4 Archive                                                                                                                      | F   | Obscure custom format.                                                                                                                                               |
| 122 | LZ8Comp / UkrPack / APLibTC / ABC-TC / JustBZLP / JustZip / UCComp / LZRComp / ELKA / SSSR / PPMPackTC / KolchCrypt             | F   | "DarkLib/UTO" obscure compressor zoo by one author. Effectively dead variants of standard compression.                                                               |
| 123 | UnPSF (PackSafeFormat)                                                                                                          | D   | Dead utility.                                                                                                                                                        |
| 124 | GCA plugin                                                                                                                      | D   | Defunct Japanese archiver.                                                                                                                                           |
| 125 | ExtrKarText (MIDI/KAR lyrics)                                                                                                   | E   | Karaoke files. Niche fun.                                                                                                                                            |
| 126 | Progress PL (PROGRESS procedure libraries)                                                                                      | C   | Real for legacy Progress 4GL devs. Plugin tier.                                                                                                                      |
| 127 | DBX config tool / Disk Explorer Professional / others above                                                                     | D   | Already counted.                                                                                                                                                     |
| 128 | Cab Packer (Microsoft)                                                                                                          | A   | Already counted at #22.                                                                                                                                              |

### Stats

Counted by deduplicated row, treating the lumped rows (#117, #122) as single items:

Total deduplicated entries: **~117**

| Category                                               | Count |    % | Meaning for Cmdr                                                                                                                                                                                                                                                                                                                                                |
| ------------------------------------------------------ | ----: | ---: | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **A**: Directly relevant (mainstream formats)          |    14 | ~12% | Cmdr's native must-haves: zip/7z/tar/gz/bz2/xz/zstd/lzma/Z, RAR, deb/rpm/cpio, MSI/CAB, ISO.                                                                                                                                                                                                                                                                    |
| **B**: Inspiring paradigm                              |    25 | ~21% | The single biggest takeaway: TC's "packer" is really a _generic transform/listing surface_. Cmdr's plugin API should let plugins present **anything** (compound docs, .class files, AI conversations, MHT, regex matches, REG/INI sections, hash lists, branch views, structured copies) as a browsable folder, and let "pack/unpack" double as a generator UX. |
| **C**: Niche-but-real                                  |    18 | ~15% | Punt to community plugins: dar, alz, fb2, mozlz4, Mbox, CHM, MATLab, Access, LISP, FATImage, IMG, InstallShield/Wise/Inno, ZPAQ, encryption legacies.                                                                                                                                                                                                           |
| **D**: Ancient/dead platform                           |    32 | ~27% | Skip entirely: Amiga, Commodore, Spectrum, Atari, MSX, CP/M, Psion, Palm, Outlook Express, Webshots, ICQ, Novell, FidoNet, Win 3.x help, ICL, fax, AlbumWrap, etc.                                                                                                                                                                                              |
| **E**: Game/retro-specific                             |   ~17 | ~14% | Skip: PS3, LoL, Doom, Quake, Heroes, FEAR, MPQ, Steam GCF, Unreal, Flashpoint, X-series, Risen, etc.                                                                                                                                                                                                                                                            |
| **F**: Obscure variants/duplicates/redundant utilities |    11 |  ~9% | Skip: DarkLib compressor zoo, Z4, GCA, UnPSF, executor, MakeBAT, etc.                                                                                                                                                                                                                                                                                           |

### Headline takeaways

- **Roughly one-third of the catalog (A + B = ~33%) is genuinely useful as input for Cmdr**, but in two very different
  ways. Category A tells you which formats to ship native; Category B tells you how to think about your _plugin API_.
- **More than half the catalog (D + E + F = ~50%) is dead or game-only**: a strong signal that long-tail plugin support
  has historically been retro-computing hobby work, not productive enterprise.
- **The single most interesting insight**: TC's packer interface unintentionally became a "structured-content browser"
  (compound docs, .class files, MHT, REG, AI chat exports, INI). For an AI-native file manager, **this is the killer
  paradigm to formalize**: any file with internal structure should be enterable. Cmdr can do far better than TC because
  the AI can describe internals on the fly even without a dedicated plugin.
- **Notable modern signal**: `gemini.wcx` (March 2026): someone is already shoehorning AI-conversation exports into the
  TC packer model. Cmdr should make that a native, not a plugin.

## File system plugins

### Unified table

| #   | Plugin (deduped)                                                                                                                        | Cat | Notes                                                                                                                                                                                    |
| --- | --------------------------------------------------------------------------------------------------------------------------------------- | --- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Cloud (Dropbox, OneDrive, Google Drive, Box, Yandex disk, Strato HiDrive)                                                               | A   | **First-tier must-have** for Cmdr. Ghisler's own plugin covers exactly the major consumer clouds.                                                                                        |
| 2   | Google Drive (separate plugin, older)                                                                                                   | A   | Subsumed by #1.                                                                                                                                                                          |
| 3   | S3 Browser                                                                                                                              | A   | Mainstream cloud storage. Cmdr should support S3 (and S3-compatible: R2, B2, Wasabi).                                                                                                    |
| 4   | AzureBlob                                                                                                                               | A   | Real for devs. Same family as #3.                                                                                                                                                        |
| 5   | CloudMailRu                                                                                                                             | C   | Russian cloud, narrow geographic audience.                                                                                                                                               |
| 6   | SABManagerTC (SecureAnyBox)                                                                                                             | C   | Commercial encrypted cloud. Plugin tier.                                                                                                                                                 |
| 7   | WebDAV                                                                                                                                  | A   | Standard protocol: Nextcloud, ownCloud, NAS boxes. Native must-have.                                                                                                                     |
| 8   | SFTP (Ghisler / Hans Petrich / libssh2 variant, multiple)                                                                               | A   | **Critical**. Must work natively. Cmdr already cares about this.                                                                                                                         |
| 9   | FTP over SSL/TLS                                                                                                                        | A   | FTPS still alive in older infra. Native.                                                                                                                                                 |
| 10  | FTP List / TestFTP / FTP monitor                                                                                                        | F   | TC's FTP backend already covers this; meta-tools redundant.                                                                                                                              |
| 11  | DiskInternals Reader (ext2/3/4, ReiserFS, HFS/HFS+, NTFS, ReFS, FAT/exFAT, UFS2, soft-RAID, MBR/GPT/APFS, VMware/VBox/Parallels images) | A   | **Massive**: read non-Windows filesystems and VM disks from Windows. Cmdr (Rust, cross-platform) actually has it easier, but the _coverage list_ here is a great target.                 |
| 12  | Ext2+Reiser / ext4tc                                                                                                                    | A   | Subset of #11; ext4 is the headline.                                                                                                                                                     |
| 13  | NTFS FileStreams                                                                                                                        | B   | **Strong paradigm**: NTFS Alternate Data Streams as a sub-folder. Cmdr should expose ADS, xattrs, and macOS resource forks similarly.                                                    |
| 14  | NTFS4TC (NTFS images, locked-files access, custom dynamic disks)                                                                        | C   | Niche; partly covered by #11.                                                                                                                                                            |
| 15  | Virtual Disk (mount ISO/BIN/NRG/IMG as drives)                                                                                          | B   | Paradigm: present a disk image as a real drive. Cmdr can do "open-as-folder" without OS-level mounting (already aligns with how mtp-rs/smb2 work).                                       |
| 16  | XBox DVD / cpmimg / EnsoniqFS / Iriver Storage                                                                                          | E/D | Game disc / retro / dead audio gear.                                                                                                                                                     |
| 17  | Android ADB / TotalAndDroid                                                                                                             | A   | Android over ADB is alive and central. Cmdr should ship Android support either via ADB (dev-flow) or MTP (consumer flow).                                                                |
| 18  | iOS (T-PoT 1.1/1.3, iPlugin, wfx_iOS via libimobiledevice)                                                                              | A   | iOS access is a real Cmdr feature gap. `libimobiledevice` is the right path.                                                                                                             |
| 19  | iPod (early, iTunesDB)                                                                                                                  | D   | Classic iPod era. Dead.                                                                                                                                                                  |
| 20  | Windows Media Audio 2 (WPD/MTP for MP3 players + Android)                                                                               | A   | David already has `mtp-rs`; this validates the relevance.                                                                                                                                |
| 21  | Windows Media Audio (older WMDM)                                                                                                        | D   | Pre-WPD. Dead.                                                                                                                                                                           |
| 22  | CanonCam / WIACam                                                                                                                       | D   | Modern cameras present as MTP/MSC; vendor SDKs irrelevant in 2026.                                                                                                                       |
| 23  | SymbFS / SGHFS / SIFS / Siemens DES / VNavigator Siemens Obex / NokiaFS / MotoPK / EFS / EFS2splugin / Brew Mobile                      | D   | Symbian, Siemens, Motorola P2K, Nokia non-smart, BREW: entire feature-phone era.                                                                                                         |
| 24  | HPLX (HP100/200LX palmtop)                                                                                                              | D   | 1990s palmtop.                                                                                                                                                                           |
| 25  | REB1100 (Rocket eBook)                                                                                                                  | D   | Late-1990s ebook reader.                                                                                                                                                                 |
| 26  | Diamond Rio PMP300 / Mpio                                                                                                               | D   | First-generation MP3 players.                                                                                                                                                            |
| 27  | TCChibiOSFS (HackRF + PortaPack, Flipper Zero serial shell)                                                                             | C   | Niche but **alive** (Flipper community is real in 2026). Plugin tier.                                                                                                                    |
| 28  | Bluetooth OBEX Object Push                                                                                                              | C   | Bluetooth file-push is rare on desktop now. Plugin tier.                                                                                                                                 |
| 29  | Serial (RS232 + Palm-USB serial)                                                                                                        | D   | Effectively dead.                                                                                                                                                                        |
| 30  | GitHubFS (browse repos, branches, releases, archives in-place)                                                                          | A   | **Direct hit** for Cmdr. Browse remote Git repos as folders, with auth and release-asset download. Cmdr should ship this natively, with a much better AI-driven discovery layer.         |
| 31  | gitview (local git branches/tags as folders)                                                                                            | B   | Paradigm: VCS state as folder structure. Cmdr could surface `git worktree`/branches similarly.                                                                                           |
| 32  | CVSBrowser / Visual SourceSafe / MKS Integrity / TFS Version Control                                                                    | D   | All defunct or fading VCSs.                                                                                                                                                              |
| 33  | DocClassifier (tag-based virtual folders, separate tagger app)                                                                          | B   | **Strong paradigm**: dynamic foldering by tags rather than path. Aligns perfectly with Cmdr's AI-native classification potential.                                                        |
| 34  | MP3Commander / MP3 Database / TWinAmp / TWinAmp2 / TWinAmp3 / TMedia                                                                    | B/D | Paradigm OK (organize music by metadata as folders). Implementations all dead/abandoned.                                                                                                 |
| 35  | Sequences (group serially numbered files into virtual entries)                                                                          | B   | Paradigm: smart grouping. Useful for camera/scan output.                                                                                                                                 |
| 36  | Branch View Extended (recursive branch with sizes)                                                                                      | B   | TC's own Branch View, supercharged. Cmdr's branch view should be fast and recursive by default.                                                                                          |
| 37  | Temporary Panel / VirtualPanel / TempDrive / Link drive / File Redirector                                                               | B   | **Useful paradigm**: persistent virtual folder of pointers to real files. Cmdr should support "scratch panels" / saved selections / tag-based collections.                               |
| 38  | Registry / TurboRegistry / CoRegistry                                                                                                   | C   | Windows-only utility. Plugin tier.                                                                                                                                                       |
| 39  | Environment Variables Ex (and older)                                                                                                    | B   | Paradigm: OS state as files (env vars edited as files). Cmdr could expose env, paths, ulimits similarly.                                                                                 |
| 40  | Services2 / TC Services                                                                                                                 | C   | Windows-only. Plugin tier.                                                                                                                                                               |
| 41  | PROC / ProcFS / procViewer / AceHelper                                                                                                  | B   | **Linux-style paradigm**: processes as a directory. Cmdr could mirror `/proc` semantics cross-platform.                                                                                  |
| 42  | Events NT / System Events Ex                                                                                                            | C   | Windows event log. Plugin tier.                                                                                                                                                          |
| 43  | Startup Guard / Startups / RedGUARD                                                                                                     | C   | Windows startup management. Plugin tier.                                                                                                                                                 |
| 44  | Uninstaller / Uninstaller64                                                                                                             | C   | Same.                                                                                                                                                                                    |
| 45  | Device Manager / RedDetect / RedSmart / RedOHM                                                                                          | C   | System-info plugins. Useful but Windows-only and dated.                                                                                                                                  |
| 46  | NetStat / FSNetStat / FSNetShare / Shared Files / LAN Seeker / NetworkAlt                                                               | C   | Network-info / Windows-share browsing. Some are dated, but the _category_ (network as a tree) is valuable for Cmdr.                                                                      |
| 47  | Tweak Collector / FDC TC / RedLOCK                                                                                                      | D   | Windows XP-era system tweaks. Dead.                                                                                                                                                      |
| 48  | DialPWD (cached Win9x dial-up passwords)                                                                                                | D   | Win9x.                                                                                                                                                                                   |
| 49  | Privileges                                                                                                                              | F   | Windows token privileges. Trivial.                                                                                                                                                       |
| 50  | TipTop (always-on-top / window opacity)                                                                                                 | F   | Window-manager utility, not a filesystem.                                                                                                                                                |
| 51  | WinWalk (enumerate windows)                                                                                                             | F   | Same.                                                                                                                                                                                    |
| 52  | CPL (Control Panel applets in panel)                                                                                                    | F   | Trivial wrapper.                                                                                                                                                                         |
| 53  | TConsole / TotalConsole                                                                                                                 | B   | **Strong paradigm** for Cmdr: console _as a panel_. Cmdr should have a first-class integrated terminal/REPL panel.                                                                       |
| 54  | Calendar                                                                                                                                | F   | Calendar appliance, not a file system.                                                                                                                                                   |
| 55  | RSS Reader                                                                                                                              | C   | Feeds-as-files. RSS is alive in 2026 but a niche power-user thing.                                                                                                                       |
| 56  | tcPhonebook                                                                                                                             | C   | vCard contacts as files. Niche but real for offline contact mgmt.                                                                                                                        |
| 57  | PassStore                                                                                                                               | C   | Encrypted password manager as fs. Real paradigm but better tools (1Password, Bitwarden) own this.                                                                                        |
| 58  | decClipboardFS / FSClipboard / RedClipboard                                                                                             | B   | **Strong paradigm**: clipboard contents as files (text, image, file-list, RTF). Cmdr should expose clipboard history this way.                                                           |
| 59  | DocClassifier (already #33)                                                                                                             | —   | —                                                                                                                                                                                        |
| 60  | HTTP browser / HTTP SmartBrowser / HTTPS Browser / HTTP base / Versions                                                                 | B   | Paradigm: browse a site's link graph as folders. Less useful in 2026 (SPAs, JS-rendering) but Cmdr could do "URL → fetched content tree" via headless browser/AI.                        |
| 61  | Webshots / Delicious Posts / wfx_Opera / wfx_ONotes / OperaFS / MSIE Cache Browser / MirandaFS / photofile                              | D   | All defunct services or abandoned browsers/IM.                                                                                                                                           |
| 62  | POP3/SMTP EmailPlugin / MAIL_WFX / POP3&SMTP                                                                                            | C   | Email as VFS. Real paradigm; modern users on IMAP/Gmail/etc. instead.                                                                                                                    |
| 63  | ADO Data Sources / MS SQL Servers                                                                                                       | B   | **Strong paradigm**: a database (tables, views, procs) as folders. Cmdr could expose SQLite/Postgres files this way as a first-class capability.                                         |
| 64  | OPC DA                                                                                                                                  | C   | Industrial-automation. Niche but alive.                                                                                                                                                  |
| 65  | RedOneC (1C:Enterprise)                                                                                                                 | D   | Russian ERP, narrow audience.                                                                                                                                                            |
| 66  | RadminPlg / RadminPlg64 / TCRadmin                                                                                                      | D   | Radmin ecosystem, niche/dead in West.                                                                                                                                                    |
| 67  | CDDataBase / Disc Maker / catalog plugins (also covered in packer review)                                                               | B   | Paradigm: offline media catalog. Useful for Cmdr if you handle external drives.                                                                                                          |
| 68  | Back2Life (undelete FAT/NTFS)                                                                                                           | C   | Real utility; better as a separate tool than core file manager.                                                                                                                          |
| 69  | badcopy (read damaged media)                                                                                                            | C   | Real utility. Plugin tier.                                                                                                                                                               |
| 70  | Wipe (FS) / fobia                                                                                                                       | F   | Secure delete. Cmdr should have it as a built-in action, not a "filesystem".                                                                                                             |
| 71  | RamCopy / Temp drive / Sequences (RAM-disk variants)                                                                                    | F   | OS-level RAM disks supersede this.                                                                                                                                                       |
| 72  | ComplexCD / Complex TC burner / Neropanel / CD/DVD Burning Plugin / CD-Ripper                                                           | D   | Optical-disc burning is dead consumer-side in 2026.                                                                                                                                      |
| 73  | TFS Version Control / Visual SourceSafe / MKS Integrity / CVSBrowser                                                                    | D   | Already at #32, defunct VCS.                                                                                                                                                             |
| 74  | AGacVEd (.NET GAC viewer/editor)                                                                                                        | C   | .NET Framework GAC mostly irrelevant on .NET 5+.                                                                                                                                         |
| 75  | OPC DA (already #64)                                                                                                                    | —   | —                                                                                                                                                                                        |
| 76  | FB2Lib                                                                                                                                  | C   | E-book library. Niche.                                                                                                                                                                   |
| 77  | TotalUpgrade                                                                                                                            | F   | Compares two TC installs, TC-internal.                                                                                                                                                   |
| 78  | PluginManager                                                                                                                           | F   | TC-internal. Cmdr will need its own equivalent.                                                                                                                                          |
| 79  | DotNet Wrapper / Perl FS / ScriptWFX                                                                                                    | B   | **Strong paradigm**: write FS plugins in scripting languages (.NET / Perl / VBScript). Cmdr should expose a high-level plugin SDK in TS/JS/Python so non-Rust devs can write FS plugins. |
| 80  | Transformer Framework (generic transform-plugin framework)                                                                              | B   | **Strong paradigm**: a sub-framework for "do anything to files" plugins, separate from the main FS interface. Cmdr's transform pipeline should be a first-class concept.                 |
| 81  | STALKER db explorer                                                                                                                     | E   | Game.                                                                                                                                                                                    |
| 82  | TCChibiOSFS (already #27)                                                                                                               | —   | —                                                                                                                                                                                        |

### Stats

Counted by deduplicated row, treating the lumped rows (#16, #23, #34, #37, #38, #41, #42, #43, #44, #45, #46, #47, #58,
#60, #61, #62, #66, #72, #73, #79) as single items.

Total deduplicated entries: **~70**

| Category                                     | Count |    % | Meaning for Cmdr                                                                                                                                                                                                                                                                                                                        |
| -------------------------------------------- | ----: | ---: | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **A**: Directly relevant (formats/protocols) |    11 | ~16% | The native must-haves: Cloud (Dropbox/OneDrive/Drive/Box/iCloud/etc.), S3-compatible, Azure Blob, SFTP, FTPS, WebDAV, MTP/WPD, ADB, libimobiledevice (iOS), GitHub-as-VFS, ext/HFS/APFS/exFAT/ReFS readers, VM-disk-image readers.                                                                                                      |
| **B**: Inspiring paradigm                    |    17 | ~24% | The plugin API should expose: tag/classifier-based virtual folders, processes/services/env/registry as folders, clipboard as folder, ADS/xattrs as folders, DB tables as folders, console-as-panel, "temp panel"/saved-selection panels, NTFS-stream-style sub-entries, scriptable plugin SDK (TS/Python), generic transform framework. |
| **C**: Niche-but-real                        |    18 | ~26% | Plugin tier: Flipper/HackRF, Bluetooth OBEX, OPC DA, RSS, vCard, password stores, undelete, badcopy, system info (services/events/startups/uninstaller/SMART), POP3/IMAP-as-fs, Russian-cloud, encryption boxes.                                                                                                                        |
| **D**: Ancient/dead platform                 |    17 | ~24% | Skip: Symbian/feature-phones (Siemens, Motorola, Nokia, Samsung, BREW), Palm, HPLX, Rocket eBook, classic iPod, first-gen MP3 players (Rio, Mpio, Iriver), WinCE, Win9x, Outlook Express, Webshots, ICQ, Miranda, Delicious, MSIE cache, Opera bookmarks, defunct VCSs (CVS/VSS/MKS/TFS), 1C, Radmin, optical burners.                  |
| **E**: Game/retro-specific                   |     2 |  ~3% | Skip: STALKER, XBox DVD.                                                                                                                                                                                                                                                                                                                |
| **F**: Utility/redundant                     |     7 | ~10% | Skip or fold into Cmdr core: secure-delete, RAM disk, plugin manager, calendar, window manager utilities, TC-internal helpers.                                                                                                                                                                                                          |

### Headline takeaways

- **A + B = ~40% of the catalog is signal**, substantially higher than the packer plugins (~33%). FS plugins map
  directly onto Cmdr's core: remote/cloud filesystems and "treat anything structured as a tree".
- **The single biggest input from this list is the cloud/protocol coverage matrix** (A): Dropbox, OneDrive, Google
  Drive, Box, S3, Azure Blob, WebDAV, SFTP, FTPS, ADB, libimobiledevice, MTP/WPD, GitHub-as-VFS, ext/HFS/APFS readers.
  That's roughly the "remote backends" roadmap.
- **The second biggest input is the paradigm of "OS state as a folder"** (B): processes, services, env vars, registry,
  clipboard, NTFS streams, DB tables, git branches, tag-based virtual collections. Cmdr can take this further than TC
  because (a) it's cross-platform, and (b) AI can dynamically describe internals so plugins aren't strictly required for
  novel sources.
- **`GitHubFS` (May 2026) and `gitview` (June 2025) are the most modern entries** in the whole TC ecosystem, both about
  Git. That's a strong signal: developers want their VCS state browsable as files. Cmdr should ship this natively.
- **`DocClassifier` + `MP3Commander` are the most interesting "old" ideas**: dynamic folders generated from
  tags/metadata. This is exactly what Cmdr's AI layer can do without requiring users to manually tag.
- **Almost all the device-specific plugins (Symbian/Palm/feature phones/old MP3 players, ~25%) are dead**: modern
  equivalents collapse into ADB, libimobiledevice, and MTP/WPD. Three protocols cover what used to take 30+ plugins.
- **`ScriptWFX` / `Perl FS` / `DotNet Wrapper` / `Transformer Framework`** prove the value of a _scriptable_ plugin SDK.
  Cmdr should expose a TS/Python-level FS plugin API in addition to the Rust core.

## Content plugins

Same structure (table → stats → takeaways), grouped aggressively by file-type family because content plugins are highly
redundant within each family.

### Unified table

| #   | Plugin family (deduped)                                                                                                                                                                                                                              | Cat | Notes                                                                                                                                                                                                                                                                                                                       |
| --- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | **Audio metadata** (Anytag, AudioInfo, id3, MP3Info, WDXTagLib, decID3WDX)                                                                                                                                                                           | A   | ID3/Vorbis/FLAC/MP4/WMA/Opus/etc. tags. Cmdr should expose these as native columns (artist, album, duration, bitrate).                                                                                                                                                                                                      |
| 2   | **Universal media** (MediaInfo, MediaInfoWDX, TCMediaInfo, Media, MKInfoCP, MediaTime)                                                                                                                                                               | A   | `mediainfo` is the canonical lib for video/audio/container metadata. Cmdr should integrate it. `MediaTime` adds _recursive duration sum_, a useful aggregation pattern.                                                                                                                                                     |
| 3   | **Image metadata: EXIF/IPTC/XMP** (Exif × 3 versions, ExifToolWDX, ImageMetaData, JPG-Comment, decAdobeSaveWDX)                                                                                                                                      | A   | EXIF/IPTC/XMP must be native (camera, GPS, lens, copyright). Use Exiv2 or libexif.                                                                                                                                                                                                                                          |
| 4   | **Image format info** (ImgSize, WDX for Images, Image Info, PngInfo, JpegQuality, PsdInfo, TiffInfo, WebpInfo, SVGInfo/SVGwdx, decIcoWDX)                                                                                                            | A   | Width/height/bit-depth/orientation/aspect for all common formats: universal columns. JpegQuality is a nice extra.                                                                                                                                                                                                           |
| 5   | **Camera RAW** (covered in #3 via Exif 2.7: CR3, ORF, RAW, RW2, ARW, PEF, RAF)                                                                                                                                                                       | A   | Photographers care; Cmdr should support RAW EXIF.                                                                                                                                                                                                                                                                           |
| 6   | **PE/EXE/DLL info** (ExeFormat, ExeInfo, wdx_exeinfo, FileScanner, IsDotNET, decPeExtraWDX, Bitchaos)                                                                                                                                                | C   | Useful for devs and reverse engineers. Plugin tier. `Bitchaos` is interesting (entropy + VirusTotal lookup); see #20.                                                                                                                                                                                                       |
| 7   | **Office docs** (CDocProp, MSWord WDX, Office2007wdx, OpenOfficeInfo, RichText)                                                                                                                                                                      | A   | Title/author/word-count/track-changes for `.docx`/`.xlsx`/`.pptx`/`.odt` etc. Should be native; AI can also derive on demand.                                                                                                                                                                                               |
| 8   | **PDF metadata + full-text** (xPDFSearch, pdfOCR)                                                                                                                                                                                                    | A   | Page count, title, author, full-text search, and "needs-OCR" page count. All worth surfacing.                                                                                                                                                                                                                               |
| 9   | **Generic full-text via converters** (TextSearch, PCREsearch with text filter)                                                                                                                                                                       | A   | Wraps RTF / ODF / DOC / DOCX / PDF → plain text for grep. Cmdr's AI/index layer naturally supersedes this.                                                                                                                                                                                                                  |
| 10  | **Archive contents** (RarInfo / RarInfoNew / RarColumns, ZipType, 7zip Info, Total SQX Content)                                                                                                                                                      | A   | File count, ratio, encryption flag, comment, solid flag. Native.                                                                                                                                                                                                                                                            |
| 11  | **Linux package metadata** (RPM_wdx, Debian Linux package)                                                                                                                                                                                           | A   | Subset of #10, covered with deb/rpm packer support.                                                                                                                                                                                                                                                                         |
| 12  | **Hashes** (LotsOfHashes, 47 algos; HashSys, wdHash, crc32tag)                                                                                                                                                                                       | A   | MD5/SHA-1/SHA-256/SHA-512/CRC32/BLAKE3 must be native columns. The 47-algorithm set is overkill; cap at ~6.                                                                                                                                                                                                                 |
| 13  | **EML / email metadata** (EML New, EML info, wdx_Eml)                                                                                                                                                                                                | C   | Subject/from/to/date columns for `.eml`. Real for offline mail browsers. Plugin tier or native.                                                                                                                                                                                                                             |
| 14  | **Date/age columns** (Today, Tempus, Age, DateNames, FileDateTime)                                                                                                                                                                                   | B   | **Strong paradigm**: derived date columns ("3 days ago", "Q1-2026", weekday, month-name). Cmdr should ship rich relative dates and let users add custom ones.                                                                                                                                                               |
| 15  | **File descriptions** (File descriptions, wdx_global_diz, NTFS Descriptions)                                                                                                                                                                         | B   | Read description from `descript.ion` / `files.bbs` / NTFS streams / first lines / version info. Concept lives on as "comments": Cmdr could store comments in xattrs/ADS and AI could auto-generate them.                                                                                                                    |
| 16  | **Links/streams** (NL_Info, NTLinks, NTFSFileStreams, NTFS Stream)                                                                                                                                                                                   | A   | Hardlinks, junctions, symlinks, ADS: Cmdr should display all of these (covered in FS-plugin review #13 too).                                                                                                                                                                                                                |
| 17  | **Text encoding / line breaks** (EncInfo, LineBreakInfo, wdx_Code, cputil, NFCname, UnicodeTest)                                                                                                                                                     | A   | Encoding (UTF-8/16/ANSI), BOM, CRLF/LF/CR/mixed, NFC/NFD normalization. Cmdr should expose these: they matter for cross-platform sync (NFC/NFD is a real macOS↔Windows gotcha).                                                                                                                                             |
| 18  | **E-books** (eBookInfo: MOBI/AZW/AZW3/EPUB/PRC, Fast Fb2 Epub, Fast Fb2 wdx, wdx_xml, anyXML)                                                                                                                                                        | C   | EPUB/MOBI metadata. Real for Calibre-style users. Plugin tier.                                                                                                                                                                                                                                                              |
| 19  | **File-type detection** (TrID, TrID_Identifier, MIME Info, RegInfo)                                                                                                                                                                                  | A   | Magic-byte sniffing should be native: `tree_magic` / `infer` crates in Rust. Don't rely on extension alone.                                                                                                                                                                                                                 |
| 20  | **Bitchaos (heuristic malware analysis: entropy, signed?, suspicious imports, VirusTotal hash lookup)**                                                                                                                                              | B   | **Strong paradigm**: a column-as-classifier. Could inspire a Cmdr "trust" column powered by signature + reputation + AI.                                                                                                                                                                                                    |
| 21  | **Code-signing** (CertificateInfo, SignatureInfo, Cert)                                                                                                                                                                                              | C   | Authenticode signature subject/issuer/validity, certificate parsing. Real for security folks.                                                                                                                                                                                                                               |
| 22  | **VCS columns** (SVNdetails, TcSvn, WDX_GitCommander/libgit2)                                                                                                                                                                                        | A   | **Direct relevance**. Cmdr should show git status (modified/untracked/branch) per file as native columns; `WDX_GitCommander` validates this is a wanted feature.                                                                                                                                                            |
| 23  | **Android APK** (APK-wdx)                                                                                                                                                                                                                            | C   | App name, version, target SDK from `.apk`. Niche; plugin tier.                                                                                                                                                                                                                                                              |
| 24  | **Filename derivation** (Filename ChrCount, Expander2/expander, SplitNameByRegExpr, NameCompare, NicePaths, Translit_wdx)                                                                                                                            | B   | **Useful paradigm**: extract synthetic columns from filename via regex/transliteration/path-length checks. Cmdr's MRT (multi-rename) should accept regex-derived columns. Path-length check is a real Windows pain point.                                                                                                   |
| 25  | **Directory aggregations** (DirSizeCalc, EmptyCheck, FileX, MediaTime aggregation)                                                                                                                                                                   | A   | Recursive size/file count/duration sum/empty-check at folder level. Cmdr's branch view + folder columns should cover this natively.                                                                                                                                                                                         |
| 26  | **Windows shell metadata** (ShellDetails, ShellInfo, Summary, Attributes, Permissions, Security Info, ShareInfo, Volume, IconLibrary, decRecycleBinWDX, Shortcut)                                                                                    | C   | Surfaces all Windows Explorer columns. Cmdr's Windows backend should expose the equivalents (NTFS attributes, ACLs, share status), but as cross-platform-aware columns.                                                                                                                                                     |
| 27  | **Fonts** (AKFontInfo)                                                                                                                                                                                                                               | C   | Family, style, weight, version. Real for designers; plugin tier.                                                                                                                                                                                                                                                            |
| 28  | **File classification by group/regex** (Groups, Group Sort, FileGroups)                                                                                                                                                                              | B   | Strong paradigm: tag-like grouping. Cmdr's AI tagging is the natural successor; the regex-mask version stays useful as a deterministic fallback.                                                                                                                                                                            |
| 29  | **Scripting frameworks** (Script Content Plugin: VBS/JS; WinScript Advanced: VBS/JS/Python/AHK/PHP/AutoIt/PowerShell; super_wdx)                                                                                                                     | B   | **Strong paradigm**: user-defined columns from arbitrary scripts, with up to 21 columns per script. Cmdr should ship a built-in "custom column from JS/Python expression" feature. `super_wdx` (one column that picks the right plugin per file type) is the universal-column paradigm, exactly what an AI column would do. |
| 30  | **Configurable XML/JSON extractors** (anyXML, wdx_xml)                                                                                                                                                                                               | B   | Same idea as #29 but declarative: "pull these XPath/JSONPath fields from this format". Could be a Cmdr "schema-driven column" feature.                                                                                                                                                                                      |
| 31  | **Regex search columns** (PCREsearch, regexp_wdx)                                                                                                                                                                                                    | B   | First-class regex columns with counting / line-numbers / random-string generation / encoding-aware. Cmdr's search should support this as a column type, not just as a search filter.                                                                                                                                        |
| 32  | **String similarity** (Similarity)                                                                                                                                                                                                                   | B   | Distance/similarity to a target string as a column. Useful for dedup/cleanup workflows. AI embeddings are the modern version.                                                                                                                                                                                               |
| 33  | **Find files contained in directory** (FileInDir, EmptyWDX, File)                                                                                                                                                                                    | B   | "Directory contains X" as a per-folder column. Useful pattern; Cmdr's branch view + filters can express this.                                                                                                                                                                                                               |
| 34  | **First/last bytes / hex peek** (kbyte, firstByte, decHexWDX)                                                                                                                                                                                        | F   | Tiny utilities; AI/lister already covers "what's in this thing".                                                                                                                                                                                                                                                            |
| 35  | **Recycle bin metadata** (decRecycleBinWDX)                                                                                                                                                                                                          | C   | Original path / deletion date. Cmdr should show this when browsing trash.                                                                                                                                                                                                                                                   |
| 36  | **BitTorrent metadata** (Torrent_wdx)                                                                                                                                                                                                                | C   | Tracker, file count, hash, total size from `.torrent`. Real but niche; plugin tier.                                                                                                                                                                                                                                         |
| 37  | **HTML/SEO** (SEO HTML)                                                                                                                                                                                                                              | D   | SEO concerns moved on; static HTML SEO via shell column is dated.                                                                                                                                                                                                                                                           |
| 38  | **MHT** (MhtUnPack wdx fields)                                                                                                                                                                                                                       | C   | Covered in packer review.                                                                                                                                                                                                                                                                                                   |
| 39  | **Browser cache** (Opera_Cache)                                                                                                                                                                                                                      | D   | Opera ≤10.10 only. Modern browsers don't expose cache like this.                                                                                                                                                                                                                                                            |
| 40  | **CDR info** (CorelDRAW)                                                                                                                                                                                                                             | C   | Niche, but designers still use Corel. Plugin tier.                                                                                                                                                                                                                                                                          |
| 41  | **Game data** (readGbx, Trackmania)                                                                                                                                                                                                                  | E   | Game-specific.                                                                                                                                                                                                                                                                                                              |
| 42  | **Defunct platform/format columns** (Palm DB File info × 2, GSF Info, MIDlet, Jad Info (J2ME), TypeLib Info, AKMedia DV, AKVegasUsage, Simulink, In4Info, Persian Calendar, ShedkoBadgesTC, URL Grank, SWF Content × 2, CDA File Info, CDA Info New) | D   | All dead/very-niche: Palm, J2ME, Trackmania, Simulink, Sony Vegas, Flash SWF, Google PageRank, etc.                                                                                                                                                                                                                         |
| 43  | **TC-internal / pure utility** (FindZeroFiles, FileTime Delta, Autorun, NameCompare, SkipCompare, wdx_Rename, decTCPluginInfoWDX, wdx_nm2_info, FileScanner, Misc, Directory)                                                                        | F   | Edge utilities; Cmdr core or AI subsumes.                                                                                                                                                                                                                                                                                   |

### Stats

Counted by deduplicated row (plugins grouped within a row count as one). Total deduplicated entries: **~43 rows
representing ~140 individual plugins.**

| Category                                            | Count |    % | Meaning for Cmdr                                                                                                                                                                                                                                                                                                                                  |
| --------------------------------------------------- | ----: | ---: | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **A**: Directly relevant (must-have native columns) |    14 | ~33% | Audio tags, MediaInfo, EXIF/IPTC/XMP, image dimensions, Office/PDF/EPUB metadata, archive metadata (incl. deb/rpm), hashes, link/stream visibility, encoding/BOM/NFC, MIME/magic detection, git status, dir aggregations, full-text search.                                                                                                       |
| **B**: Inspiring paradigm                           |    11 | ~26% | Date-derivative columns, file-comments-as-xattrs, classification-by-rule, Bitchaos-style trust column, scripting-as-column (JS/Python), declarative XML/JSON column extractors, regex columns with counting, string similarity (AI embeddings), filename-regex-derived columns, "directory contains X" columns, super_wdx-style universal column. |
| **C**: Niche-but-real                               |     9 | ~21% | EML metadata, code-signing/cert, EXE/PE info, Windows shell columns, fonts, e-books, APK, recycle-bin, BitTorrent.                                                                                                                                                                                                                                |
| **D**: Ancient/dead                                 |     5 | ~12% | Palm/J2ME/Trackmania/Sony Vegas/Simulink/Flash/PageRank/Opera-cache/CDA/AKMedia DV/CorelDRAW.                                                                                                                                                                                                                                                     |
| **E**: Game-specific                                |     1 |  ~2% | Trackmania.                                                                                                                                                                                                                                                                                                                                       |
| **F**: Utility/redundant                            |     3 |  ~6% | Hex/byte peeks, sync-tool helpers, TC-plugin-meta, name-case checks, attribute mirrors.                                                                                                                                                                                                                                                           |

### Headline takeaways

- **Content plugins are the most AI-aligned of the four plugin types.** A + B = ~59%, by far the highest signal/noise of
  any plugin category we've reviewed. **All of category B is "structured data as a column"**, and that's exactly what an
  AI-native file manager produces by default. Cmdr can deliver most of B without writing per-format plugins, by having
  the AI describe any file into well-typed columns.
- **The native-columns list is short and well-bounded.** Ship rich extractors for: audio tags (TagLib), media
  (MediaInfo), EXIF/IPTC/XMP (Exiv2), image dims (image-rs), Office/PDF/EPUB metadata, archive listings, deb/rpm package
  info, hashes (5–6 algos), magic-byte type detection, git status. That's ~10 libraries and covers the vast majority of
  every "I want this column" request from 25 years of TC users.
- **Three paradigms worth formalizing in Cmdr's column system**:
  1. **Custom column from a JS/Python expression** (the `WinScript Advanced` / `Script Content Plugin` / `super_wdx`
     lineage).
  2. **Declarative XPath / JSONPath columns** (`anyXML` lineage): define columns via config, no code.
  3. **AI-derived column**: natural successor to `super_wdx` and `Bitchaos`: ask the AI for arbitrary semantic columns
     (genre, summary, copyright clarity, similarity to a query, "needs OCR", "looks like spam", etc.).
- **Native git status columns are a clear win.** `WDX_GitCommander` (libgit2) being a stand-alone plugin in TC tells you
  developers want this, and Cmdr is dev-friendly to begin with.
- **Cross-platform-correctness hooks**: `LineBreakInfo`, `EncInfo`, `NFCname`, `UnicodeTest`, `Filename ChrCount` exist
  because users hit real interop pain. Cmdr should bake CRLF/LF, BOM, NFC/NFD, max-path-length warnings into the UI
  rather than burying them in optional columns.
- **`MediaTime` (sum durations recursively) is a quietly powerful idea**: aggregate columns at the directory level.
  Cmdr's branch view should support arbitrary aggregations (sum, max, min, mean) over child columns, not just `size`.
- **About 20% (D + E + F) is dead or trivial**, which is lower than packer (~50%) and FS (~37%); content plugins age
  better because metadata extraction is more universal than format support.

## Lister plugins

Same structure (table → stats → takeaways), grouped aggressively by what's being previewed since lister plugins are
densely redundant within each format family.

---

### Unified table

| #   | Plugin family (deduped)                                                                                                                                                                                                                                                                                                                                          | Cat | Notes                                                                                                                                                                                                           |
| --- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | **Image viewer** (Imagine, ImgView, Imgview 2.0, SGViewer, PhotoViewer, MyViewPad, AKPic, ImageLister, decWinCodecWLX, decFLTViewer, TC IrfanView 1.x/2.x)                                                                                                                                                                                                       | A   | The single most-replicated plugin family. Cmdr's preview must natively show all common formats: BMP/JPG/PNG/GIF/TIFF/WebP/AVIF/HEIC/SVG/EMF/WMF/ICO/PSD/RAW. Use `image`/`libheif`/`webp` Rust crates.          |
| 2   | **Camera RAW** (covered in IrfanView/Imagine/PhotoViewer: ARW, CR2, CR3, CRW, DNG, NEF, ORF, RAF, RW2, etc.)                                                                                                                                                                                                                                                     | A   | Photographers care; Cmdr should ship RAW preview support (libraw via Rust).                                                                                                                                     |
| 3   | **Animations / GIF / APNG / animated WebP** (Imagine, IrfanView, Flic)                                                                                                                                                                                                                                                                                           | A   | Cmdr preview should auto-play; Flic (FLI/FLC) is dead.                                                                                                                                                          |
| 4   | **SVG** (SVGView, SVGwlx, plus Imagine/IrfanView)                                                                                                                                                                                                                                                                                                                | A   | Native SVG render via WebView2/wry.                                                                                                                                                                             |
| 5   | **PDF** (pdfview, PDFView, ActivePDFView, sLister, Sumatra-based, gswlx, TC SumatraPDF)                                                                                                                                                                                                                                                                          | A   | Mainstream; Cmdr should ship native PDF preview (mupdf/pdfium/poppler).                                                                                                                                         |
| 6   | **DjVu** (TC WinDjView, DjvuList)                                                                                                                                                                                                                                                                                                                                | C   | Real for academia/scanned books; plugin-tier.                                                                                                                                                                   |
| 7   | **PostScript / EPS / DVI** (gswlx, DVI Simple Viewer)                                                                                                                                                                                                                                                                                                            | C   | Niche but alive in academia. Plugin tier.                                                                                                                                                                       |
| 8   | **Microsoft Office (legacy + OOXML)** (OfficeView, Office, Excellence, RedDOC, RedCell, RVLister, Office2007wlx_64, MultiLister, ListDoc)                                                                                                                                                                                                                        | A   | Cmdr should preview .doc/.docx/.xls/.xlsx/.ppt/.pptx without requiring Office. Use libraries like `quick-xml` for OOXML, `dotenvy`/openxml4j ports.                                                             |
| 9   | **OpenOffice / LibreOffice** (OpenOffice Simple Viewer, OpenOffice/DOCX/FB2 Viewer)                                                                                                                                                                                                                                                                              | A   | Same family. ODF preview is reasonable.                                                                                                                                                                         |
| 10  | **E-books** (TC AlReaderExt: EPUB/MOBI/FB2/AZW/AZW3/CBR/CBZ/PRC/RTF/DOCX; eBookInfo WLX, wlx_fb2, Fast Fb2 Epub)                                                                                                                                                                                                                                                 | A   | Cmdr should preview EPUB/MOBI natively. CBR/CBZ comic readers covered too.                                                                                                                                      |
| 11  | **Comics CBR/CBZ/CB7/CBT** (TC AlReaderExt, TC SumatraPDF, mthumbs)                                                                                                                                                                                                                                                                                              | C   | Real audience; archive-as-image-stream pattern.                                                                                                                                                                 |
| 12  | **Plain text + syntax highlighting** (CudaLister × 2, hpg-ed, SynUs, SynWrite × 2, GSA Lister, TotalHLT, Code Viewer, Scintilla Lister, SyntaxColorizer, Syn)                                                                                                                                                                                                    | A   | Cmdr's text preview must do syntax highlighting natively. Use `tree-sitter` / `syntect` (the latter ships with Cmdr's stack).                                                                                   |
| 13  | **Markdown rendering** (WLX Markdown Viewer, MarkdownViewer)                                                                                                                                                                                                                                                                                                     | A   | Native, with images and links.                                                                                                                                                                                  |
| 14  | **HTML / web** (HTMLView, IEView × 2, IEWebLister, RedHTML, WebView, URL Shortcut Viewer, **WLX Edge Viewer (Chromium)**)                                                                                                                                                                                                                                        | A   | Cmdr's webview-based preview gives this for free. WLX Edge Viewer (2026) is a modern hint: Chromium-based universal lister.                                                                                     |
| 15  | **XML / JSON tree+grid views** (XML Review, XML Viewer, xmltab, JSON Viewer, jsontab, anyXML)                                                                                                                                                                                                                                                                    | A   | **Strong native pattern**: tree + table-of-objects + filterable. JSON/YAML/TOML/XML should ship with this UX out of the box.                                                                                    |
| 16  | **CSV / TSV** (CSV View, CSV Viewer, csvtab, ODBC-CSV)                                                                                                                                                                                                                                                                                                           | A   | Filterable, sortable table is a must, not just plain text.                                                                                                                                                      |
| 17  | **SQLite** (SQLiteViewer × 2, sqlite-wlx, wLx_SQLLite, unhide-wlx for deleted rows)                                                                                                                                                                                                                                                                              | A   | **Direct hit**: SQLite as table viewer is a proven popular feature. Cmdr should ship this natively. `unhide-wlx` (deleted rows!) is a clever extra.                                                             |
| 18  | **Other DB engines** (DBF: BaseView/DBF-View/DBFViewer/xBaseView; Access: Access DB Viewer/odbc-wlx; Firebird/Interbase: GDBView/Firebird DB Viewer; generic DBLister)                                                                                                                                                                                           | C   | Real but niche. Plugin tier.                                                                                                                                                                                    |
| 19  | **Audio metadata viewer** (Anytag × 2, Audio Tag View, Mmedia, Multimedia factory × 2, decID3WLX, MP3 Tag View, mp3tag)                                                                                                                                                                                                                                          | A   | Already covered by content plugins; preview should show album art + tags.                                                                                                                                       |
| 20  | **Audio playback in preview** (AmpView × 3, TC ModPlug, TC 1by1, TCPlayer, Wise Tracker, Modules Player, Media Show, APlayer, SMViewer)                                                                                                                                                                                                                          | A   | Preview should _play_ audio. Modern tracker formats (MOD/XM/IT/S3M) covered by libopenmpt; niche but cheap.                                                                                                     |
| 21  | **Video playback in preview** (Mmedia × 2, mthumbs, mplayer4tc × 2, TxQuickView, DSView, SMViewer, TC FlashPlayer)                                                                                                                                                                                                                                               | A   | Preview should play video. Use system codec or bundled ffmpeg. Flash players are dead.                                                                                                                          |
| 22  | **Universal media via MediaInfo / IrfanView / SumatraPDF wrap** (Mmedia, mthumbs, sLister, TC IrfanView, TC SumatraPDF, TC AkelPad, TC AlReaderExt)                                                                                                                                                                                                              | B   | **Strong paradigm**: lister plugin = thin wrapper around an external tool. Cmdr should let users register any CLI/GUI tool as a previewer for given extensions.                                                 |
| 23  | **uLister (Oracle Outside-In, 500+ formats) / MultiLister / TxQuickView**                                                                                                                                                                                                                                                                                        | B   | **Strongest paradigm match for Cmdr's AI angle**: one viewer that can render _anything_. AI is the modern replacement; for binary/proprietary formats, fall back to wrapping commercial filters.                |
| 24  | **Mmedia + MediaInfo combo**                                                                                                                                                                                                                                                                                                                                     | A   | Validates the model: native preview + integrated metadata via `mediainfo`.                                                                                                                                      |
| 25  | **Hex viewer / editor** (HexViewer, FileView 2.0)                                                                                                                                                                                                                                                                                                                | A   | Cmdr should ship a native hex view (with edit).                                                                                                                                                                 |
| 26  | **Binary inspection: PE / ELF / Java .class** (PE Viewer × 2, FileInfo, Symview, AnyELF, TC Jad, dirtyJOE, fileinfo)                                                                                                                                                                                                                                             | A   | Cmdr should support PE + ELF + Mach-O preview (sections, imports/exports, symbols, signatures), and decompile Java .class on the fly. AI summary on top is the natural enhancement.                             |
| 27  | **X.509 / certs / signatures** (CertView, ASNView, CertificateInfo, SignatureInfo)                                                                                                                                                                                                                                                                               | A   | Cmdr should ship a cert previewer for `.cer/.pem/.p7b/.p12/.crl/.csr`, using real Rust crates (`x509-parser`, `rustls-pemfile`).                                                                                |
| 28  | **Archive content preview** (ArchView × 2, ArcView)                                                                                                                                                                                                                                                                                                              | A   | Already covered by archive support; preview shows file count/ratio/comment.                                                                                                                                     |
| 29  | **MAT files (MATLAB)**                                                                                                                                                                                                                                                                                                                                           | C   | Plugin tier.                                                                                                                                                                                                    |
| 30  | **FITS (astronomy)** (LookFits)                                                                                                                                                                                                                                                                                                                                  | C   | Plugin tier; real for science.                                                                                                                                                                                  |
| 31  | **CAD: DWG / DXF / HPGL / SVG / CGM** (CAD View × 2, 2D CAD View, ruDWGPreview)                                                                                                                                                                                                                                                                                  | C   | Real, niche. Plugin tier. AutoCAD DWG is alive in industry.                                                                                                                                                     |
| 32  | **3D models** (Interactive OpenGL viewer for 3MF/STL/STEP, 3D File viewer 3DS/LWO/DXF/STL/OBJ/AC/PLY, vendor previews: Solid Edge / Inventor / Revit / SolidWorks / 3ds Max / Rhinoceros)                                                                                                                                                                        | C   | Multiple vendor preview-bitmap extractors, most just show the embedded thumbnail. Cmdr should at least extract embedded thumbnails; STL/3MF live render is a B-grade nice-to-have.                              |
| 33  | **Diagrams: Mermaid + PlantUML + Qt UI** (Mermaid.js Lister, PlantUML Lister, QtUiViewer)                                                                                                                                                                                                                                                                        | A   | **Direct hit for modern devs.** Cmdr should auto-render `.mmd`/`.puml`/`.dot`/`.tex` diagrams in preview. Mermaid via JS in webview is straightforward.                                                         |
| 34  | **GIS / shapefiles** (GisViewer, GisLister)                                                                                                                                                                                                                                                                                                                      | C   | Real for GIS folks; plugin tier.                                                                                                                                                                                |
| 35  | **EML / MSG / Mbox email** (EML Viewer, EMLView, IEView with .eml)                                                                                                                                                                                                                                                                                               | C   | Plugin tier. Cmdr's ".eml feels like a folder" pattern (covered in packer review) plus a viewer pairing.                                                                                                        |
| 36  | **MHT / MAFF** (decMaffWLX)                                                                                                                                                                                                                                                                                                                                      | C   | Self-contained web archives.                                                                                                                                                                                    |
| 37  | **Torrent files** (TCTorrent, TorrentLister, Torrent)                                                                                                                                                                                                                                                                                                            | C   | Tracker/files/piece-size view. Real but niche.                                                                                                                                                                  |
| 38  | **INI / .reg / config files** (IniView, IniEd)                                                                                                                                                                                                                                                                                                                   | A   | Tree-style ini/reg/yaml/toml editor in preview is a good UX.                                                                                                                                                    |
| 39  | **Log tail / large log viewer** (LogViewer, LogTail)                                                                                                                                                                                                                                                                                                             | A   | **Underrated**: live-tail with regex coloring + 5 GB+ support. Cmdr should ship a first-class log viewer.                                                                                                       |
| 40  | **Crash dump / minidump** (DmpView)                                                                                                                                                                                                                                                                                                                              | C   | Plugin tier.                                                                                                                                                                                                    |
| 41  | **NFO / DIZ** (NFOViewer, NFO View, ANSI viewer)                                                                                                                                                                                                                                                                                                                 | C   | Real (scene/release info, code-page-866 ASCII art). Cmdr could just ship a CP437/CP866 toggle in text preview.                                                                                                  |
| 42  | **Fonts** (AKFont, TTFviewer, Font, Multimedia Factory)                                                                                                                                                                                                                                                                                                          | A   | Show glyphs + sample text + metadata. Real value for designers.                                                                                                                                                 |
| 43  | **Icons** (IclView × 2, ICLView, decIcoWDX)                                                                                                                                                                                                                                                                                                                      | C   | Niche-real (Windows).                                                                                                                                                                                           |
| 44  | **EBCDIC** (EBCDICview)                                                                                                                                                                                                                                                                                                                                          | C   | Mainframe interop. Plugin tier.                                                                                                                                                                                 |
| 45  | **Source-code analysis (chars/encoding)** (CharsOccurrences, EncInfo)                                                                                                                                                                                                                                                                                            | F   | Edge utility; better as on-demand AI query.                                                                                                                                                                     |
| 46  | **Calendar appliance** (tcCalendar + edt-pack)                                                                                                                                                                                                                                                                                                                   | F   | Doesn't belong in a file manager.                                                                                                                                                                               |
| 47  | **Sudoku game** (XSudoku)                                                                                                                                                                                                                                                                                                                                        | F   | …                                                                                                                                                                                                               |
| 48  | **Boot screen / Putty / WhoOpenDoc / Multimedia Factory Preview / decAdobeSaveWDX / Aml Pages / Mathematica Link / Origin / CDR** (defunct or niche-niche)                                                                                                                                                                                                       | D/C | Mostly dead-platform or one-vendor-only previewers.                                                                                                                                                             |
| 49  | **Game/retro file viewers** (Modelviewer Half-Life, MD2wlx Quake2, inAlasm/inSCR ZX Spectrum, listtap GDR, DirCBMLister Commodore, JccView, scrlist .scr screensavers, WSZView Winamp 2.x skins, MXP Macromedia, Multimedia Factory Preview, Boot Screen View, Flash SWF × 6, MD2wlx Quake2, Modelviewer Half-Life, decThumbsDBViewer, GSF Gedemin, DDS_DD 1Cv7) | D/E | All retro-platform / dead-software / game-specific.                                                                                                                                                             |
| 50  | **Generic viewer wrappers** (AnyCmd: pipe any command's stdout to lister; AppLoader: open with associated app from quickview; Nothing: show nothing; Script plugin-maker: write plugins in JS/VBS; tLister: tabs in lister; WDXGuideInLister / WDXGuideInLister64: embed dev tool)                                                                               | B   | **Important paradigm cluster**: lister-as-pipeline. Cmdr's preview should accept "preview = output of `<cmd>`" as a built-in option, plus tabs in preview, plus a debug "show all metadata for this file" mode. |

### Stats

Counted by deduplicated row (plugins grouped within a row count as one). Total deduplicated entries: **~50 rows
representing ~210 individual plugins**, by far the largest plugin universe of the four types.

| Category                                             | Count |    % | Meaning for Cmdr                                                                                                                                                                                                                                                                                |
| ---------------------------------------------------- | ----: | ---: | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **A**: Directly relevant (must-have native previews) |    23 | ~46% | Image / RAW / animated / SVG / PDF / Office / OpenOffice / e-books / text-with-syntax / Markdown / HTML / XML+JSON tree-grid / CSV / SQLite / audio metadata + playback / video playback / hex / PE+ELF+Java binary / X.509 cert / archives / Mermaid+PlantUML / log tail / INI-config / fonts. |
| **B**: Inspiring paradigm                            |     4 |  ~8% | Wrap-external-tool-as-previewer (`uLister`, `mthumbs`, `Mmedia`); universal-viewer-via-AI; lister-as-pipeline (`AnyCmd`, `Script plugin-maker`); tabs + debug mode in preview (`tLister`, `WDXGuideInLister`).                                                                                  |
| **C**: Niche-but-real                                |    12 | ~24% | DjVu, PS/EPS/DVI, comics CBR/CBZ, MAT, FITS, CAD (DWG/DXF), 3D models / vendor previews, GIS / shapefiles, EML/MSG, MHT, torrent, ASN.1, MAFF, fonts, icons, EBCDIC, NFO/ANSI-art, dump files. Plugin tier.                                                                                     |
| **D**: Ancient/dead                                  |     3 |  ~6% | Flash SWF × 6 plugins, Win9x/XP boot screens, Winamp 2.x skins, Aml Pages, 1C-v7, Mathematica MX, Origin OPJ, decThumbsDB, FLIC.                                                                                                                                                                |
| **E**: Game/retro                                    |     5 | ~10% | Half-Life MDL, Quake2 MD2, ZX Spectrum (Alasm, SCR, listtap), Commodore CBM (DirCBMLister), MAFF/Webshots, scrlist, JCC crossword, MD2/MDL.                                                                                                                                                     |
| **F**: Utility/redundant                             |     3 |  ~6% | tcCalendar appliance, XSudoku game, AppLoader, Nothing, char-count analytics.                                                                                                                                                                                                                   |

### Headline takeaways

- **Lister plugins have the highest A-rate of any plugin type** (~46%). Preview is the most universally needed
  file-manager feature, and the long tail of "I want to see what's inside _this_ without opening a heavy app" is where
  TC's plugin model has been most loved over 25 years.
- **The native-preview list is the longest, but well-bounded.** ~20 format families cover the vast majority of user
  needs: text-with-syntax, markdown, HTML, image (incl. RAW/SVG/animated), PDF, Office
  (.doc/.docx/.xls/.xlsx/.ppt/.pptx), OpenOffice, e-books, audio-with-playback, video-with-playback, hex,
  PE/ELF/Mach-O/.class, X.509 cert, archive listing, Mermaid/PlantUML, log tail with regex highlight, JSON/XML/YAML/TOML
  tree, CSV/SQLite as table, INI/.reg.
- **The single biggest paradigm to formalize: "preview = arbitrary command output"** (`AnyCmd`, `MultiLister`,
  `mthumbs`, `uLister`, `TC IrfanView`, `TC SumatraPDF`). Cmdr's preview pipeline should allow the user to register
  `extension → cli command → render output as text/html/image/pdf` in one config. Combined with AI: "if no rule matches,
  ask AI to summarize."
- **Two modern entries (May 2026 and Sep 2025) point in the right direction**:
  `Interactive OpenGL viewer for .3mf/.stl/.step` and `Mermaid.js Lister`: devs and makers want diagrams and 3D objects
  rendered live.
- **`WLX Edge Viewer` (Feb 2026) is the most modern entry overall**: a Chromium-based universal lister. Cmdr's
  WebView2/wry preview surface natively gives you this: every web-based preview format (HTML, MD, PDF.js, Mermaid,
  Three.js for 3D models, monaco-editor for code, etc.) becomes free.
- **`SQLite Viewer` (143k downloads as a single TC plugin) is the most-downloaded modern lister**, validating
  SQLite-as-table as a clear native feature for Cmdr.
- **Long-tail (D + E + F) is ~22%** (middle-of-the-pack). Lister plugins age better than packer/FS but worse than
  content plugins, because formats die (Flash SWF dragged the curve down).
- **One under-discussed gem worth lifting: `LogViewer`**: 5GB+ tail with conditional coloring. Cmdr is dev-focused; a
  great log viewer is a quiet differentiator.
- **`tLister` (tabs in lister) is a small but durable hint**: Cmdr's preview should support tabbed/multi-pane preview so
  users don't keep losing the previous file when they peek at the next.

## Analysis

Read AGENTS.md, architecture.md (skimmed for the Volume model), and the analysis in full. Opinions below, direct.

### Overlap across plugin types, and which abstraction owns what

There's massive overlap, and TC's four-bucket split is mostly historical accident, not principle. Same job ends up
implemented as a packer, an FS plugin, AND a lister:

| Job                                 | Where TC put it                                                               | Where it belongs in Cmdr                                                                        |
| ----------------------------------- | ----------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| ISO / disk image                    | Packer (TotalISO) + FS (Virtual Disk)                                         | **Volume**. One backend, optional "extract here" verb.                                          |
| .git / GitHub / branches            | FS (GitHubFS, gitview) + Content (git status column)                          | **Volume + Content**: exactly what you already do.                                              |
| MHT / EML / Outlook PST             | Packer (MhtUnPack, TotalObserver) + Lister (EMLView) + Content (EML metadata) | **Lister primary, Content for columns, Volume only for power-user "give me the parts"**.        |
| Office docs (.docx etc.)            | Packer (Storage, WordArc) + Lister (OfficeView) + Content (CDocProp)          | **Lister + Content**. Don't ship "open .docx as folder": wrong primary affordance.              |
| SQLite                              | Lister (SQLiteViewer, the most-downloaded modern plugin)                      | **Lister primary** (table grid). Volume secondary, opt-in.                                      |
| AI conversation exports             | Packer (gemini.wcx)                                                           | **Lister + Volume**. Render the convo nicely; let users browse messages as files. Not a packer. |
| Hashes                              | Packer (Checksum) + Content (LotsOfHashes)                                    | **Content column + native action**. Never a packer.                                             |
| Process list / clipboard / env vars | FS (PROC, FSClipboard, EnvVars)                                               | **Volume**, but flag them as "system surfaces" so they don't pollute the cloud-y Volume picker. |
| NTFS ADS / xattrs / resource forks  | FS (NTFSFileStreams) + Content (NTFS Stream column)                           | **Both**: column for visibility, child-entry expansion when the user drills in.                 |
| Archives (zip/7z/tar)               | Packer + Content (RarInfo) + Lister (ArchView)                                | **All three roles, one backend**. Native, not plugin.                                           |

**Correct rule of thumb:**

- **Live/stateful/remote/has-a-root** → Volume.
- **Render one file better than raw bytes** → Lister/preview.
- **Derive facts about a file (sortable, filterable)** → Content column.
- **One-shot pack/unpack as a verb** → Action/transform, not a "thing you open."

**Wrong directions in TC to NOT inherit:**

- "Packer-as-generator" (MakeBAT, KillCopy, Mover, Wipe). Those are actions. You need a real **Action** extension point
  TC didn't have one and the community shoehorned actions into packers.
- "Lister-as-command-runner" (AnyCmd). Useful, but it conflates two things: separate "register CLI as previewer" from
  "register CLI as action."
- Packer being the dumping ground for "browse compound thing as folder" (Storage, WordArc, gemini.wcx). It only landed
  there because TC's FS plugins are heavy to write. If your plugin SDK makes Volumes cheap, this whole category
  collapses into Volumes.

### Other patterns and most important takeaways

1. **Three "universal" patterns recur across all four plugin types**: wrap-external-CLI (Multi-Arc / AnyCmd / ScriptWFX
   / super_wdx), scriptable-in-TS/JS/Python (Script Content / ScriptWFX / Script plugin-maker), and declarative-config
   (anyXML / MultiArc INI). **Don't reimplement these per plugin type.** Have one "register a command/script/config"
   mechanism the four roles share.

2. **Modernity gradient is loud.** Lister 46% A, content 33% A, FS 16% A, packer 12% A. Listers and content age best
   because metadata and rendering survive format death. Packer ages worst because compression fads and dead platforms
   dominate. Investment priority should match: **content & lister > FS > packer** (which mostly collapses to "native
   zip/7z/tar/zstd + a few first-party plugins").

3. **The newest plugins are all dev-centric**: GitHubFS, gitview, gemini.wcx, Mermaid Lister, WLX Edge Viewer
   (Chromium), 3MF/STL/STEP viewer. **That's literally your audience.** Skating to where the puck went would mean:
   native Git everything, native Mermaid/PlantUML, native 3D model preview, native AI-conversation rendering. Most of
   these need no plugin API at all; they're features.

4. **AI quietly subsumes ~all of Content B-tier** (regex columns, similarity, classification, scriptable columns,
   super*wdx). Cmdr's content extension system can be smaller than TC's \_because* AI is the universal backstop. The
   native columns are well-bounded (~10: TagLib, MediaInfo, Exiv2, image-rs, Office/PDF metadata, archive listing,
   deb/rpm, hashes, magic-bytes, git status). Beyond that, lean on AI before opening a plugin slot.

5. **Aggregation as a first-class column property** (MediaTime summing durations, DirSizeCalc, Branch View Extended).
   Cmdr's branch view should support sum/min/max/mean/count rollups on any column, both for built-in and user-defined
   columns. TC bolted this on per-plugin; you can design it in.

6. **"Tags as virtual folders"** (DocClassifier, MP3Commander, M3U/M3U8) appears across packer AND FS: strong signal
   that virtual/saved-query collections are wanted. Cmdr's AI angle makes this trivial; design the Volume contract to
   admit "virtual collection backed by a query," not just "directory tree backed by a real path."

### What to ask/look at before jumping on API shapes

The big ones I'd want answered first; these shape the API more than format support does:

1. **Process isolation & crash containment.** TC plugins are in-process DLLs; one bad plugin tanks the app. What's
   Cmdr's stance? WASM (sandboxed, cross-platform, slower)? Subprocess + JSON-RPC (Tauri-native, easy)? Native dylib
   (fast, dangerous)? Pick before you design types, as it dictates the ABI.
2. **MCP overlap.** You already expose Cmdr to agents via MCP (port 9224). MCP is essentially "external tool as a stdio
   JSON-RPC plugin." Should Cmdr's plugin API just _be_ MCP-shaped, so the same plugin serves humans and AI agents? This
   could collapse two systems into one. Worth a hard look before committing to a bespoke contract.
3. **Trust/permissions model.** TC has none. Cmdr is going to ship to non-developers. Per-plugin capability declarations
   (filesystem scope, network scope, exec scope), Tauri-style consent, signed manifests, and a registry: these need to
   be in v1 of the API, not bolted on. Look at VS Code, Raycast, Obsidian extension permissions.
4. **Async + cancellation.** TC's plugin API is blocking and that's caused 20+ years of UI freezes. Your `Volume` trait
   is async; the plugin trait must be too, with explicit cancellation and progress reporting.
5. **Language / SDK target.** What does the developer write in? Rust (matches core, excludes 90% of plugin authors)? TS
   (matches your front-end stack)? Both via WASM (any language compiles)? B-tier in FS and Content (`ScriptWFX`,
   `Script Content Plugin`, `Perl FS`, `super_wdx`) shows scriptable-in-a-friendly-language is what plugin authors
   actually want.
6. **Are there really four plugin types, or one capability manifest?** The real axes are: read vs r/w, file-bound vs
   root-bound, stateful vs one-shot, renders vs lists vs derives vs transforms. Consider one manifest with capability
   flags rather than four types: fewer concepts, no overlap confusion (which TC suffered from). Lots of plugins would
   naturally declare 2–3 capabilities at once (e.g. SQLite plugin = lister + volume + content).
7. **Action/transform extension point.** TC missed this; devs jammed actions into packers. Cmdr should have a
   first-class "add a verb to the file manager" slot (right-click items, command palette entries, multi-file batch
   actions). Decide its shape _now_ so it doesn't get retrofitted.
8. **Aggregation API.** Per (5) above: if you commit to native aggregation columns, plugins need to be able to declare
   "I'm aggregable, use sum" or "I'm aggregable, use AI-summary." Worth thinking about up front.
9. **Stability/versioning commitment.** TC's API has been stable for decades: that's a feature plugin authors love. Will
   Cmdr commit? SemVer + capability negotiation is the modern answer.
10. **Compatibility with existing TC plugins.** Probably "no, but here's the porting pattern." Worth deciding explicitly
    because the long tail (5,000+ TC plugins) is real even if mostly category C/D.

If I had to pick the **two** most consequential decisions to make before drawing up types: **(a) MCP-shape vs bespoke**,
and **(b) one capability manifest vs four plugin types**. Both are easy to get wrong and very expensive to change later.

## Unpacking each of these 10 points

### 1. Process isolation: recommendation

**Recommendation: hybrid. Subprocess + JSON-RPC as primary, WASM as fast-lane, no in-process native ever.**

| Model                            | Speed (per call)    | Crash safety           | Sandboxable                                                     | Lang support                                               | Dev ergonomics                 |
| -------------------------------- | ------------------- | ---------------------- | --------------------------------------------------------------- | ---------------------------------------------------------- | ------------------------------ |
| In-process native dylib          | ~ns (function call) | ❌ takes down host     | OS-level only                                                   | Rust/C++/C                                                 | Hard ABI, hard signing         |
| Subprocess + stdio JSON-RPC      | ~50–200μs           | ✅ process boundary    | ✅ macOS sandbox-exec, Win AppContainer, Linux landlock/seccomp | Anything (Node, Python, Rust, Go)                          | Easy, mature pattern (LSP/MCP) |
| WASM (Wasmtime in-host)          | ~1–10μs             | ✅ memory-bounded trap | ✅ capability-based via WASI                                    | Rust, AssemblyScript, Go (tinygo), C/C++; Python improving | Medium, tooling young          |
| Embedded JS engine (QuickJS/boa) | ~μs                 | ✅ bounded, no native  | ✅ no native imports                                            | JS only                                                    | Easy for JS, locked-in         |

What the successful precedents do:

- **VS Code**: Node subprocess "extension host"; every extension is JS in one shared subprocess. Crashes contained, but
  extensions in one host can interfere.
- **Obsidian**: in-process JS, fast, but plugins can scribble anywhere. Trust model = "we curate."
- **Raycast**: subprocesses + a heavy review process for the store.
- **Browsers**: WASM + JS, sandboxed by default, capability-prompted.

**For Cmdr's "easy + safe" goal:**

- **Default tier: Subprocess + JSON-RPC**. Spawned per plugin. Killed cleanly on crash. Sandboxed via OS primitives
  derived from manifest scopes. Any language. The 100μs/call overhead is invisible for 99% of plugin work because real
  plugin calls (preview a PDF, list a directory, fetch metadata) are batched, not per-row.
- **Fast tier: WASM**. For pure-Rust performance plugins (a custom hash column called per-row in a 100k folder, an
  in-memory FS that's hit constantly). Same wire shape, different transport. Plugin manifest declares which transport.
- **Never** in-process dylibs. The dev convenience win is small and the security/stability loss is enormous. TC's pain
  teaches this.

The OS sandboxing piece is the underrated half: subprocess alone isn't safe; a runaway plugin can still read `~/.ssh`.
You need to spawn the subprocess inside a sandbox profile derived from its declared scopes. macOS `sandbox-exec`, Linux
`landlock` + `seccomp`, Windows `AppContainer`. That's where "safe by design" actually comes from.

### 2. MCP overlap: overhead reality check

**Honest numbers** (modern Mac/Linux, local pipes, small payloads):

| Operation                                   | Overhead                     |
| ------------------------------------------- | ---------------------------- |
| stdio JSON-RPC roundtrip (single small msg) | ~50–200μs                    |
| Native syscall (`readdir`, `stat`)          | ~1–10μs                      |
| Function call into WASM                     | ~1μs (just-in-time compiled) |
| Native function call (in-process)           | ~1ns                         |

So MCP-over-stdio is **10–100× slower** than a syscall _per call_. In your "in-memory FS that's hit constantly" example:
if Cmdr calls the plugin 100,000 times sequentially for a directory walk, you eat 5–20 seconds in pure IPC overhead.
That's catastrophic.

But that's avoidable two ways:

1. **Batch / stream the API.** Don't expose `getEntry(path)`; expose `listDir(path, opts) -> stream<Entry>`. Plugin
   returns 100k entries in one streamed response; overhead amortizes to once. This is how LSP works (it doesn't ask "is
   this token a keyword?" per char; it sends `textDocument/semanticTokens` once).
2. **Multiple transports for the same MCP-shape contract.** Same JSON-RPC schema, different wire:
   - stdio JSON-RPC for community/sideloaded plugins
   - WASM in-host for signed/Rust plugins (microsecond overhead)
   - Shared-memory for blessed first-party plugins (negligible overhead)

   The plugin manifest declares which transport it needs and which it supports. Cmdr picks the fastest available given
   trust level.

So the answer is: **yes, MCP-shape for the contract; no, not stdio for everything.** Use the protocol's shape, vary the
transport. You get "humans and AI agents both call plugins through the same surface" without paying stdio overhead for
things that need to be fast.

(Side note: this also means your existing MCP server on port 9224 and your future plugin host can share a lot of code;
the plugin host is "MCP, but inbound, with sandboxing.")

### 3. Permissions: what good looks like

**Best-of-breed pattern (synthesizing browser extensions, OAuth scopes, macOS TCC, VS Code, iOS):**

A. **Manifest declares scopes** (default deny):

```toml
[plugin]
id = "com.acme.exif-columns"
name = "EXIF Columns"

[capabilities]
provides = ["column"]

[scopes]
fs.read = ["selection"]              # only files Cmdr passes me
fs.write = []                         # nothing
net.fetch = ["api.exiftool.org"]      # specific host(s) only
process.exec = []
clipboard.read = false
```

Scopes are **path-qualified, host-qualified, and verb-qualified**, never just "filesystem" or "network."

B. **Install-time consent** (browser-extension style). User sees:

> EXIF Columns wants:
>
> - Read files you select
> - Connect to api.exiftool.org

User accepts once. Installed.

C. **OS-level enforcement as defense-in-depth.** When Cmdr launches the plugin subprocess, it generates a sandbox
profile from the manifest:

- macOS: `sandbox-exec` profile, `(allow file-read* (subpath "..."))`
- Linux: landlock ruleset + seccomp filter
- Windows: AppContainer with restricted token

If the plugin lies about its scopes, the OS denies the syscall. Belt + braces.

D. **Three trust tiers** for plugin distribution:

- **Sandboxed/sideloaded**: limited scope set; no `process.exec`, no `fs.write` outside selection, no broad `fs.read`.
- **Signed (community store)**: passes basic review (no obvious malware), can request more scopes; user still consents.
- **First-party**: bundled with Cmdr, full access.

E. **Audit log**: persistent local log of plugin calls (or at least: scope grants, exec invocations, network hosts hit).
Users can `Settings → Plugins → [plugin] → Activity` and revoke.

F. **Runtime escalation prompts** (sparingly) for genuinely sensitive things like "first time reading `~/.ssh`" or
"wants to read full disk." VS Code/browsers don't do this and it's a real gap.

The consent UX matters as much as the model. Cmdr can be the thing that finally does this _well_; most apps' permission
UIs are awful. Lean on `manifest -> human-readable summary` translation; don't dump JSON at users.

### 4. Async: agreed, moving on.

### 5. Rust + TS dual SDK: patterns that work

**Yes, this is well-trodden ground.** The pattern is:

> Define the plugin contract once in a schema. Generate two SDKs (Rust + TS). Both speak the same wire protocol.

**Reference points (positive examples):**

| Project                            | How they do it                                                                                                                     |
| ---------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| **LSP** (Language Server Protocol) | JSON-RPC contract published by Microsoft. Clients in TS, Java, Rust, Go. Servers in any language. The protocol is the API.         |
| **Tauri itself**                   | Rust core + TS frontend, share schema via `tauri-specta` (auto-generates TS types from Rust traits). You already use this for IPC. |
| **Rspack / Turbopack**             | Rust core, JS plugin API via N-API + WASM. Two-tier perf.                                                                          |
| **napi-rs / PyO3**                 | Rust libs exposed as Node/Python packages. Unified codebase.                                                                       |
| **gRPC + protobuf / smithy**       | Define service in `.proto`, generate clients in 10+ languages. Industry standard.                                                  |

**Concrete shape for Cmdr:**

1. **Single source of truth**: a Rust trait + types annotated with `tauri-specta`-style codegen attributes. (Or a
   `.smithy`/`.proto` if you prefer a separate schema language; I'd start with the Rust trait, less ceremony.)
2. **`cmdr-plugin` Rust crate**: Devs `impl Plugin for MyPlugin`, get a binary they ship. Builds to either:
   - Native subprocess (fast dev, easy debug)
   - WASM module (sandboxed fast lane) Same source, two targets via Cargo features.
3. **`@cmdr/plugin` npm package**: Devs `export default class MyPlugin implements Plugin {}`. Runs as Node subprocess.
   TS types are auto-generated from the Rust trait, so they can never drift.
4. **CLI: `cmdr plugin new --rust` or `--ts`** scaffolds a working hello-world in 10 seconds. Includes manifest, build
   config, test harness, hot reload.
5. **Test harness**: `cmdr plugin test` spawns the plugin, hits it with mock requests, validates the contract. Same
   harness works for both languages.

**Why this works**: the _contract_ is the API; the _language_ is just an implementation choice. Plugin authors don't
have to care that there's a Rust core. TS plugin authors get the same semantics as Rust plugin authors. New languages
(Python, Go) are added later by writing a third SDK against the same wire protocol.

**Watch-outs**:

- Don't let Rust idioms leak into TS API (no `Result<T, E>`-shaped types in TS; make them throw or use a
  `{ ok, value, error }` discriminated union).
- Don't let TS idioms leak into Rust API (no untyped `any`, no callback-style; use proper traits).
- Generate, don't hand-write the bridge types.

### 6. Plugin types: my proposal

**Four orthogonal capabilities, one manifest, a plugin opts into one or more.** Not one mega-type (too vague, no UX
guardrails per role); not TC's four hard types (forces overlap into wrong buckets).

| Capability    | What it provides                                         | Plugs into                              |
| ------------- | -------------------------------------------------------- | --------------------------------------- |
| **Volume**    | A virtual root with files/folders, read/write, watching  | File explorer pane                      |
| **Previewer** | Render one file's contents                               | Preview pane / file viewer              |
| **Column**    | Derive a sortable/filterable value from a file or folder | File list columns                       |
| **Action**    | A verb that operates on a selection                      | Context menu, command palette, keybinds |

A plugin's manifest declares which capabilities it provides:

```toml
[plugin]
id = "com.acme.sqlite"

[capabilities]
provides = ["previewer", "volume", "column"]

[capabilities.previewer]
extensions = ["db", "sqlite", "sqlite3"]
mimeTypes = ["application/x-sqlite3"]

[capabilities.volume]
schemes = ["sqlite"]                     # opens sqlite://path/to/file.db
opensFromFile = ["db", "sqlite"]         # also: "open this .db as a folder"

[capabilities.column]
columns = ["sqlite.tableCount", "sqlite.size"]
```

This collapses TC's confusion (SQLite is "all three of those things at once") while keeping per-capability **UX
contracts**:

- **Previewer** must declare max render time, must respond to cancellation, must support "loading/error/ready" states,
  must respect Cmdr's theme tokens.
- **Volume** must implement async streaming `list_dir`, must declare capabilities (read-only? watch? rename?), must be
  cancellable.
- **Column** must declare type (string/number/date/seconds), aggregation (sum/min/max/mean/none), display formatter,
  sort key. Computed lazily, cached.
- **Action** must declare scope (file types, multi/single, min count), inputs (prompt schema), and stream progress
  events.

**Where the UX guardrails come from:**

- The SDK enforces what it can (must return progress, must respond to cancel within N ms, etc).
- The store review (for signed plugins) catches the rest (must respect light/dark mode, must handle errors gracefully,
  must localize).
- A `cmdr plugin lint` command flags violations during dev.

So: **empower with a small, sharp toolkit, guardrail with the SDK + store + lint**, not by making the API anemic. The
API is rich; the guardrails ride on top.

### 7. Action examples and shape

**Realistic examples** (drawn partly from TC's misclassified packers, partly from modern file managers):

- "Compress with zstd, level 19"
- "Convert PNG → WebP, side-by-side"
- "Strip EXIF from photos"
- "Generate `shasum.txt` for this folder"
- "Open in iTerm / VS Code / Cursor"
- "Send to Slack channel"
- "Sign with GPG key"
- "Move to `~/_archive/$YYYY-MM`"
- "Make torrent from folder"
- "Mirror to S3 bucket"
- "Tag with Finder color label"
- "Run `pre-commit` hooks on staged files"
- "Diff two files with my preferred diff tool"
- "Set folder modtime from contents" (TC's `SetFolderDate`, but as an action)

**Shape:**

```typescript
{
  id: "convert.png-to-webp",
  title: "Convert PNG to WebP",
  description: "Convert selected PNGs to WebP next to the originals",

  appliesTo: {
    select: "files",                 // "files" | "folders" | "any"
    multi: true,
    minCount: 1,
    fileTypes: ["image/png"],        // optional filter; greys out otherwise
    requiresWritable: true
  },

  surfaces: {
    contextMenu: { group: "transform", order: 100 },
    commandPalette: true,
    keybind: null                     // user-customizable
  },

  prompts: [                          // optional input dialog
    { id: "quality", type: "number", label: "Quality", default: 80, min: 1, max: 100 },
    { id: "deleteOriginals", type: "boolean", label: "Delete originals", default: false }
  ],

  capabilities: ["fs:read:selection", "fs:write:selection-dir"],

  async run(ctx: ActionContext): AsyncIterable<ProgressEvent> {
    for (const file of ctx.selection) {
      yield { type: "progress", current: i, total: ctx.selection.length, label: file.name };
      // do work
      yield { type: "log", level: "info", message: `Wrote ${out}` };
    }
    yield { type: "done", summary: "Converted 12 files" };
  }
}
```

Key shape decisions:

- **Streamed progress** so cancellation works at any point.
- **Declared scope** so Cmdr can grey out the menu item when no PNG is selected (no broken-looking items).
- **Prompts as a schema**, not freeform UI: Cmdr renders them, so they get the design system for free.
- **`run` returns an async iterable of events** rather than a promise: natural fit with Rust async streams and JS async
  iterators.

### 8. Aggregation examples and shape

**Aggregation = how a column rolls up from children to a parent (folder/branch view).**

**Examples:**

| Column                       | Aggregation                                         |
| ---------------------------- | --------------------------------------------------- |
| `size`                       | `sum` of descendants                                |
| `count`                      | `count` of descendants                              |
| `duration` (MediaTime)       | `sum` of seconds                                    |
| `modified` newest            | `max`                                               |
| `created` oldest             | `min`                                               |
| `git status`                 | "any-modified", "any-conflict" → max severity       |
| `error count`                | `sum`                                               |
| `ai.summary`                 | LLM aggregates child summaries (custom)             |
| `image.megapixels`           | `mean` (avg), or `sum` for "total pixels in folder" |
| `language` (e.g. cloc-style) | "histogram" → top 1                                 |

**Shape:**

```typescript
{
  id: "media.duration",
  title: "Duration",
  type: "duration",                    // typed; Cmdr knows how to sort/format

  extract: async (file, ctx) => {
    return await readMediaDuration(file.path); // Option<f64> seconds
  },

  aggregate: {
    strategy: "sum",                   // built-in: sum | min | max | mean | count | any | all | none | "histogram-top"
    // OR a custom function:
    // strategy: "custom",
    // fn: (childValues) => ...
  },

  format: {
    display: (v) => formatDuration(v), // "1h 23m"
    sort: (v) => v,                    // sort by raw seconds
    align: "right"
  },

  invalidation: {
    on: ["file.modified", "child.added", "child.removed"]
  }
}
```

**Engine responsibilities** (so plugin authors don't reimplement):

- Cache aggregates per directory keyed by `(plugin, column, dir-mtime, child-set-hash)`.
- Invalidate on watched events.
- Compute lazily (only when column visible).
- Stream partial results ("3/100 done, current sum: 4.2GB...") for big folders.

**Why a small fixed set of strategies + escape hatch**: 95% of aggregations are sum/min/max/mean/count, and the engine
can heavily optimize those (incremental updates, no re-reads). The escape hatch handles the 5% (AI summary, custom
logic). Same shape as SQL window functions or Pandas `.agg()`, proven to scale.

### 9. Iteration phase: agreed, ship small, break freely.

One thing worth pre-committing even before stability: **manifest schema versioning** (`schemaVersion: 1`). Even if you
break the contract weekly, every plugin file declares which version it expects, and Cmdr refuses to load mismatches.
That way the day you decide to stabilize, you don't have to invent a way to identify "old" manifests.

### 10. TC-plugin compatibility: agreed, skip.

Two notes for the record:

- TC plugins are Win32 DLLs with Pascal-derived calling conventions, blocking single-threaded APIs, and assume
  in-process trust. Even on a hypothetical Cmdr-Windows, wrapping them would mean shipping a 32-bit-aware loader,
  dealing with thread-affinity bugs, and accepting that any plugin can crash Cmdr. Not worth it.
- However, the **format coverage** of long-tail TC plugins (Korean ALZ, FATImage, FB2, dar, ZPAQ, etc.) is real and
  useful. The right way to harvest that is "Cmdr plugin authors port the format support"; most of these are thin
  wrappers around an OSS library that already has Rust/TS bindings. That's a community-incentive question, not a compat
  question.

### Suggesting for the next concrete step

Before drawing types: **build a "skateboard"**: one full vertical slice of the simplest plugin type (Column),
end-to-end, in one language (Rust or TS)\*\*, with: manifest, scopes, sandboxed subprocess, JSON-RPC contract, hot
reload, install/consent UI, audit log entry. ~1–2 weeks of work.

That'll surface 80% of the design problems you'll otherwise discover after shipping the API. Specifically:

- How painful is the manifest-vs-runtime-vs-store consent dance in practice?
- What's the actual subprocess startup time on macOS, and does it bother users?
- Does your generated TS-from-Rust binding feel native or weird?
- What does `cmdr plugin new` need to do to be delightful?

Then expand to Previewer, then Volume, then Action, in that order, because Column is the simplest and Volume is the
hardest (state, cancellation, watching, write semantics). By the time you get to Volume, you'll know what's wrong with
your earlier shapes and can fix them while breaking changes are still cheap.
