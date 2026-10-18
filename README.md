# Atlas

Minecraft-Launcher im Stil des Modrinth-Launchers, mit eigenem Cosmetics-System.

- **Bibliothek:** Instanzen für Vanilla und Fabric, Start per Klick. Atlas lädt Minecraft, Bibliotheken, Assets und das passende Java selbst.
- **Entdecken:** Mods, Modpacks (`.mrpack`), Resource Packs und Shader direkt über die Modrinth-API, inklusive benötigter Abhängigkeiten.
- **Cosmetics:** Katalog aus Modell-, Textur- und Preview-Dateien. Siehe [`cosmetics/README.md`](cosmetics/README.md).
- **Account:** vorerst nur Offline-Modus. Der Microsoft-Login kommt als Nächstes.

## Aufbau

```
crates/atlas-core/   Launcher-Logik (Auth, Versionen, Java, Start, Modrinth, Cosmetics)
src-tauri/           Tauri-App (Befehle für die UI)
ui/                  Oberfläche (HTML/CSS/JS, kein Build-Schritt)
cosmetics/           Cosmetics-Katalog und Assets
```

## Bauen

Die Windows-`.exe` baut GitHub Actions bei jedem Push auf `main` (Artifact `Atlas-windows`). Ein Tag `v*` legt zusätzlich ein Release an.

Lokal (Windows, Rust + Node installiert):

```
npx @tauri-apps/cli@2 build
```

Ergebnis: `target/release/atlas.exe` (portabel) und `target/release/bundle/nsis/` (Installer). Voraussetzung ist die WebView2-Runtime, die Windows 11 mitbringt.

## Daten

Alles liegt unter `%APPDATA%\Atlas`: `instances/`, `versions/`, `libraries/`, `assets/`, `runtime/` (Java), `cosmetics/`, `settings.json`.
