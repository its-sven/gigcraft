# Where GigCraft lives

*Status 2026-11-25. Decision record: [ADR-0006](../adr/0006-hosting.md).*

## Recommendation

| What | Where | Why |
|---|---|---|
| Source code | **GitHub `its-sven/gigcraft`**, private at first, public after Phase 0 | ArtCraft/craft-libs are on GitHub; ArtCraft Launcher installs from GitHub releases; macOS runners for iOS builds; Sven's open-source projects are there |
| Later, with contributors | GitHub organisation (e.g. `gigcraft-app`; `gigcraft` is taken by a user account) | transfer keeps redirects, issues and stars |
| Issues, discussions, roadmap | GitHub Issues + Projects + Discussions | one place |
| Releases | GitHub Releases (desktop, SHA-256), TestFlight/App Store (iOS), later Flathub, F-Droid/Play | |
| Docs/website | GitHub Pages from `docs/` (later own domain) | free, versioned |
| Server image | GitHub Container Registry (`ghcr.io/its-sven/gigcraft-server`) | next to the code |
| Mirror (optional) | Codeberg | independence |
| Community | ArtCraft Discord (ask about community-app listing), later own Discussions | |

## Checks before going public

1. Name: "GigCraft" not found as a music app (App Store/web search 2026-11-25); several unrelated small GitHub repos use it (freelancer tools). Run EUIPO/DPMA/USPTO searches.
2. Domains `gigcraft.app` / `gigcraft.org` / `gigcraft.de`: availability not checked yet.
3. README states independence from ArtCraft and from BandHelper/Stage Traxx; no third-party logos.
4. No private or company data (internal names, domains, IPs) anywhere in the repo.

## Commands to create the GitHub repo (run by the maintainer)

```bash
cd ~/Documents/dev/repos/gigcraft
gh repo create its-sven/gigcraft --private --source . --description "Open-source app for gigging musicians and bands: setlists, charts, backing tracks, MIDI and band sync"
git push -u origin main
```
