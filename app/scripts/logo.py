#!/usr/bin/env python3
"""Erzeugt das Tory-Zeichen.

Das Motiv ist nicht frei erfunden, sondern die Bildsprache der App selbst: jede
Zeile auf dem Startscreen traegt links einen schmalen Dringlichkeitsbalken, und
rechts davon steht, worum es geht. Genau das ist das Zeichen — ein tuerkiser
Balken und drei Zeilen, von denen nur die oberste leuchtet.

"Signal vor Detail" als Bild: eine Sache ist wichtig, der Rest ist Kontext.

Erzeugt vier Dateien, die `tauri icon` weiterverarbeitet:

  app-icon.png             das vollstaendige Zeichen (Desktop, iOS)
  app-icon-bg.png          Hintergrundebene fuer Androids adaptive Symbole
  app-icon-fg.png          Vordergrundebene dazu, freigestellt
  app-icon-monochrome.png  einfarbig, fuer Androids eingefaerbte Symbole

Aufruf:  python3 scripts/logo.py
Danach:  npx tauri icon icons/app-icon.json
"""

from pathlib import Path

from PIL import Image, ImageDraw

# Vierfache Abtastung, danach herunterskaliert — Pillow kann keine Kanten
# glaetten, aber ein Viertel von vier Pixeln ist genau das.
SS = 4
SIZE = 1024

# Dieselben Werte wie in app/src/app.css, Richtung "Cockpit".
GRUND_OBEN = (12, 15, 17)
GRUND_UNTEN = (22, 28, 30)
AKZENT = (86, 183, 200)
GEDAEMPFT = (58, 74, 78)
SEHR_GEDAEMPFT = (42, 54, 57)


def verlauf(size: int) -> Image.Image:
    """Senkrechter Verlauf. Eine flache Flaeche wirkt auf einem Homescreen tot."""
    bild = Image.new("RGB", (1, size))
    for y in range(size):
        t = y / max(size - 1, 1)
        bild.putpixel(
            (0, y),
            tuple(round(a + (b - a) * t) for a, b in zip(GRUND_OBEN, GRUND_UNTEN)),
        )
    return bild.resize((size, size), Image.Resampling.BILINEAR)


def motiv(zeichnung: ImageDraw.ImageDraw, size: int, einfarbig: tuple | None = None) -> None:
    """Balken und drei Zeilen, alles relativ zur Kantenlaenge."""
    e = size / 1024  # Einheit

    akzent = einfarbig or AKZENT
    mittel = einfarbig or GEDAEMPFT
    schwach = einfarbig or SEHR_GEDAEMPFT

    # Der Dringlichkeitsbalken links.
    zeichnung.rounded_rectangle(
        [232 * e, 286 * e, 296 * e, 738 * e], radius=32 * e, fill=akzent
    )

    # Drei Zeilen. Nur die oberste leuchtet — das ist die Aussage.
    # Auf die Mitte des Balkens ausgerichtet, nicht auf die der Flaeche: sonst
    # steht der Block optisch zu hoch.
    for y, breite, farbe in (
        (327, 468, akzent),
        (479, 360, mittel),
        (631, 250, schwach),
    ):
        zeichnung.rounded_rectangle(
            [368 * e, y * e, (368 + breite) * e, (y + 66) * e],
            radius=33 * e,
            fill=farbe,
        )


def gerundete_maske(size: int, radius_anteil: float) -> Image.Image:
    maske = Image.new("L", (size, size), 0)
    ImageDraw.Draw(maske).rounded_rectangle(
        [0, 0, size - 1, size - 1], radius=size * radius_anteil, fill=255
    )
    return maske


def speichern(bild: Image.Image, pfad: Path) -> None:
    bild.resize((SIZE, SIZE), Image.Resampling.LANCZOS).save(pfad, "PNG", optimize=True)
    print(f"  {pfad.name:26} {pfad.stat().st_size / 1024:6.1f} kB")


def main() -> None:
    ziel = Path(__file__).resolve().parent.parent / "icons"
    ziel.mkdir(parents=True, exist_ok=True)
    gross = SIZE * SS
    print("Tory-Zeichen:")

    # 1 · Das vollstaendige Zeichen, mit abgerundeter Kante wie ein App-Symbol.
    voll = Image.new("RGBA", (gross, gross), (0, 0, 0, 0))
    grund = verlauf(gross).convert("RGBA")
    grund.putalpha(gerundete_maske(gross, 0.225))
    voll.alpha_composite(grund)
    motiv(ImageDraw.Draw(voll), gross)
    speichern(voll, ziel / "app-icon.png")

    # 2 · Hintergrundebene: randlos, Android schneidet die Form selbst zu.
    speichern(verlauf(gross).convert("RGBA"), ziel / "app-icon-bg.png")

    # 3 · Vordergrundebene: nur das Motiv, freigestellt. Android beschneidet auf
    #     die inneren zwei Drittel, deshalb sitzt es kleiner in der Flaeche.
    vorder = Image.new("RGBA", (gross, gross), (0, 0, 0, 0))
    inneres = Image.new("RGBA", (gross, gross), (0, 0, 0, 0))
    motiv(ImageDraw.Draw(inneres), gross)
    rand = round(gross * 0.16)
    vorder.alpha_composite(
        inneres.resize((gross - 2 * rand, gross - 2 * rand), Image.Resampling.LANCZOS),
        (rand, rand),
    )
    speichern(vorder, ziel / "app-icon-fg.png")

    # 4 · Einfarbig fuer eingefaerbte Symbole: Android faerbt die Deckkraft ein,
    #     also zaehlt nur die Form.
    mono = Image.new("RGBA", (gross, gross), (0, 0, 0, 0))
    inneres = Image.new("RGBA", (gross, gross), (0, 0, 0, 0))
    motiv(ImageDraw.Draw(inneres), gross, einfarbig=(255, 255, 255))
    mono.alpha_composite(
        inneres.resize((gross - 2 * rand, gross - 2 * rand), Image.Resampling.LANCZOS),
        (rand, rand),
    )
    speichern(mono, ziel / "app-icon-monochrome.png")

    manifest = ziel / "app-icon.json"
    manifest.write_text(
        '{\n'
        '  "default": "app-icon.png",\n'
        '  "bg_color": "#0C0F11",\n'
        '  "android_bg": "app-icon-bg.png",\n'
        '  "android_fg": "app-icon-fg.png",\n'
        '  "android_monochrome": "app-icon-monochrome.png"\n'
        '}\n'
    )
    print(f"  {manifest.name:26} Manifest fuer `tauri icon`")


if __name__ == "__main__":
    main()
