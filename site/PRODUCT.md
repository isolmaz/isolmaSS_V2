# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Stack

Static HTML/CSS with a small dependency-free local script for language, mobile menu and illustrative carousel. No build step or external assets. Deployable to Workers Static Assets (`site/wrangler.jsonc`) or any static host. No runtime package dependencies or secrets.

## Users

Windows users evaluating or using isolmaSS who need download, installation, documentation, privacy, security, and terms answers. Site copy is concise Turkish and English, selectable on every page. Secondary reader: a maintainer deploying the site.

## Product Purpose

`ss.isolmaz.com` is the public informational/download site for isolmaSS, a native Windows capture editor. Success: within seconds a visitor knows the app keeps captures local, downloads only from the verified GitHub Releases latest path, and can reach installation, docs, security, privacy, and terms pages.

## Positioning

Capture → annotate → copy/save locally; upload is an explicit action after the user deploys private Worker/R2/D1 resources in their own Cloudflare account. The publisher hosts no screenshots or login service. Updates use a pinned RSA-3072 signature over SHA-256.

## Operating Context

- Public release channel: `github.com/isolmaz/isolmaSS-updates` (public, releases only). Source repo `isolmaz/isolmaSS_V2` is public; its Deploy button requires the v0.5.0 release tag and each user's Cloudflare authorization.
- Site hosting decision: Cloudflare Workers Static Assets for `ss.isolmaz.com`; never claim the site is live before its owner deploys it.
- Source version: 0.5.0 (local-first with opt-in BYO Cloudflare upload); cloud sharing remains inactive until the user pairs their Worker.
- Local copy/save always works and never depends on cloud configuration.

## Capabilities and Constraints

- Download link may point only to `https://github.com/isolmaz/isolmaSS-updates/releases/latest`; asset names appear as text, never guessed direct URLs.
- Site hosts no screenshots, no imagery of user captures, no external assets, no analytics, no cookies.
- Any pricing/cost wording (Cloudflare tiers, "free") must be labeled estimate, not guarantee; no fabricated testimonials, guarantees, SLAs, or legal entity.
- Real facts only from: README.md, SECURITY.md, DISTRIBUTION.md, CHANGELOG.md, ROADMAP.md, LICENSE (MIT).
- Honest disclosures required: SmartScreen may warn and is never bypassed; blur/highlighter are not secure redaction (Karart is opaque); signed updates begin at 0.4.0 (older installs need one manual install); update checks contact the GitHub API.

## Brand Commitments

Name: isolmaSS. Bilingual TR/EN, factual tone, accessible and responsive. Closely adapt the owner's `isolmaz/SSDownload-site` visual/structural pattern (light editorial layout, blue accent, hero plus illustrated carousel, feature grid, reading pages and footer) without copying SSDownload's product claims. Use illustrative UI art, not fabricated product screenshots.

## Evidence on Hand

Repo docs: README.md, SECURITY.md, DISTRIBUTION.md, CHANGELOG.md, ROADMAP.md, LICENSE, cloudflare/README.md. Absences that must not be fabricated: testimonials, user counts, authentic app screenshots, uptime/performance claims, a contact person or legal entity.

## Product Principles

1. Claim "local" only when it is true; frame cloud strictly as opt-in, user-owned, not live until configured.
2. One verified download path, everywhere.
3. State limits, warnings, and estimates honestly; never guarantee what the project does not control.
4. Local scripts only for accessible language/navigation/gallery; no analytics, tracking requests or site accounts.

## Accessibility & Inclusion

WCAG AA contrast, semantic landmarks, keyboard navigation with visible focus, responsive desktop/mobile composition, `prefers-reduced-motion` honored, no font dependency.
