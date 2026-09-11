#!/usr/bin/env python3
"""Prueft, dass jedes in abdeckung.json genannte Feld in den Kotlin-Quellen existiert.

Aufruf:  python3 contracts/pruefe_abdeckung.py
Exitcode 0 = jede Information aus den Mockups hat ein Feld im Modell.
"""
import json
import pathlib
import re
import sys

WURZEL = pathlib.Path(__file__).parent
QUELLEN = sorted((WURZEL / "src").rglob("*.kt"))

TYP = re.compile(r"^\s*(?:data\s+|sealed\s+|value\s+)?(?:class|interface|object|enum class)\s+(\w+)")
FELD = re.compile(r"\b(?:val|var)\s+(\w+)\s*:")
FUNKTION = re.compile(r"\bfun\s+(\w+)\s*\(")


def sammle_typen():
    """Ordnet jedem Typnamen seine Felder und Funktionen zu.

    Verschachtelte Typen (etwa in sealed interfaces) zaehlen als eigene Typen,
    ihre Felder gehoeren nur zu ihnen und nicht zum umschliessenden Typ.
    """
    typen: dict[str, set[str]] = {}
    for datei in QUELLEN:
        stapel: list[tuple[int, str]] = []  # (Einrueckung, Typname)
        for zeile in datei.read_text(encoding="utf-8").splitlines():
            if not zeile.strip() or zeile.lstrip().startswith("//"):
                continue
            einzug = len(zeile) - len(zeile.lstrip())
            treffer = TYP.match(zeile)
            if treffer:
                while stapel and stapel[-1][0] >= einzug:
                    stapel.pop()
                name = treffer.group(1)
                stapel.append((einzug, name))
                typen.setdefault(name, set())
                if "enum class" in zeile:
                    inhalt = zeile.split("{", 1)[1] if "{" in zeile else ""
                    typen[name].update(re.findall(r"\b([A-Z][A-Z0-9_]+)\b", inhalt))
                # Einzeiler wie: data class Zeitfenster(val von: Instant, val bis: Instant)
                rest = zeile[treffer.end():]
                typen[name].update(FELD.findall(rest))
                continue
            if not stapel:
                continue
            aktuell = stapel[-1][1]
            for regel in (FELD, FUNKTION):
                typen[aktuell].update(regel.findall(zeile))
    for datei in QUELLEN:
        text = datei.read_text(encoding="utf-8")
        for name, koerper in re.findall(r"enum class (\w+)\s*\{(.*?)\}", text, re.S):
            typen.setdefault(name, set()).update(re.findall(r"\b([A-Z][A-Z0-9_]{1,})\b", koerper))
    return typen


def main() -> int:
    typen = sammle_typen()
    karte = json.loads((WURZEL / "abdeckung.json").read_text(encoding="utf-8"))
    fehlend: list[str] = []
    geprueft = 0
    benutzte_typen: set[str] = set()

    for eintrag in karte["eintraege"]:
        for verweis in eintrag["felder"]:
            geprueft += 1
            typ, _, feld = verweis.partition(".")
            benutzte_typen.add(typ)
            if typ not in typen:
                fehlend.append(f"{eintrag['screen']:12} {eintrag['element'][:44]:46} Typ fehlt:  {typ}")
            elif feld and feld not in typen[typ]:
                fehlend.append(f"{eintrag['screen']:12} {eintrag['element'][:44]:46} Feld fehlt: {verweis}")

    print(f"{len(QUELLEN)} Kotlin-Dateien, {len(typen)} Typen")
    print(f"{len(karte['eintraege'])} Mockup-Elemente, {geprueft} Feldverweise geprueft")

    if fehlend:
        print(f"\nFEHLT ({len(fehlend)}):")
        for zeile in fehlend:
            print("  " + zeile)
        return 1

    unbenutzt = sorted(t for t in typen if t not in benutzte_typen)
    if unbenutzt:
        print("\nTypen ohne Mockup-Bezug (Infrastruktur oder noch nicht entworfen):")
        print("  " + ", ".join(unbenutzt))
    print("\nOK - jede Information aus den Mockups hat ein Feld im Modell.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
