#!/bin/sh
# Create files and directories with unicode names. Also creates directories for
# the unicode-named shares defined in smb.conf (公開, café, 文档) so clients can
# exercise UTF-8 share-name enumeration too.
for dir in /shares/kokai /shares/cafe /shares/wendang; do
    mkdir -p "$dir"
    printf "content inside unicode-named share\n" > "$dir/README.txt"
done
chmod -R 777 /shares/kokai /shares/cafe /shares/wendang

BASE="/shares/public"
mkdir -p "$BASE"

# CJK characters
printf "Japanese test content\n" > "$BASE/日本語テスト.txt"
printf "Chinese test content\n" > "$BASE/中文测试.txt"

# Emoji directory and file
mkdir -p "$BASE/📁 folder"
printf "File inside emoji folder\n" > "$BASE/📁 folder/📄 document.txt"

# Accented characters
printf "French cafe content\n" > "$BASE/café.txt"
printf "German umlaut content\n" > "$BASE/Ärger.txt"
printf "Spanish content\n" > "$BASE/señor.txt"

# Cyrillic
printf "Russian document\n" > "$BASE/документ.txt"
printf "Ukrainian text\n" > "$BASE/привіт.txt"

# Mixed script directory
mkdir -p "$BASE/données"
printf "Mixed content\n" > "$BASE/données/résumé.txt"

# Arabic
printf "Arabic text\n" > "$BASE/مستند.txt"

chmod -R 777 "$BASE"
