#!/usr/bin/env python3
"""Tira do vídeo do `h2_cabo_forma.py contorno` os dois quadros de cada pose (as marcas em JSON: pose, segundo com, segundo sem): `<pose>.png` é o quadro com a interface (a mira no meio e o contorno do
bloco) e `<pose>_sem.png` o sem. O F1 do script alterna a interface, e o estado em que o jogo estava no começo não é conhecido: quem tem a mira (o centro do quadro mais claro) é o `com`.

    extrair_quadros.py VIDEO.mp4 MARCAS.json PASTA
"""
import json
import subprocess
import sys


def brilho(png):
    r = subprocess.run(["magick", png, "-crop", "30x30+945+525", "+repage", "-colorspace", "Gray", "-format", "%[fx:mean]", "info:"], capture_output=True, text=True)
    return float(r.stdout)


def main():
    video, marcas, pasta = sys.argv[1:4]
    for pose, a, b, *_ in json.load(open(marcas)):
        qa, qb = f"{pasta}/{pose}.png", f"{pasta}/{pose}_sem.png"
        for t, png in ((a, qa), (b, qb)):
            subprocess.run(["ffmpeg", "-v", "error", "-y", "-ss", str(t), "-i", video, "-frames:v", "1", png], check=True)
        if brilho(qb) > brilho(qa):
            subprocess.run(["mv", qa, qa + ".t"], check=True)
            subprocess.run(["mv", qb, qa], check=True)
            subprocess.run(["mv", qa + ".t", qb], check=True)


if __name__ == "__main__":
    main()
