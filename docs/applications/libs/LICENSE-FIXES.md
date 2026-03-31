# License Fixes — 2026-03-30

> **Rule**: `GPL-3.0-only` for all library crates, `AGPL-3.0-only` for desktop GUI apps.
>
> Fix = update `license` field in Cargo.toml + LICENSE file if needed.
> Republish to crates.io after fix if already published.

---

## AGPL → GPL (library crates wrongly using AGPL)

- [ ] abaco — `AGPL-3.0-only` → `GPL-3.0-only`
- [ ] aethersafta — `AGPL-3.0-only` → `GPL-3.0-only`
- [ ] agnosai — `AGPL-3.0-only` → `GPL-3.0-only`
- [ ] ai-hwaccel — `AGPL-3.0-only` → `GPL-3.0-only`
- [ ] bote — `AGPL-3.0-only` → `GPL-3.0-only`
- [ ] dhvani — `AGPL-3.0-only` → `GPL-3.0-only`
- [ ] hoosh — `AGPL-3.0-only` → `GPL-3.0-only`
- [ ] ifran — `AGPL-3.0-only` → `GPL-3.0-only`
- [ ] kavach — `AGPL-3.0-only` → `GPL-3.0-only`
- [ ] libro — `AGPL-3.0-only` → `GPL-3.0-only`
- [ ] majra — `AGPL-3.0-only` → `GPL-3.0-only`
- [ ] ranga — `AGPL-3.0-only` → `GPL-3.0-only`
- [ ] szal — `AGPL-3.0-only` → `GPL-3.0-only`

## GPL SPDX fix (library crates, just missing `-only`)

- [ ] delta — `GPL-3.0` → `GPL-3.0-only`
- [ ] dravya — `GPL-3.0` → `GPL-3.0-only`
- [ ] kana — `GPL-3.0` → `GPL-3.0-only`
- [ ] kiran — `GPL-3.0` → `GPL-3.0-only`
- [ ] murti — `GPL-3.0` → `GPL-3.0-only`
- [ ] pavan — `GPL-3.0` → `GPL-3.0-only`
- [ ] salai — `GPL-3.0` → `GPL-3.0-only`
- [ ] stiva — `GPL-3.0` → `GPL-3.0-only`
- [ ] sutra-community — `GPL-3.0` → `GPL-3.0-only`
- [ ] tanur — `GPL-3.0` → `GPL-3.0-only`
- [ ] tarang — `GPL-3.0` → `GPL-3.0-only`
- [ ] t-ron — `GPL-3.0` → `GPL-3.0-only`

## AGPL SPDX fix (desktop GUI apps, just missing `-only`)

- [ ] mneme — `AGPL-3.0` → `AGPL-3.0-only`
- [ ] rasa — `AGPL-3.0` → `AGPL-3.0-only`
- [ ] tazama — `AGPL-3.0` → `AGPL-3.0-only`

## License change (desktop GUI apps, wrong license type)

- [ ] aequi — `MIT OR AGPL-3.0` → `AGPL-3.0-only`

## Missing license in Cargo.toml (add field)

- [ ] agnostic — add `AGPL-3.0-only` (desktop GUI)
- [ ] bullshift — add `AGPL-3.0-only` (desktop GUI)
- [ ] photisnadi — add `AGPL-3.0-only` (desktop GUI)
- [ ] secureyeoman — add `AGPL-3.0-only` (desktop GUI)

## Already correct (no action needed)

These repos already have the right license:

**GPL-3.0-only** (45 library crates):
abaco¹, badal, bhava, bijli, bodh, dravya¹, falak, garjan, ghurni,
goonj, hisab, impetus, jantu, jivanu, jnana, jyotish, khanij, kimiya,
mabda, naad, nein, nidhi, pavan¹, prakash, pramana, prani, pravash,
raasta, sangha, sankhya, shabda, shabdakosh, sharira, shravan, soorat,
svara, tanmatra, ushma, vanaspati, vidya, yukti

¹ = after fix above

**AGPL-3.0-only** (desktop GUI apps):
abacus, aequi¹, jalwa, mneme¹, nazar, rahd, rasa¹, selah, shruti,
tazama¹, taswir, vidhana

¹ = after fix above

---

## Remove `publish = false` (blocks crates.io publish)

- [ ] bodh — science crate, will publish at v1
- [ ] falak — science crate, will publish at v1
- [ ] jivanu — science crate, will publish at v1
- [ ] joshua — game manager, will publish
- [ ] jyotish — science crate, will publish at v1
- [ ] mneme — needs coring (engine/GUI split), then publish engine
- [ ] sangha — science crate, will publish at v1
- [ ] tara — science crate, will publish at v1

**Intentionally `publish = false` (no action):** agnostik, agnosys, daimon

---

*42 items total. Delete this file when done.*
