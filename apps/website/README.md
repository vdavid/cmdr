# Website

Cmdr marketing website built with Astro and Tailwind CSS v4.

## Overview

A static landing page for [getcmdr.com](https://getcmdr.com) featuring:

- AI-focused messaging (natural language file operations)
- 14-day free trial CTA
- Feature highlights
- Download section
- Dark design for everyone

## Tech stack

- **Astro** — Static site generator, zero JS by default
- **Tailwind CSS v4** — Styling with CSS-first configuration
- **Docker + nginx** — Production deployment

## Development

```bash
# Install dependencies
pnpm install

# Start dev server
pnpm run dev
```

The dev server runs at `http://localhost:4321`.

## Building

```bash
# Build static site
pnpm run build

# Preview production build
pnpm run preview
```

Output goes to `dist/`.

## Deployment

The site is containerized for deployment to any Docker host.

### Build and run locally

```bash
docker build -t getcmdr-static .
docker run -p 8080:80 getcmdr-static
```

### Deploy to server

The website is automatically deployed when changes are pushed to `main` and CI passes.

For server setup and troubleshooting, see [Deploying the website](../../docs/guides/deploy-website.md).

```yaml
# Caddyfile on server
getcmdr.com { reverse_proxy getcmdr-static:80 }
```

## Project structure

```
apps/website/
├── src/
│   ├── components/
│   │   ├── Header.astro     # Fixed navigation
│   │   ├── Hero.astro       # Main hero section
│   │   ├── Features.astro   # Feature grid
│   │   ├── Download.astro   # Download CTA
│   │   └── Footer.astro     # Footer
│   ├── layouts/
│   │   └── Layout.astro     # Base HTML layout
│   ├── pages/
│   │   └── index.astro      # Landing page
│   └── styles/
│       └── global.css       # Tailwind theme
├── public/
│   └── favicon.png
├── Dockerfile
├── docker-compose.yml
├── nginx.conf
├── astro.config.mjs
└── package.json
```

## Design

The design is inspired by [linear.app](https://linear.app):

- Warm dark background (`#14130f`)
- Subtle gradients and grid patterns
- Accent color: Indigo (`#6366f1`)
- `Inter` font family
- Smooth fade-in animations
