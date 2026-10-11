# Hazlo Mexa website

A static, responsive landing page. Node.js 20 or later is used only to build and preview; there are no dependencies, API keys, trackers, or server-side services.

From this directory:

```sh
npm run build
npm test
npm run dev
```

The preview opens at http://127.0.0.1:4173. It serves only the page's public assets. Stop it with Ctrl+C.

## Cloudflare Pages

Connect the GitHub repository in Cloudflare Pages and use:

| Setting | Value |
| --- | --- |
| Production branch | `main` |
| Framework preset | None |
| Root directory | `website` |
| Build command | `npm run build` |
| Build output directory | `dist` |

No environment variables or runtime Node server are needed. Cloudflare serves the generated static files and applies `public/_headers`.

See [Cloudflare's build configuration documentation](https://developers.cloudflare.com/pages/configuration/build-configuration/).

Edit `public/index.html` for copy and `public/styles.css` for design. The workbench illustration is a concept, and all integrations are explicitly described as planned. This website is covered by the repository's MIT license.
