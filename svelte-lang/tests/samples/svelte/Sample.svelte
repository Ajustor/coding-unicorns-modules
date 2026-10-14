<script lang="ts">
  export let items: string[] = [];
  let count = 0;
  const add = () => (count += 1);
</script>

<h1 class:big={count > 3}>Clicks: {count}</h1>
{#if items.length}
  {#each items as item, i (item)}
    <p on:click={add}>{i}: {item.toUpperCase()}</p>
  {/each}
{:else}
  <p>None</p>
{/if}

<style>
  h1 {
    color: rebeccapurple;
    font-size: 2rem;
  }
</style>
