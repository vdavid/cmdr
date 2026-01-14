To regenerate the icon, use:

```bash
cd apps/desktop && pnpm tauri icon ../../_ignored/new-app-icon.png
```

It puts them to `apps/desktop/src-tauri/icons`.

To see the new icon in the desktop app, restart the dev server (`pnpm dev`).

To update the website favicons:

```bash
cp apps/desktop/src-tauri/icons/64x64.png apps/website/public/favicon.png
cp apps/desktop/src-tauri/icons/icon.ico apps/website/public/favicon.ico
magick _ignored/new-app-icon.png -resize 180x180 apps/website/public/apple-touch-icon.png
```