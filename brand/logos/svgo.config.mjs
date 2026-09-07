// SVGO settings for minifying `cmdr.svg` into the copies we ship over the wire.
// Run through `scripts/regenerate-icons.sh`, never by hand.
export default {
  multipass: true,
  plugins: [
    {
      name: 'preset-default',
      params: {
        overrides: {
          // The viewBox is what makes the logo scale; width/height are only a
          // default size. Dropping it pins the logo to 512 and breaks every
          // consumer that sizes it with CSS.
          removeViewBox: false,
          // `<title>` is the accessible name when the logo is inlined into a
          // page rather than loaded through an `<img alt>`.
          removeTitle: false,
        },
      },
    },
  ],
}
