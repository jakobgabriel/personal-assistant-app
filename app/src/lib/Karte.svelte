<script lang="ts">
  // Eine Domaenenkarte. Dieselbe `Overview` bedient in Richtung B die
  // Kennzahlenzeile und in A die Karte — hier die dichte Variante.
  import { istVeraltet, syncText } from "./format";
  import { sourceId, type Card } from "./types";

  let { card, onOeffnen }: { card: Card; onOeffnen: (id: string) => void } = $props();

  const stand = $derived(syncText(card.sync));
  const alt = $derived(istVeraltet(card.sync));
  const id = $derived(sourceId(card.source));
</script>

<button class="karte" class:veraltet={alt} onclick={() => onOeffnen(id)}>
  <div class="oben">
    <span class="label">{card.source.label}</span>
    <span class="stand label">{stand}</span>
  </div>

  {#if card.overview}
    <div class="kennzahl">
      <span class="zahl wert">{card.overview.metric}</span>
      <span class="bezeichnung leise">{card.overview.caption}</span>
    </div>

    {#if card.overview.note}
      <div class="zusatz">{card.overview.note}</div>
    {/if}

    {#if card.overview.progress !== undefined && card.overview.progress !== null}
      <div class="balkenspur">
        <div class="fortschritt" style="width: {Math.round(card.overview.progress * 100)}%"></div>
      </div>
    {/if}

    {#if card.overview.lines?.length}
      <ul class="zeilen">
        {#each card.overview.lines.slice(0, 4) as zeile (zeile.text)}
          <li class="u-{zeile.urgency ?? 'info'}">
            <span class="punkt"></span>
            <span class="text">{zeile.text}</span>
            {#if zeile.note}<span class="notiz leise">{zeile.note}</span>{/if}
          </li>
        {/each}
      </ul>
    {/if}
  {:else}
    <div class="leer leise">
      {card.sync.fault ? card.sync.fault.kind : "noch nichts geladen"}
    </div>
  {/if}
</button>

<style>
  .karte {
    display: block;
    width: 100%;
    text-align: left;
    background: var(--flaeche);
    border: 1px solid var(--linie);
    border-radius: var(--radius);
    padding: 12px 14px;
  }

  .oben {
    display: flex;
    justify-content: space-between;
    gap: 8px;
  }

  .stand {
    flex: 0 0 auto;
  }

  .kennzahl {
    display: flex;
    align-items: baseline;
    gap: 7px;
    margin-top: 6px;
  }

  .wert {
    font-size: 26px;
    line-height: 1.1;
  }

  .bezeichnung {
    font-size: 12px;
  }

  .zusatz {
    font-size: 12px;
    color: var(--kritisch);
    margin-top: 2px;
  }

  .balkenspur {
    height: 3px;
    background: var(--linie);
    border-radius: 2px;
    margin-top: 8px;
    overflow: hidden;
  }

  .fortschritt {
    height: 100%;
    background: var(--akzent);
  }

  .zeilen {
    list-style: none;
    margin: 9px 0 0;
    padding: 0;
    display: grid;
    gap: 4px;
  }

  /* Zwei Texte nebeneinander gehen in einer 150 px breiten Kachel nicht auf:
     sie kuerzen sich gegenseitig weg, bis beide unleserlich sind. Deshalb ein
     Raster — die Notiz steht unter ihrer Zeile statt neben ihr. */
  .zeilen li {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    align-items: center;
    gap: 1px 7px;
    font-size: 12px;
  }

  .punkt {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--u, var(--info));
    grid-column: 1;
  }

  .text,
  .notiz {
    grid-column: 2;
    /* `minmax(0, 1fr)` oben und `min-width: 0` hier: erst zusammen darf die
       Spalte schmaler werden als ihr Inhalt, sonst greift `text-overflow` nie. */
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .notiz {
    font-size: 11px;
  }

  .leer {
    font-size: 12px;
    margin-top: 8px;
  }
</style>
