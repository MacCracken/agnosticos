# Logo review notes

Working notes from iterating on the logo set in this folder. Nothing here is decided — these are candidates for the next round, kept so the feedback isn't lost between passes.

## Round 1 — 2026-10-15

First back-and-forth on the set as merged (Λ mark with event horizon + accretion disk, Cinzel wordmark, JetBrains Mono tagline).

### Keep

- **The concept.** The A drawn as Λ, the event horizon in its counter, the disk in place of the crossbar — it still reads as an A without the explanation, and it carries the gnosis/agnostos story.
- **The type pairing.** Cinzel (inscriptional, classical) against widely tracked JetBrains Mono (terminal) says "old roots, systems underneath".
- **The palette discipline.** Warm off-white on near-black, amber used only on the disk.
- **The asset hygiene.** Outlined paths with no font dependency; light/dark/mono variants; colour and usage rules in the README.

### Candidate fixes

1. **The event horizon vanishes at small sizes.** It's the conceptual centre of the mark and it disappears exactly where the icon is seen most (favicon, taskbar, launcher). Measured from `agnos-icon.svg`: the ring's stroke is ~22 units and its diameter ~120 units in a ~730-unit letter, so it renders at:

   | Icon size | Ring stroke | Ring diameter | Disk thickness |
   |-----------|-------------|---------------|----------------|
   | 16 px | 0.3 px | 1.6 px | 0.5 px |
   | 32 px | 0.6 px | 3.3 px | 1.0 px |
   | 64 px | 1.2 px | 6.5 px | 2.1 px |

   At 32 and 64 the icon reads as "A with an orange slash". Idea: a separately drawn small-size icon (16/32/64) — larger ring, heavier disk, possibly no feet — rather than scaling the master down.

2. **The ring is small even at banner size.** About a sixth of the letter height; next to the wordmark it reads as a stray "c" or "e" rather than an object. Try scaling it ~1.5× and check it still clears both legs.

3. **The A's feet don't match Cinzel's serifs.** The legs stand on flat, unbracketed 15-unit bars with square joins; Cinzel's N beside it has bracketed, cupped serifs. Up close the A looks like it's standing on planks. The README says the A matches Cinzel's serifs — either redraw the feet with brackets or soften that claim.

4. **Optional: surface the backronym in the tagline.** Brighten the initials of "**A** **G**ENERAL **N**ETWORKED **O**PERATING **S**YSTEM" so the reader catches that the tagline spells the name.

Fix 1 is the one that matters; 2–4 are polish.
