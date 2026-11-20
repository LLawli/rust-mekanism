#!/bin/bash
# Compara a janela do livro (com as abas de fora) do port e do original, por caso e escala. Com várias capturas (do port ou dos dois),
# fica o par de menos pixels diferentes (a mesma fase do `SlotSelectTime` e da animação). Uso: comparar_livro.sh PASTA (com oficial_*.png
# e port_*.png). Escreve recortes empilhados (port em cima) e o mapa da diferença em PASTA/cmp/.
cd "$1"; mkdir -p cmp
for e in 2 3; do
  if [ $e = 2 ]; then L=392; T=187; WY=88; H=540; else L=232; T=97; WY=87; H=360; fi
  X=$(( (L - 32) * e )); Y=$(( (T + WY - 2) * e )); W=$(( (147 + 34) * e )); A=$(( (H - T - WY + 2) * e ))
  for caso in $(ls oficial_chamber_livro_*_e$e*.png | sed -E "s/^oficial_(.*)_e$e(_[0-9]+)?\.png$/\1/" | sort -u); do
    melhor=; menor=999999999
    for o in oficial_${caso}_e$e.png oficial_${caso}_e${e}_*.png; do
      [ -e "$o" ] || continue
      magick "$o" -crop ${W}x${A}+$X+$Y +repage cmp/o.png
      for p in port_${caso}_e$e.png port_${caso}_e${e}_*.png; do
        [ -e "$p" ] || continue
        magick "$p" -crop ${W}x${A}+$X+$Y +repage cmp/p.png
        n=$(magick compare -metric AE -fuzz 1% cmp/p.png cmp/o.png null: 2>&1 | cut -d' ' -f1); n=${n%.*}
        if [ "$n" -lt "$menor" ]; then menor=$n; melhor="$p $o"; fi
      done
    done
    [ -n "$melhor" ] || { echo "$caso e$e: sem port"; continue; }
    set -- $melhor
    magick "$1" -crop ${W}x${A}+$X+$Y +repage cmp/p.png; magick "$2" -crop ${W}x${A}+$X+$Y +repage cmp/o.png
    magick compare -fuzz 1% cmp/p.png cmp/o.png cmp/${caso}_e${e}_diff.png 2>/dev/null
    magick cmp/p.png cmp/o.png -append cmp/${caso}_e${e}.png
    echo "$caso e$e: $menor pixels ($1 contra $2)"
  done
done
