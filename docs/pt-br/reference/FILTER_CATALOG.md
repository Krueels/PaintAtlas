# Inventário de filtros e escopo de implementação

Pesquisa de 2026-10-24: PaintAtlas expõe **73 comandos de filtro em 14
categorias**. O inventário compara famílias documentadas da Adobe; disponibilidade
no Photoshop varia por versão, plataforma e documento. Plugins de terceiros não
estão incluídos.

## Fontes

- [Referência de efeitos de filtro da Adobe](https://helpx.adobe.com/photoshop/desktop/effects-filters/blur-sharpen-filters/filter-effects-reference.html)
- [Galeria de filtros](https://helpx.adobe.com/photoshop/desktop/effects-filters/get-started-with-filters/filter-gallery.html)
- [Blur Gallery](https://helpx.adobe.com/photoshop/desktop/effects-filters/blur-sharpen-filters/use-blur-gallery-depth-of-field-effects.html)
- [Neural Filters](https://helpx.adobe.com/photoshop/desktop/effects-filters/neural-filters/neural-filters-gallery.html)

As referências orientam os nomes e famílias; implementações do PaintAtlas são
decisões independentes e não incluem código ou algoritmos proprietários da
Adobe.

## Filtros disponíveis

Os filtros atuam na camada raster ativa e visível, respeitam a seleção e usam
prévia, cancelamento e undo em uma etapa. Só um filtro é aplicado por vez; não
há pilha persistente de efeitos nem Smart Objects.

| Categoria | Comandos |
|---|---|
| Blur | Average, Blur, Blur More, Box, Gaussian, Motion, Radial, Zoom, Shape e Surface Blur |
| Blur Gallery | Iris Blur, Tilt-Shift |
| Noise | Add Noise, Despeckle, Dust & Scratches, Median, Reduce Noise |
| Distort | Displace, Pinch, Polar Coordinates, Ripple, Shear, Spherize, Twirl, Wave, Zig Zag, Glass |
| Pixelate | Color Halftone, Crystallize, Facet, Fragment, Mezzotint, Mosaic, Pointillize |
| Render | Clouds, Difference Clouds, Fibers, Lens Flare |
| Sharpen | Sharpen, Sharpen More, Sharpen Edges, Adaptive Sharpen, Unsharp Mask |
| Stylize | Diffuse, Emboss, Find Edges, Oil Paint, Solarize, Trace Contour, Wind |
| Artistic | Colored Pencil, Dry Brush, Film Grain, Neon Glow, Poster Edges, Watercolor |
| Brush Strokes | Crosshatch, Ink Outlines |
| Sketch | Graphic Pen, Photocopy, Stamp, Halftone Pattern |
| Texture | Grain, Mosaic Tiles, Patchwork, Stained Glass, Texturizer |
| Video | De-Interlace |
| Other | Custom 3×3, High Pass, Maximum, Minimum, Offset |

## Diferenças e limites

- Gaussian Blur usa três passagens separáveis de box como aproximação; buffers
  premultiplicados ajudam a evitar franjas escuras em transparências.
- Blur de movimento/radial/zoom/forma usa amostragem limitada. Iris e Tilt-Shift
  misturam imagem desfocada e máscara de foco; não reproduzem controles de lente
  ou bokeh da Adobe.
- Surface Blur, Reduce Noise, Despeckle e Dust & Scratches são aproximações
  próprias. Oil Paint usa vizinhança no estilo Kuwahara; efeitos artísticos e
  procedurais também são implementações independentes.
- Convolução Custom aceita kernel 3×3, divisor e offset. Filtros aleatórios usam
  seed determinística, com opção para alterá-la.
- A galeria usa miniaturas de demonstração; processamento e miniaturas rodam em
  tarefas de fundo. Efeitos espaciais podem alterar alfa; alpha lock é sempre
  respeitado.

Smart Filters, Neural Filters, Liquify, Camera Raw, Vanishing Point, vários
filtros especializados da Adobe e plugins externos permanecem adiados. Alguns
exigiriam modelos/licenças, interfaces de edição próprias ou arquitetura
persistente de efeitos.

Os filtros trabalham atualmente com RGB(A) de 8 bits. Aplicar incorpora os
resultados aos pixels; parâmetros do último filtro duram apenas na sessão.
Filtros de muitas amostras podem demorar em resolução completa, embora a
interface permaneça responsiva.
