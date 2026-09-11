<script lang="ts">
  // Ein beschriftetes Eingabefeld. Existiert, damit die Einstellungen nicht aus
  // 200 Zeilen wiederholtem Label-Input-Markup bestehen.
  let {
    label,
    value = $bindable(),
    typ = "text",
    platzhalter = "",
    hinweis = "",
  }: {
    label: string;
    value: string | number;
    typ?: "text" | "number" | "url";
    platzhalter?: string;
    hinweis?: string;
  } = $props();
</script>

<label class="feld">
  <span class="label">{label}</span>
  {#if typ === "number"}
    <input type="number" bind:value placeholder={platzhalter} />
  {:else}
    <input type="text" inputmode={typ === "url" ? "url" : "text"} bind:value
      placeholder={platzhalter} autocapitalize="off" spellcheck="false" />
  {/if}
  {#if hinweis}<span class="hinweis">{hinweis}</span>{/if}
</label>

<style>
  .feld {
    display: block;
    margin-bottom: 12px;
  }

  .feld input {
    margin-top: 4px;
  }

  .hinweis {
    display: block;
    font-size: 11px;
    color: var(--text-sehr-leise);
    margin-top: 3px;
  }
</style>
