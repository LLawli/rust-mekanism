#!/usr/bin/env python3
"""Compara a forma de onda do som das máquinas gravada nos dois jogos (WAV de 48 kHz, do sink nulo): o original (`oficial_som_audio.sh`) e o port (`horizon2/h2_som_audio.py`).

Cada gravação tem um trecho de som por cenário (a máquina acesa até a energia acabar); os trechos saem do envelope de 20 ms (acima de um limiar, com os vãos de até 0,5 s juntados e
os menores que 2 s descartados). O que se compara por cenário, depois da subida (os primeiros 0,5 s ficam de fora): o RMS (do canal médio, na escala de 16 bits), o pico, e a correlação
normalizada máxima entre os primeiros 2 s dos dois (a 6 kHz, com atraso de até 100 ms): 1 é a mesma forma de onda. A razão de cada volume contra o padrão sai nos dois jogos e entre eles.

Uso: som_onda.py ORIGINAL.wav PORT.wav [SAIDA.png] [nome ...]   (os nomes dos cenários, na ordem das gravações: padrao base_0_5 base_2 silencio_depois)
"""
import array, struct, subprocess, sys

JANELA = 960  # 20 ms a 48 kHz
LIMIAR = 0.002  # fração do fundo de escala
VAO = 25  # janelas de 20 ms: 0,5 s
MINIMO = 100  # 2 s
SUBIDA = 25  # 0,5 s
DECIMAR = 8


def ler(caminho):
    """(taxa, canal médio em [-1, 1]) de um WAV PCM de 16 ou 32 bits ou float de 32."""
    dados = open(caminho, "rb").read()
    assert dados[:4] == b"RIFF" and dados[8:12] == b"WAVE", caminho
    pos = 12
    fmt = None
    while pos + 8 <= len(dados):
        tag, tamanho = dados[pos:pos + 4], struct.unpack("<I", dados[pos + 4:pos + 8])[0]
        corpo = dados[pos + 8:pos + 8 + tamanho]
        if tag == b"fmt ":
            fmt = struct.unpack("<HHIIHH", corpo[:16])
        elif tag == b"data":
            if tamanho == 0xFFFFFFFF or pos + 8 + tamanho > len(dados):
                corpo = dados[pos + 8:]
            break
        pos += 8 + tamanho + (tamanho & 1)
    formato, canais, taxa, _, _, bits = fmt
    if formato == 3 and bits == 32:
        amostras = array.array("f")
        escala = 1.0
    elif formato in (1, 0xFFFE) and bits == 16:
        amostras = array.array("h")
        escala = 32768.0
    elif formato in (1, 0xFFFE) and bits == 32:
        amostras = array.array("i")
        escala = 2147483648.0
    else:
        raise SystemExit(f"formato de WAV não suportado: {fmt}")
    corpo = corpo[:len(corpo) - len(corpo) % (amostras.itemsize * canais)]
    amostras.frombytes(corpo)
    medio = [sum(amostras[i:i + canais]) / (canais * escala) for i in range(0, len(amostras), canais)]
    return taxa, medio


def rms(trecho):
    return (sum(v * v for v in trecho) / len(trecho)) ** 0.5 if trecho else 0.0


def segmentos(sinal):
    """[(início, fim)] em amostras dos trechos com som."""
    envelope = [rms(sinal[i:i + JANELA]) for i in range(0, len(sinal) - JANELA + 1, JANELA)]
    ativos = [e > LIMIAR for e in envelope]
    trechos, inicio, ultimo = [], None, None
    for i, a in enumerate(ativos):
        if a:
            if inicio is None:
                inicio = i
            ultimo = i
        elif inicio is not None and i - ultimo > VAO:
            trechos.append((inicio, ultimo + 1))
            inicio = None
    if inicio is not None:
        trechos.append((inicio, ultimo + 1))
    return [(a * JANELA, b * JANELA) for a, b in trechos if b - a >= MINIMO]


