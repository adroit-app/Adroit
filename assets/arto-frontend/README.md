# Arto's frontend bundles

The `arto-frontend` crate carries the stylesheet and scripts the Markdown
viewer draws a document with. They are built from JavaScript and CSS packages
whose licences the crate does not carry, so they are kept here and copied into
`Adroit.app` beside `THIRD-PARTY-LICENSES.txt`.

| File | Package | What it draws |
| --- | --- | --- |
| `pele.LICENSE.txt` | [Pele](https://github.com/kepano/pele) | Mermaid diagrams |
| `katex.LICENSE.txt` | [KaTeX](https://github.com/KaTeX/KaTeX) | formulas, and the fonts in the stylesheet |
| `highlight.js.LICENSE.txt` | [highlight.js](https://github.com/highlightjs/highlight.js) | the colours of code |
| `html2canvas.LICENSE.txt` | [html2canvas](https://github.com/niklasvh/html2canvas) | a formula copied as an image |
| `css-line-break.LICENSE.txt` | [css-line-break](https://github.com/niklasvh/css-line-break) | bundled with html2canvas |
| `text-segmentation.LICENSE.txt` | [text-segmentation](https://github.com/niklasvh/text-segmentation) | bundled with html2canvas |
| `primer-css.LICENSE.txt` | [Primer CSS](https://github.com/primer/css) | the document's styles |
| `primer-primitives.LICENSE.txt` | [Primer Primitives](https://github.com/primer/primitives) | the themes' colours |

When `arto-frontend` is bumped, compare this list with the `dependencies` of
Arto's `frontend/package.json` at the release it names.
