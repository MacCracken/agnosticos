# sharira

> **Sanskrit**: शरीर (body, physical form)

**Physiology engine** — skeletal structures, musculature, locomotion, biomechanics, respiratory, and cardiovascular systems.

- **Type**: Shared library crate
- **License**: GPL-3.0
- **Version**: 1.0.0
- **crates.io**: [sharira](https://crates.io/crates/sharira)
- **Repository**: [MacCracken/sharira](https://github.com/MacCracken/sharira)

## Relationship to Other Crates

| | sharira | jantu | impetus |
|---|---------|-------|---------|
| **Domain** | Physical body — bones, muscles, organs, biomechanics | Animal behavior — instinct, survival, swarm | Rigid body physics — forces, collisions, constraints |
| **Model** | Anatomical structures, muscle activation, locomotion gaits | Ethological drives, stimulus-response | Newtonian mechanics, spatial queries |
| **Consumers** | Creature physics, medical simulation, character animation | Creature AI, wildlife simulation | All physics-driven systems |

## Consumers

- **kiran** / **joshua** — character animation, creature locomotion, ragdoll physics
- **impetus** — anatomically-driven constraint systems
- **jantu** — physical basis for creature behavior (fatigue, injury, movement limits)
