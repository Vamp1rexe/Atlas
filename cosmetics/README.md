# Cosmetics

Ein Cosmetic besteht aus Modell, Textur, Vorschau und einem Eintrag in `catalog.json`. Die eingebauten Cosmetics erzeugt `tools/gen_cosmetics.py` (Pillow nötig):

```
python3 cosmetics/tools/gen_cosmetics.py
```

Das Skript schreibt Modelle, Texturen, Previews, `catalog.json` und `previews/design-sheet.png`. Ein neues Cosmetic ist eine Funktion, die ein `Design` zurückgibt, plus ein Eintrag in `DESIGNS`.

## catalog.json

```json
{
  "id": "wings_angel",
  "name": "Engelsflügel",
  "slot": "wings",
  "rarity": "epic",
  "description": "Weiße Federflügel, die ruhig schlagen.",
  "model": "models/wings_angel.json",
  "texture": "textures/wings_angel.png",
  "preview": "previews/wings_angel.png",
  "default_color": "#ffffff"
}
```

- `slot`: `hat`, `back`, `cape`, `wings`, `face`, `pet`. Pro Slot ist ein Cosmetic ausrüstbar.
- `rarity`: `common`, `rare`, `epic`, `legendary` (`custom` für eigene Capes).
- `default_color`: Farbe, mit der die Textur multipliziert wird. Texturen sind daher hell und neutral gemalt. Die Farbe lässt sich im Launcher pro Cosmetic ändern, genauso die Größe.

## Modellformat 2 (Knochen)

```json
{
  "format": 2,
  "attach": "body",
  "scale": 1.0,
  "bones": [
    {
      "name": "wing_l", "parent": null, "pivot": [2.5, 3, 2.3],
      "rotation": { "base": [0, -22, -55], "amp": [0, 18, 4], "move": [0, 10, 0], "speed": 2.4, "phase": 0 },
      "cubes": [{ "from": [2.5, 1.8, 2.3], "to": [19.5, 4.2, 3.3],
                  "faces": { "south": [0, 0, 17, 2], "north": [17, 0, 0, 2] } }]
    }
  ]
}
```

- Einheiten sind Pixel (1/16 Block), y zeigt nach unten (wie in Minecraft-Modellen). `attach` ist `head` (Ursprung am Hals, Kopf reicht bis y = -8, x/z ±4) oder `body` (Ursprung oben in der Körpermitte, Körper ±4 breit, ±2 tief; +z ist der Rücken, +x die linke Seite).
- `pivot` steht im Ruhezustand in Attach-Koordinaten. Kindknochen drehen und bewegen sich mit ihrem Elternknochen mit.
- `rotation.base` ist die feste Drehung in Grad (x, y, z). Reihenfolge: erst z, dann y, dann x. `amp` ist die Schwingung `amp * sin(speed * t + phase)` in Grad, `move` kommt dazu, wenn der Spieler läuft. Verschobene `phase`-Werte entlang einer Kette erzeugen die Welle (Federn, Cape).
- Wurzelknochen skalieren um ihren eigenen Pivot, damit Flügel beim Vergrößern an den Schultern bleiben.
- `faces` nennt pro Seite ein Rechteck `[u0, v0, u1, v1]` in Texeln (`west`, `north`, `east`, `south`, `up`, `down`). Ist `u1 < u0`, wird gespiegelt. `south` ist die Außenseite eines Rückencosmetics, so wie man sie von hinten sieht. Statt `faces` geht auch `uv` (Box-Layout wie bei Minecraft).
- Format 1 (`cubes` ohne Knochen, `animation: bob|spin`) wird weiterhin gelesen. Das Halo nutzt es.

Die Textur wird beim Laden texelweise abgetastet und gleichfarbige Texel werden zu Rechtecken verschmolzen. Dadurch kostet ein Frame nur wenige Matrixoperationen und ein Zeichnen pro Knochen. Beleuchtung und Halbtransparenz sind noch nicht umgesetzt, Seiten werden leicht abgedunkelt.

## Eigene Capes

Im Launcher unter Cosmetics → „Cape aus Bild“: Bild wählen, Ausschnitt verschieben und zoomen, speichern. Atlas legt daraus ein Cape mit acht gekoppelten Streifen an (weht im Stand und hebt sich beim Laufen) und trägt es in die Bibliothek ein. Eigene Capes liegen unter `%APPDATA%\Atlas\cosmetics` und werden beim Synchronisieren nicht überschrieben.

## Ablauf

1. Der Launcher synchronisiert Katalog und Dateien nach `%APPDATA%\Atlas\cosmetics`.
2. Beim Start schreibt er den zusammengeführten Katalog und `options.json` (Größe/Farbe) und übergibt dem Spiel `-Datlas.cosmetics.dir`, `-Datlas.cosmetics.catalog`, `-Datlas.options` und `-Datlas.equipped=<ids>`.
3. Der Atlas-Cosmetics-Mod (`mod/`, Fabric, Minecraft 26.2) liest das und zeichnet die Modelle am lokalen Spieler. Der Launcher installiert ihn samt Fabric API automatisch, sobald ein Cosmetic ausgerüstet ist.
