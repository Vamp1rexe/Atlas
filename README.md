# Atlas

Minecraft-Launcher im Stil des Modrinth-Launchers, mit eigenem Cosmetics-System.

- **Bibliothek:** Instanzen für Vanilla und Fabric, Start per Klick. Atlas lädt Minecraft, Bibliotheken, Assets und das passende Java selbst.
- **Entdecken:** Mods, Modpacks (`.mrpack`), Resource Packs und Shader direkt über die Modrinth-API, inklusive benötigter Abhängigkeiten.
- **Cosmetics:** animierte Flügel (Engel, Dämon, Drache), wehende Capes, eigene Capes aus Bildern, Größe und Farbe einstellbar. Siehe [`cosmetics/README.md`](cosmetics/README.md).
- **Performance:** optimierte JVM-Flags und ein Performance-Pack (Sodium, Lithium, FerriteCore, Entity Culling, ImmediatelyFast, ModernFix) für Fabric-Instanzen.
- **Design:** Akzent- und Hintergrundfarbe per Hexcode oder Preset.
- **Account:** vorerst nur Offline-Modus. Der Microsoft-Login kommt als Nächstes.

## Aufbau

```
crates/atlas-core/   Launcher-Logik (Auth, Versionen, Java, Start, Modrinth, Cosmetics)
mod/                 Atlas-Cosmetics-Mod (Fabric, Minecraft 26.2)
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