def correlacao(a, b):
    """Correlação normalizada máxima, com atraso de até 100 ms, dos primeiros 2 s dos dois, a 6 kHz."""
    a = [sum(a[i:i + DECIMAR]) / DECIMAR for i in range(0, 96000 - DECIMAR, DECIMAR)]
    b = [sum(b[i:i + DECIMAR]) / DECIMAR for i in range(0, 96000 - DECIMAR, DECIMAR)]
    ma, mb = sum(a) / len(a), sum(b) / len(b)
    a, b = [v - ma for v in a], [v - mb for v in b]
    melhor = (-2.0, 0)
    for atraso in range(-600, 601, 2):
        if atraso >= 0:
            x, y = a[atraso:], b[:len(b) - atraso]
        else:
            x, y = a[:len(a) + atraso], b[-atraso:]
        num = sum(p * q for p, q in zip(x, y))
        den = (sum(p * p for p in x) * sum(q * q for q in y)) ** 0.5
        if den and num / den > melhor[0]:
            melhor = (num / den, atraso)
    return melhor


def envelope(sinal, trecho):
    a, b = trecho
    return [rms(sinal[i:i + JANELA]) for i in range(a + SUBIDA * JANELA, b - JANELA, JANELA)]


def correlacao_do_envelope(a, b):
    """Correlação normalizada máxima entre os envelopes de RMS (20 ms) do regime dos dois trechos, com atraso de até 1 s: a forma de onda de cada jogo parte de uma fase própria do
    laço e do relógio de áudio, então o que se compara com sentido é a energia ao longo do tempo."""
    n = min(len(a), len(b)) - 50
    melhor = (-2.0, 0)
    for atraso in range(-50, 51):
        x = a[max(0, atraso):][:n]
        y = b[max(0, -atraso):][:n]
        mx, my = sum(x) / len(x), sum(y) / len(y)
        num = sum((p - mx) * (q - my) for p, q in zip(x, y))
        den = (sum((p - mx) ** 2 for p in x) * sum((q - my) ** 2 for q in y)) ** 0.5
        if den and num / den > melhor[0]:
            melhor = (num / den, atraso)
    return melhor


def resumo(sinal, trecho):
    a, b = trecho
    corpo = sinal[a + SUBIDA * JANELA:b]
    return {"inicio_s": a / 48000, "duracao_s": (b - a) / 48000, "rms": rms(corpo) * 32768, "pico": max(abs(v) for v in corpo) * 32768}


def desenhar(caminho, nomes, originais, portas, sinais):
    """Para cada cenário: a forma de onda de 30 ms em regime (original e port juntos) e o envelope de RMS do trecho inteiro."""
    larg, alt, linha, esq = 1100, 230, 40, 40
    corpo = []
    for k, nome in enumerate(nomes):
        y0 = k * alt + linha
        (so, sp) = sinais[k]
        corpo.append(f'<text x="{esq}" y="{y0 - 14}" font-size="14" font-family="sans-serif">{nome}: original RMS {originais[k]["rms"]:.1f} (azul), port {portas[k]["rms"]:.1f} (laranja)</text>')
        for lado, (x0, x1, titulo) in enumerate(((esq, 520, "30 ms em regime"), (600, 1060, "envelope de RMS (20 ms) do trecho"))):
            corpo.append(f'<rect x="{x0}" y="{y0}" width="{x1 - x0}" height="{alt - linha - 30}" fill="none" stroke="#999"/>')
            corpo.append(f'<text x="{x0}" y="{y0 + alt - linha - 12}" font-size="11" font-family="sans-serif">{titulo}</text>')
            for cor, (sinal, trecho) in (("#1f77b4", so), ("#ff7f0e", sp)):
                a, b = trecho
                if lado == 0:
                    inicio = a + 2 * 48000
                    dados = sinal[inicio:inicio + 1440]
                else:
                    dados = [rms(sinal[i:i + JANELA]) for i in range(a, b - JANELA, JANELA)]
                pontos = []
                n = len(dados)
                maximo = ESCALA[(k, lado)]
                for i, v in enumerate(dados):
                    x = x0 + (x1 - x0) * i / max(n - 1, 1)
                    meio = y0 + (alt - linha - 30) / 2
                    y = meio - v / maximo * (alt - linha - 30) / 2 * 0.95 if lado == 0 else y0 + (alt - linha - 30) - v / maximo * (alt - linha - 30) * 0.95
                    pontos.append(f"{x:.1f},{y:.1f}")
                corpo.append(f'<polyline points="{" ".join(pontos)}" fill="none" stroke="{cor}" stroke-width="1"/>')
    svg = f'<svg xmlns="http://www.w3.org/2000/svg" width="{larg}" height="{len(nomes) * alt + 10}"><rect width="100%" height="100%" fill="white"/>' + "".join(corpo) + "</svg>"
    caminho_svg = caminho.rsplit(".", 1)[0] + ".svg"
    open(caminho_svg, "w", encoding="utf-8").write(svg)
    subprocess.run(["rsvg-convert", "-o", caminho, caminho_svg], check=True)


