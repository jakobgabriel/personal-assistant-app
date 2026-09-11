// Zeit- und Textformate. Alles an einer Stelle, damit "in 5 h" ueberall
// gleich aussieht.

import type { Signal, SyncState, TimeKind, Urgency } from "./types";

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const TAG = 24 * HOUR;

/** Kalendertage zwischen zwei Zeitpunkten — nicht 24-Stunden-Schritte. */
function tageDazwischen(a: Date, b: Date): number {
  const startA = new Date(a.getFullYear(), a.getMonth(), a.getDate()).getTime();
  const startB = new Date(b.getFullYear(), b.getMonth(), b.getDate()).getTime();
  return Math.round((startA - startB) / TAG);
}

/**
 * Die Zeitangabe einer Signalzeile. `time_kind` entscheidet die Formulierung —
 * darum gibt es das Feld: sonst braeuchte jede Domaene eine eigene Regel.
 */
export function zeitText(signal: Signal, jetzt: Date = new Date()): string {
  if (!signal.at) return "";
  const at = new Date(signal.at);
  const art: TimeKind = signal.time_kind ?? "at";
  const delta = at.getTime() - jetzt.getTime();

  if (art === "since") return `vor ${dauer(Math.abs(delta))}`;

  if (art === "window" && signal.window_end) {
    const bis = new Date(signal.window_end);
    return `${uhrzeit(at)}–${uhrzeit(bis)}`;
  }

  if (art === "due") {
    const tage = tageDazwischen(at, jetzt);
    if (tage < 0) return tage === -1 ? "gestern faellig" : `${Math.abs(tage)} T ueberfaellig`;
    if (tage === 0) return "heute faellig";
    if (tage === 1) return "morgen faellig";
    if (tage <= 6) return `Frist ${tage} T`;
    return `faellig ${datum(at)}`;
  }

  if (delta < 0) return `seit ${dauer(-delta)}`;
  const tage = tageDazwischen(at, jetzt);
  if (tage === 0) return uhrzeit(at);
  if (tage === 1) return `morgen ${uhrzeit(at)}`;
  if (tage <= 6) return `in ${tage} T`;
  return datum(at);
}

function dauer(ms: number): string {
  if (ms < MINUTE) return "einem Moment";
  if (ms < HOUR) return `${Math.round(ms / MINUTE)} Min`;
  if (ms < TAG) return `${Math.round(ms / HOUR)} h`;
  return `${Math.round(ms / TAG)} T`;
}

const uhrzeit = (d: Date): string =>
  d.toLocaleTimeString("de-DE", { hour: "2-digit", minute: "2-digit" });

const datum = (d: Date): string =>
  d.toLocaleDateString("de-DE", { day: "2-digit", month: "short" });

/** Wie alt der Stand einer Quelle ist. */
export function syncText(state: SyncState, jetzt: Date = new Date()): string {
  if (state.fault?.kind === "offline") return "kein Netz";
  if (!state.last_ok) return state.fault ? "nie erfolgreich" : "noch nicht geladen";
  const alter = jetzt.getTime() - new Date(state.last_ok).getTime();
  if (alter < 2 * MINUTE) return "gerade eben";
  return `vor ${dauer(alter)}`;
}

/** Ein Stand aelter als zwei Stunden ist kein aktueller Stand mehr. */
export const istVeraltet = (state: SyncState, jetzt: Date = new Date()): boolean =>
  !state.last_ok || jetzt.getTime() - new Date(state.last_ok).getTime() > 2 * HOUR;

export const dringlichkeitLabel = (u: Urgency): string =>
  ({ critical: "Jetzt", high: "Heute", normal: "Bald", info: "Info" })[u];

/** Ueberschrift nach Tageszeit. */
export function gruss(jetzt: Date = new Date()): string {
  const h = jetzt.getHours();
  if (h < 5) return "Gute Nacht";
  if (h < 11) return "Guten Morgen";
  if (h < 18) return "Guten Tag";
  return "Guten Abend";
}

export const heuteLang = (jetzt: Date = new Date()): string =>
  jetzt.toLocaleDateString("de-DE", { weekday: "long", day: "numeric", month: "long" });
