# Rasayan

> **Rasayan** (Sanskrit: रसायन — alchemy, chemistry of life) — Biochemistry engine

| Field | Value |
|-------|-------|
| Status | Pre-1.0 |
| Version | `0.1.0` |
| Repository | `MacCracken/rasayan` |
| Runtime | library crate (Rust) |

---

## What It Does

- **Enzyme**: Michaelis-Menten kinetics, competitive/uncompetitive/mixed inhibition, Hill equation (allosteric cooperativity), Q10 temperature dependence
- **Metabolism**: Glycolysis, TCA cycle, oxidative phosphorylation — ATP/ADP balance, energy charge, metabolic rate, anaerobic detection
- **Signal**: Signal transduction cascades — receptor binding, second messengers (cAMP, Ca²⁺, IP3), dose-response curves
- **Protein**: Amino acid properties (20 standard), hydrophobicity scales, molecular weight, composition analysis
- **Membrane**: Nernst potential, Goldman-Hodgkin-Katz equation, Fick's diffusion, ion channel transport
- **Energy**: Bioenergetics — phosphocreatine system, glycogen reserves, MET levels, anaerobic threshold, ATP hydrolysis

## Relationship to Mastishk

Neurotransmitter synthesis requires metabolic precursors and enzymatic pathways. Rasayan models the biochemistry (enzyme kinetics, metabolic state) that feeds into mastishk's neurotransmitter dynamics. The membrane module provides the ion channel math underlying neural circuit firing.

## Relationship to Kimiya

Kimiya models general chemistry (reactions, kinetics, equilibria). Rasayan specializes in *biological* chemistry — enzyme-catalyzed reactions, metabolic pathways, and living system energetics. Rasayan uses Kimiya's reaction kinetics foundations.

## Consumers

- **mastishk** — neurotransmitter synthesis pathways, membrane potential for neural circuits
- **sharira** — muscle bioenergetics, fatigue modeling, ATP consumption
- **jivanu** — microbial metabolism, pharmacokinetics
- **kimiya** — shared reaction kinetics patterns
- **kiran** — creature/NPC metabolic simulation
