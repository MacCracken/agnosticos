# shabda

> Sanskrit: शब्द — word / sound

Grapheme-to-phoneme (G2P) conversion: text to phoneme sequences for vocal synthesis.

- **Version**: 0.1.0
- **Repository**: [github.com/MacCracken/shabda](https://github.com/MacCracken/shabda)
- **Depends on**: svara 1.0, hisab 1.2
- **Consumers**: dhvani, vansh

## Key Capabilities

- G2P engine with dictionary lookup + English rule-based fallback
- Built-in English pronunciation dictionary (~30 common/irregular words)
- Text normalization, sentence type detection, automatic stress assignment
- `speak()` one-call text-to-audio (G2P + svara rendering)
- G2P conversion in ~500ns, full speak in ~2ms
