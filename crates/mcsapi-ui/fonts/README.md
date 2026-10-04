# Bundled fonts

Subsets of the [Nerd Fonts](https://github.com/ryanoasis/nerd-fonts) v3.4.0
patched faces, made with `pyftsubset` to keep the crate small:

| File | Source | Role |
| --- | --- | --- |
| `NotoSansNerdFontPropo-Light.ttf` | NotoSans Nerd Font Propo Light | UI text (egui proportional) |
| `NotoSansNerdFontPropo-Medium.ttf` | NotoSans Nerd Font Propo Medium | Strong UI text |
| `CousineNerdFont-Regular.ttf` | Cousine Nerd Font Regular | Monospace, shortcuts, the command palette prompt |

Kept ranges: Basic Latin, Latin-1, Latin Extended-A/B and Additional (Czech,
Slovak, Polish, Hungarian, Slovene, Croatian, Romanian…), combining marks,
general punctuation, currency, letterlike symbols, arrows, math operators,
box drawing, geometric shapes, misc symbols and dingbats where the faces have
them, plus these Nerd Font icon sets: Powerline (U+E0A0–E0D7), Seti
(U+E5FA–E6B7), Devicons (U+E700–E8EF), Codicons (U+EA60–EC1E), Font Awesome
(U+ED00–F2FF) and Octicons (U+F400–F533). Colour emoji and anything else fall
back to egui's bundled fonts.

To regenerate, run from a directory holding the full v3.4.0 files:

```sh
pyftsubset NotoSansNerdFontPropo-Light.ttf \
  --unicodes="U+0020-007E,U+00A0-024F,U+0259,U+02B0-02FF,U+0300-036F,U+1E00-1EFF,U+2000-206F,U+20A0-20CF,U+2100-214F,U+2190-21FF,U+2200-22FF,U+2300-23FF,U+2460-24FF,U+2500-25FF,U+2600-26FF,U+2700-27BF,U+E0A0-E0D7,U+E5FA-E6B7,U+E700-E8EF,U+EA60-EC1E,U+ED00-F2FF,U+F400-F533,U+FFFD" \
  --layout-features='kern,liga,calt,ccmp,mark,mkmk' --name-IDs='*' --name-languages='*' \
  --output-file=NotoSansNerdFontPropo-Light.ttf
```

(Same arguments for the other two files.)

## Licenses

- Noto Sans: SIL Open Font License 1.1, © Google LLC.
- Cousine: Apache License 2.0, © Google LLC.
- Nerd Fonts patcher and glyph sets: MIT (Nerd Fonts), Codicons CC BY 4.0
  (Microsoft), Font Awesome Free SIL OFL 1.1 (Fonticons, Inc.), Octicons MIT
  (GitHub), Devicons MIT, Seti UI MIT, Powerline Extra Symbols MIT.
