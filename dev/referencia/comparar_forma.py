#!/usr/bin/env python3
"""O contorno dos transmissores no port e no original, empilhado (o port em cima, o original embaixo) e a contagem de pixels diferentes de cada recorte (frente `cabo-forma`).

    comparar_forma.py PORT_DIR ORIGINAL_DIR SAIDA_DIR [--tolerancia 6] [--clareia 35] [--escala 2]

`PORT_DIR` tem um quadro `<pose>.png` por pose (o contorno só aparece na captura PipeWire do gamescope: `horizon2/h2_cabo_forma.py contorno` grava e anota o segundo de cada pose, e o `ffmpeg`
tira o quadro); `ORIGINAL_DIR` tem os `forma_oficial_<pose>.png` do `oficial_forma.sh` (a mesma pose, a mesma câmera: yaw e pitch iguais no `latest.log` e nas marcas). Os recortes saem de `REGIOES`
(x, y, largura, altura em pixels do quadro de 1920 por 1080) e ficam com o contorno realçado (`-level 0%,CLAREIA%`: ele é fino e translúcido sobre o cabo escuro). A contagem é de `magick compare -metric AE`
com a tolerância por canal em porcento: no recorte inteiro entram o modelo do cabo (a frente `cabo-modelo` trata do visual) e o fundo, não só o contorno. Escreve `SAIDA_DIR/<pose>_empilhado.png` e
imprime `pose pixels_diferentes de total` por pose.
"""
import argparse
import os
import subprocess
import sys

REGIOES = {
    "pequeno_inteiro": (700, 420, 520, 280),
    "pequeno_oeste": (640, 330, 460, 320),
    "pequeno_leste": (820, 330, 460, 320),
    "grande_inteiro": (700, 420, 520, 280),
    "grande_oeste": (640, 330, 460, 320),
    "grande_leste": (820, 330, 460, 320),
    "reta_inteiro": (700, 420, 520, 280),
    "esquina_inteiro": (700, 420, 520, 280),
}


def magick(*args):
    return subprocess.run(["magick", *args], capture_output=True, text=True)


def recorte(origem, regiao, saida, clareia, escala, rotulo):
    x, y, w, h = regiao
    r = magick(origem, "-crop", f"{w}x{h}+{x}+{y}", "+repage", "-level", f"0%,{clareia}%", "-filter", "point", "-resize", f"{escala * 100}%",
               "-gravity", "NorthWest", "-fill", "white", "-undercolor", "#000000b0", "-pointsize", "22", "-annotate", "+6+6", rotulo, saida)
    if r.returncode:
        sys.exit(r.stderr)


def diferentes(a, b, regiao, tolerancia):
    x, y, w, h = regiao
    pa, pb = "/tmp/.cf_a.png", "/tmp/.cf_b.png"
    magick(a, "-crop", f"{w}x{h}+{x}+{y}", "+repage", pa)
    magick(b, "-crop", f"{w}x{h}+{x}+{y}", "+repage", pb)
    r = subprocess.run(["magick", "compare", "-metric", "AE", "-fuzz", f"{tolerancia}%", pa, pb, "null:"], capture_output=True, text=True)
    os.remove(pa)
    os.remove(pb)
    return int(float(r.stderr.split()[0])), w * h


def mascara(com, sem, regiao, saida, limiar):
    """O contorno de um jogo: os pixels que mudam entre o quadro com o contorno e o mesmo quadro sem ele (a mira, no meio, fica de fora). Escreve a máscara (branco = contorno) e devolve quantos pixels."""
    x, y, w, h = regiao
    mira = (960 - x, 540 - y)
    r = magick(com, sem, "-compose", "difference", "-composite", "-crop", f"{w}x{h}+{x}+{y}", "+repage", "-colorspace", "Gray", "-threshold", f"{limiar}%",
               "-fill", "black", "-draw", f"rectangle {mira[0] - 26},{mira[1] - 26} {mira[0] + 26},{mira[1] + 26}", saida)
    if r.returncode:
        sys.exit(r.stderr)
    return conta(saida)