ESCALA = {}


def main():
    original, port = sys.argv[1], sys.argv[2]
    saida = sys.argv[3] if len(sys.argv) > 3 and sys.argv[3].endswith(".png") else None
    nomes = [a for a in sys.argv[3:] if not a.endswith(".png")] or ["padrao", "base_0_5", "base_2", "silencio_depois"]
    taxa_o, so = ler(original)
    taxa_p, sp = ler(port)
    assert taxa_o == taxa_p == 48000, (taxa_o, taxa_p)
    to, tp = segmentos(so), segmentos(sp)
    print(f"original: {len(so) / 48000:.1f} s, {len(to)} trechos; port: {len(sp) / 48000:.1f} s, {len(tp)} trechos")
    if len(to) < len(nomes) or len(tp) < len(nomes):
        print("faltam trechos:", [(a / 48000, b / 48000) for a, b in to], [(a / 48000, b / 48000) for a, b in tp])
        sys.exit(1)
    # O cenário `silencio_depois` (o silêncio entra no meio) tem trecho menor: só os três primeiros entram na razão de volume.
    ro = [resumo(so, t) for t in to[:len(nomes)]]
    rp = [resumo(sp, t) for t in tp[:len(nomes)]]
    print(f"{'cenario':16} {'jogo':9} {'inicio':>7} {'duracao':>8} {'RMS':>9} {'pico':>8} {'razao_rms':>10}")
    for k, nome in enumerate(nomes):
        for jogo, r in (("original", ro[k]), ("port", rp[k])):
            razao = r["rms"] / (ro if jogo == "original" else rp)[0]["rms"] if (ro if jogo == "original" else rp)[0]["rms"] else float("nan")
            print(f"{nome:16} {jogo:9} {r['inicio_s']:7.1f} {r['duracao_s']:8.1f} {r['rms']:9.1f} {r['pico']:8.0f} {razao:10.3f}")
    print()
    print(f"{'cenario':16} {'RMS port/original':>18} {'forma de onda (atraso ms)':>26} {'envelope (atraso ms)':>22}")
    for k, nome in enumerate(nomes):
        a = so[to[k][0] + SUBIDA * JANELA:][:96000]
        b = sp[tp[k][0] + SUBIDA * JANELA:][:96000]
        c, atraso = correlacao(a, b)
        ce, ae = correlacao_do_envelope(envelope(so, to[k]), envelope(sp, tp[k]))
        print(f"{nome:16} {rp[k]['rms'] / ro[k]['rms']:18.4f} {c:16.4f} ({atraso * DECIMAR / 48:+6.1f}) {ce:14.4f} ({ae * 20:+5d})")
    if saida:
        sinais = [((so, to[k]), (sp, tp[k])) for k in range(len(nomes))]
        for k in range(len(nomes)):
            corpo_o = so[to[k][0] + 2 * 48000:][:1440]
            corpo_p = sp[tp[k][0] + 2 * 48000:][:1440]
            ESCALA[(k, 0)] = max(max(abs(v) for v in corpo_o), max(abs(v) for v in corpo_p), 1e-9)
            env = [rms(s[i:i + JANELA]) for s, t in ((so, to[k]), (sp, tp[k])) for i in range(t[0], t[1] - JANELA, JANELA)]
            ESCALA[(k, 1)] = max(env + [1e-9])
        desenhar(saida, nomes, ro, rp, sinais)
        print("gráfico:", saida)


if __name__ == "__main__":
    main()
