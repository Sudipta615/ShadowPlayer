# Bundled fonts

ShadowPlayer embeds Inter plus six optional families so themes can change
typography without depending on fonts installed on the host system.

| File | Family | Source |
| :--- | :--- | :--- |
| `Inter-Regular.ttf`, `Inter-Medium.ttf` | Inter | https://github.com/rsms/inter |
| `families/Fraunces-*.ttf` | Fraunces | https://github.com/google/fonts/tree/main/ofl/fraunces |
| `families/InstrumentSerif-Regular.ttf` | Instrument Serif | https://github.com/google/fonts/tree/main/ofl/instrumentserif |
| `families/SpaceGrotesk-*.ttf` | Space Grotesk | https://github.com/google/fonts/tree/main/ofl/spacegrotesk |
| `families/DMSans-*.ttf` | DM Sans | https://github.com/google/fonts/tree/main/ofl/dmsans |
| `families/BricolageGrotesque-*.ttf` | Bricolage Grotesque | https://github.com/google/fonts/tree/main/ofl/bricolagegrotesque |
| `families/JetBrainsMono-*.ttf` | JetBrains Mono | https://github.com/google/fonts/tree/main/ofl/jetbrainsmono |

All families are licensed under the SIL Open Font License 1.1. The original
licence text for each family is in `licenses/`; Inter's licence is
`LICENSE.txt`.

The Google Fonts files are variable fonts. The `families/*-Regular.ttf` and
`families/*-Medium.ttf` files are fixed instances generated with
`fontTools.varLib.instancer` at weights 400 and 500/600 so egui does not
rasterise an unintended default instance. Optical-size axes were pinned to
body and display values where the family exposes them.