def conta(png):
    r = magick(png, "-format", "%[fx:round(mean*w*h)]", "info:")
    return int(float(r.stdout))


def contorno_so(pose, regiao, port_com, port_sem, orig_com, orig_sem, saida, limiar):
    """O contorno do port e o do original, comparados: pixels de cada um, em comum, e os que o outro não tem nem a 1 pixel de distância (o resíduo de uma ou duas linhas do renderer do Horizon, H204,
    fica dentro dessa folga)."""
    mp, mo = f"{saida}/.{pose}_mp.png", f"{saida}/.{pose}_mo.png"
    n_port = mascara(port_com, port_sem, regiao, mp, limiar)
    n_orig = mascara(orig_com, orig_sem, regiao, mo, limiar)
    out = {"port": n_port, "original": n_orig}
    for nome, a, b in (("so_port_a_mais_de_1px", mp, mo), ("so_original_a_mais_de_1px", mo, mp)):
        tmp = f"{saida}/.{pose}_t.png"
        magick(a, "(", b, "-morphology", "Dilate", "Square:1", "-negate", ")", "-compose", "Multiply", "-composite", tmp)
        out[nome] = conta(tmp)
        os.remove(tmp)
    tmp = f"{saida}/.{pose}_c.png"
    magick(mp, mo, "-compose", "Multiply", "-composite", tmp)
    out["em_comum_exato"] = conta(tmp)
    os.remove(tmp)
    # A imagem: o contorno do port em vermelho, o do original em azul, o que coincide em branco.
    magick(mo, "-fill", "blue", "-opaque", "white", mp, "-fill", "red", "-opaque", "white", "-compose", "Screen", "-composite", "-filter", "point", "-resize", "200%", f"{saida}/{pose}_contorno.png")
    os.remove(mp)
    os.remove(mo)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("port")
    ap.add_argument("original")
    ap.add_argument("saida")
    ap.add_argument("--tolerancia", type=float, default=6)
    ap.add_argument("--clareia", type=int, default=100)
    ap.add_argument("--escala", type=int, default=2)
    ap.add_argument("--limiar", type=float, default=2, help="limiar da diferença com e sem o contorno, em porcento do cinza")
    args = ap.parse_args()
    os.makedirs(args.saida, exist_ok=True)
    for pose, regiao in REGIOES.items():
        p = os.path.join(args.port, f"{pose}.png")
        o = os.path.join(args.original, f"forma_oficial_{pose}.png")
        if not (os.path.exists(p) and os.path.exists(o)):
            print(f"{pose}: falta {'o port' if not os.path.exists(p) else 'o original'}")
            continue
        n0, total = diferentes(p, o, regiao, 0)
        n, _ = diferentes(p, o, regiao, args.tolerancia)
        cima, baixo = f"{args.saida}/.{pose}_p.png", f"{args.saida}/.{pose}_o.png"
        recorte(p, regiao, cima, args.clareia, args.escala, f"port: {pose}")
        recorte(o, regiao, baixo, args.clareia, args.escala, f"original: {pose}")
        magick(cima, "(", "-size", f"%[w]x6", "xc:gray50", ")", baixo, "-append", f"{args.saida}/{pose}_empilhado.png")
        os.remove(cima)
        os.remove(baixo)
        print(f"{pose}: {n} pixels diferentes com tolerância {args.tolerancia}% ({n0} sem tolerância) de {total}")
        psem, osem = os.path.join(args.port, f"{pose}_sem.png"), os.path.join(args.original, f"forma_oficial_{pose}_sem.png")
        if os.path.exists(psem) and os.path.exists(osem):
            print(f"{pose}: contorno", contorno_so(pose, regiao, p, psem, o, osem, args.saida, args.limiar))


if __name__ == "__main__":
    main()
