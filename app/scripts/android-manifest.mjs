// Traegt den Deep-Link-Eintrag in das erzeugte Android-Manifest ein.
//
// Warum es dieses Skript gibt: `tauri android init` erzeugt
// `gen/android/` neu, und `tauri-plugin-deep-link` kann ueber
// `tauri.conf.json` auf Android nur App Links (https) anmelden — ein eigenes
// Schema wie `de.tory.app://` muss im Manifest stehen. Ohne diesen Eintrag
// startet die Gmail-Anmeldung, aber die Rueckleitung kommt nie an.
//
// Das Skript ist idempotent: zweimal aufgerufen aendert es beim zweiten Mal
// nichts. Findet es den Verankerungspunkt nicht, bricht es mit einer Meldung ab,
// statt still ein Manifest ohne Rueckkanal durchzulassen — ein APK, das
// aussieht wie erwartet und es nicht ist, waere das schlechtere Ergebnis.
//
// Aufruf:  node scripts/android-manifest.mjs [--check]
//          --check aendert nichts und meldet nur, ob der Eintrag steht.

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const wurzel = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const manifest = resolve(
  wurzel,
  "src-tauri/gen/android/app/src/main/AndroidManifest.xml",
);

/** Muss zum `redirect_uri` in der Gmail-Konfiguration passen. */
const SCHEMA = "de.tory.app";

const EINTRAG = `
            <!-- OAuth-Rueckkanal: de.tory.app://oauth2
                 Eingetragen von scripts/android-manifest.mjs, weil ein eigenes
                 URL-Schema auf Android nicht aus tauri.conf.json kommen kann. -->
            <intent-filter>
                <action android:name="android.intent.action.VIEW" />
                <category android:name="android.intent.category.DEFAULT" />
                <category android:name="android.intent.category.BROWSABLE" />
                <data android:scheme="${SCHEMA}" />
            </intent-filter>`;

const nurPruefen = process.argv.includes("--check");

let xml;
try {
  xml = readFileSync(manifest, "utf8");
} catch {
  console.error(
    `Kein Manifest unter ${manifest}\n` +
      "Erst `npm run tauri -- android init` laufen lassen.",
  );
  process.exit(1);
}

if (xml.includes(`android:scheme="${SCHEMA}"`)) {
  console.log(`Deep-Link ${SCHEMA}:// steht bereits im Manifest.`);
  process.exit(0);
}

if (nurPruefen) {
  console.error(`Deep-Link ${SCHEMA}:// fehlt im Manifest.`);
  process.exit(1);
}

// Verankert wird am Ende des LAUNCHER-Filters der MainActivity: dort steht der
// Eintrag sicher innerhalb der richtigen Activity, und die Stelle ist die
// einzige im Manifest, die `category.LAUNCHER` enthaelt.
const launcher = xml.indexOf("android.intent.category.LAUNCHER");
if (launcher < 0) {
  console.error(
    "Im Manifest steht kein LAUNCHER-Filter — der Aufbau hat sich geaendert.\n" +
      `Bitte den intent-filter fuer ${SCHEMA} von Hand in die MainActivity eintragen:\n` +
      EINTRAG,
  );
  process.exit(1);
}

const ende = xml.indexOf("</intent-filter>", launcher);
if (ende < 0) {
  console.error("LAUNCHER-Filter ist nicht geschlossen — Manifest unerwartet.");
  process.exit(1);
}

const schnitt = ende + "</intent-filter>".length;
writeFileSync(manifest, xml.slice(0, schnitt) + EINTRAG + xml.slice(schnitt));
console.log(`Deep-Link ${SCHEMA}:// in das Manifest eingetragen.`);
