<script>
  // Monochrome inline SVG icons (Lucide path data) per DESIGN.md Iconography:
  // 24x24 grid, fill=none, stroke=currentColor, stroke-width=2, round cap/join.
  // Icons inherit the color of their text context; never hardcode a color here.
  let { name, size = '1.2em' } = $props()

  const icons = {
    'refresh-cw': [
      'M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8',
      'M21 3v5h-5',
      'M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16',
      'M8 16H3v5',
    ],
    pencil: [
      'M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z',
      'm15 5 4 4',
    ],
    sun: [
      'M16 12a4 4 0 1 1-8 0 4 4 0 0 1 8 0Z',
      'M12 2v2',
      'M12 20v2',
      'm4.93 4.93 1.41 1.41',
      'm17.66 17.66 1.41 1.41',
      'M2 12h2',
      'M20 12h2',
      'm6.34 17.66-1.41 1.41',
      'm19.07 4.93-1.41 1.41',
    ],
    moon: ['M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z'],
    'chevron-up': ['m18 15-6-6-6 6'],
    'chevron-down': ['m6 9 6 6 6-6'],
    'chevron-left': ['m15 18-6-6 6-6'],
    'chevron-right': ['m9 18 6-6-6-6'],
    x: ['M18 6 6 18', 'm6 6 12 12'],
  }

  const paths = $derived(icons[name] ?? [])
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  {#each paths as d (d)}
    <path {d} />
  {/each}
</svg>

<style>
  svg {
    /* Align to the text baseline when used inline next to a label. */
    vertical-align: -0.2em;
    flex-shrink: 0;
  }
</style>
