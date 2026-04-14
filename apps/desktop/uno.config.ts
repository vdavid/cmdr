import { defineConfig } from 'unocss'
import presetIcons from '@unocss/preset-icons'

export default defineConfig({
  // Explicit file list for performance: UnoCSS only rescans these files on change
  // instead of watching the entire src/ tree. Update this list when adding UnoCSS
  // classes (i-lucide:* icons) to new files.
  content: {
    filesystem: [
      'src/lib/file-explorer/pane/ErrorPane.svelte',
      'src/lib/file-explorer/selection/SelectionInfo.svelte',
      'src/lib/file-explorer/views/FullList.svelte',
    ],
  },
  presets: [
    presetIcons({
      extraProperties: {
        display: 'inline-block',
        'vertical-align': 'middle',
        'flex-shrink': '0',
      },
    }),
  ],
})
