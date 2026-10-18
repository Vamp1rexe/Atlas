# Cosmetics

Ein Cosmetic besteht aus drei Dateien plus einem Eintrag in `catalog.json`:

| Datei | Zweck |
|---|---|
| `models/<id>.json` | Geometrie (Quader) und Platzierung |
| `textures/<id>.png` | Textur, UV wie bei Minecraft-Boxen |
| `previews/<id>.png` | Vorschaubild für den Launcher (128×128) |

## catalog.json

```json
{
  "id": "halo",
  "name": "Halo",
  "slot": "hat",
  "rarity": "rare",
  "description": "Goldener Ring über dem Kopf.",
  "model": "models/halo.json",
  "texture": "textures/halo.png",
  "preview": "previews/halo.png"
}
```

- `slot`: `hat`, `back`, `cape`, `wings`, `face`, `pet` – pro Slot ist ein Cosmetic ausrüstbar.
- `rarity`: `common`, `rare`, `epic`, `legendary`.

## Modellformat (`format: 1`)

```json
{
  "format": 1,
  "texture_size": [16, 16],
  "attach": "head",
  "offset": [0, -10, 0],
  "scale": 1.0,
  "animation": { "type": "bob", "speed": 1.0, "amount": 0.5 },
  "cubes": [{ "from": [-4, 0, -4], "to": [4, 1, -3], "uv": [0, 0] }]
}
```

- Einheiten sind Pixel (1/16 Block), Ursprung ist der Mittelpunkt des `attach`-Punkts (`head`, `body`, `back`).
- `animation` ist optional: `bob` (auf und ab), `spin` (drehen), `flap` (Flügelschlag).

## Ablauf

1. Der Launcher synchronisiert den Katalog und die Dateien nach `%APPDATA%\Atlas\cosmetics`.
2. Beim Start übergibt er dem Spiel `-Datlas.cosmetics.dir`, `-Datlas.cosmetics.catalog` und `-Datlas.equipped=<ids>`.
3. Der Atlas-Cosmetics-Mod (Fabric, noch nicht gebaut) liest diese Werte und zeichnet die Modelle am Spieler.
