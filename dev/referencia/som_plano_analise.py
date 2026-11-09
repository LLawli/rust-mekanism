#!/usr/bin/env python3
"""Analisa as gravações do som da Enrichment Chamber em condições controladas (frente `som-plano`): várias partidas de cada jogo, cada uma com os trechos de `cenas/som_plano.txt`
(`oficial_som_plano.sh` no original, `horizon2/h2_som_plano.py` no port). Por cenário e por repetição: o RMS do regime (WAV de 48 kHz, canal médio, escala de 16 bits), a média e o desvio
de cada jogo, a razão port/original com o intervalo de 95 % (Welch), as razões contra o cenário padrão, o envelope de RMS e o ganho esperado pela atenuação linear.

Métricas de RMS (todas contadas a partir de 0,5 s depois do começo do som, que fica de fora da subida):
  laco2    duas voltas inteiras do laço (2 x 4,0749 s): o RMS de um sinal periódico em voltas inteiras não depende da fase em que a janela começa
  janela   de 0,5 s a 3,5 s depois do começo (a mesma janela em todos os cenários; é a que vale no `silencio_depois`, onde o som cai de 4 a 6 s depois do começo)
  corpo    de 0,5 s até o fim do trecho (a medida da gravação de antes)
  blocos   o RMS de seis blocos de 0,5 s da janela: o mesmo bloco nos dois jogos tem o mesmo conteúdo do laço, e a razão por bloco não depende do conteúdo

Uso: som_plano_analise.py --cena cenas/som_plano.txt --original o1.wav o2.wav ... --port p1.wav p2.wav ... [--logs-original o1.jsonl ...] [--distancia-port D] [--png SAIDA.png]
"""
import argparse, json, math, os, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import som_onda as s

TAXA = 48000
LACO_S = 179704 / 44100  # o .ogg de `enrichment_chamber`: 179.704 amostras a 44,1 kHz
SUBIDA = int(0.5 * TAXA)
JANELA_S = 3.0
BLOCO = TAXA // 2
# Quantis t de 97,5 % (bilateral de 95 %) para 1 a 30 graus de liberdade.
T975 = [12.706, 4.303, 3.182, 2.776, 2.571, 2.447, 2.365, 2.306, 2.262, 2.228, 2.201, 2.179, 2.160, 2.145, 2.131, 2.120, 2.110, 2.101, 2.093, 2.086, 2.080, 2.074, 2.069, 2.064, 2.060, 2.056, 2.052, 2.048, 2.045, 2.042]
# OpenAL Soft (1.25.2) e o mixer do Horizon, ouvinte de frente para uma fonte centrada: 0,5 + 0,0957 em cada canal (`stereo_gains`, `audio.rs`).
GANHO_CENTRADO = 0.5 + 0.0956608
RMS_MODELO = 869.70  # o RMS do .ogg decodificado, escala de 16 bits


def t975(df):
    return T975[max(1, min(30, int(round(df)))) - 1] if df < 30 else 1.96


def media_desvio(v):
    m = sum(v) / len(v)
    return m, (math.sqrt(sum((x - m) ** 2 for x in v) / (len(v) - 1)) if len(v) > 1 else 0.0)


def nomes_da_cena(caminho):
    nomes = []
    with open(caminho, encoding="utf-8") as arquivo:
        for linha in arquivo:
            linha = linha.strip()
            if not linha or linha.startswith("#") or linha.startswith("mundo "):
                continue
            for palavra in linha.split():
                if palavra.startswith("nome="):
                    nomes.append(palavra[5:])
    return nomes


def inicio_fino(x, a):
    """O começo do som em amostras: a primeira janela de 5 ms, perto do começo grosso `a` (janelas de 20 ms), acima de 25 % do RMS do trecho seguinte."""
    ref = s.rms(x[a + SUBIDA:a + SUBIDA + TAXA])
    for i in range(max(0, a - 1920), a + 2400, 48):
        if s.rms(x[i:i + 240]) > 0.25 * ref:
            return i
    return a


def medidas(x, trecho):
    a, b = trecho
    i0 = inicio_fino(x, a)
    corpo0 = i0 + SUBIDA
    laco = int(round(2 * LACO_S * TAXA))
    jan = int(JANELA_S * TAXA)
    esc = 32768.0
    m = {"inicio_s": i0 / TAXA, "duracao_s": (b - i0) / TAXA}
    m["corpo"] = s.rms(x[corpo0:b]) * esc
    m["janela"] = s.rms(x[corpo0:corpo0 + jan]) * esc
    m["laco2"] = s.rms(x[corpo0:corpo0 + laco]) * esc if corpo0 + laco <= b else None
    m["blocos"] = [s.rms(x[corpo0 + k * BLOCO:corpo0 + (k + 1) * BLOCO]) * esc for k in range(int(JANELA_S * 2))]
    m["envelope"] = [s.rms(x[i:i + s.JANELA]) * esc for i in range(corpo0, min(b, corpo0 + 8 * TAXA) - s.JANELA, s.JANELA)]
    return m


def carregar(arquivos, nomes):
    """[(arquivo, repetição, cenário, medidas)] de cada gravação, com os trechos na ordem da cena."""
    saida = []
    for arq in arquivos:
        taxa, x = s.ler(arq)
        assert taxa == TAXA, (arq, taxa)
        trechos = s.segmentos(x)
        print(f"{os.path.basename(arq)}: {len(x) / TAXA:.1f} s, {len(trechos)} trechos (esperados {len(nomes)})", flush=True)
        if len(trechos) != len(nomes):
            print("  trechos:", [(round(a / TAXA, 1), round(b / TAXA, 1)) for a, b in trechos])
            raise SystemExit(f"{arq}: número de trechos diferente do da cena")
        for k, (nome, trecho) in enumerate(zip(nomes, trechos)):
            saida.append((os.path.basename(arq), k // 4 if len(nomes) > 4 else 0, nome, medidas(x, trecho)))
    return saida


def valores(dados, cenario, chave):
    return [m[chave] for _, _, n, m in dados if n == cenario and m[chave] is not None]


def razao(po, pp):
    """Razão port/original das médias, com o erro-padrão por propagação e o intervalo de 95 % (Welch)."""
    (mo, so), (mp, sp) = media_desvio(po), media_desvio(pp)
    r = mp / mo
    vo, vp = so * so / len(po), sp * sp / len(pp)
    se = r * math.sqrt(vp / (mp * mp) + vo / (mo * mo))
    df = (vo + vp) ** 2 / ((vo * vo) / (len(po) - 1) + (vp * vp) / (len(pp) - 1)) if vo + vp > 0 else len(po) + len(pp) - 2
    return r, t975(df) * se, df


def ganho_esperado(volume, distancia, atenuacao=16.0):
    """`calculateVolume` (volume limitado a 1) vezes a atenuação linear do OpenAL (`SoundEngine.play`: `max(volume, 1) * atenuacao`)."""
    maximo = max(volume, 1.0) * atenuacao
    return min(max(volume, 0.0), 1.0) * max(0.0, 1.0 - min(distancia, maximo) / maximo)


def distancias(logs):
    """A distância do ouvinte à máquina (a mediana dos ticks com o som tocando) e a altura dos olhos, de cada log `.jsonl` do original."""
    saida = []
    for caminho in logs:
        d, olhos = [], []
        with open(caminho, encoding="utf-8") as arquivo:
            for linha in arquivo:
                r = json.loads(linha)
                if r.get("e") == "tick" and r.get("som") and r["som"].get("ativo"):
                    d.append(r["distancia"])
                    olhos.append(r["olhos"][1])
        d.sort()
        saida.append((d[len(d) // 2] if d else None, olhos[len(olhos) // 2] if olhos else None))
    return saida


def tabela(orig, port, nomes_unicos, distancia_o, distancia_p, volumes):
    linhas = []
    for chave, rotulo in (("laco2", "RMS de 2 voltas do laço"), ("janela", "RMS da janela 0,5 a 3,5 s"), ("corpo", "RMS do corpo (0,5 s ao fim)")):
        linhas.append(f"\n{rotulo} (escala de 16 bits)")
        linhas.append(f"{'cenário':16} {'n_o':>3} {'original média':>15} {'desvio':>7} {'cv%':>6} {'n_p':>3} {'port média':>11} {'desvio':>7} {'cv%':>6} {'port/orig':>9} {'dif%':>7} {'±95%':>7}  {'dentro da variação':>18}")
        for nome in nomes_unicos:
            po, pp = valores(orig, nome, chave), valores(port, nome, chave)
            if len(po) < 2 or len(pp) < 2:
                continue
            (mo, so), (mp, sp) = media_desvio(po), media_desvio(pp)
            r, ic, _ = razao(po, pp)
            dentro = "sim" if abs(r - 1) <= ic else "não"
            linhas.append(f"{nome:16} {len(po):3d} {mo:15.3f} {so:7.3f} {100 * so / mo:6.3f} {len(pp):3d} {mp:11.3f} {sp:7.3f} {100 * sp / mp:6.3f} {r:9.5f} {100 * (r - 1):7.3f} {ic:7.5f}  {dentro:>18}")
    linhas.append("\nrazão contra o cenário padrão (laco2; média de cada jogo)")
    linhas.append(f"{'cenário':16} {'original':>9} {'port':>9} {'esperado orig':>14} {'esperado port':>14}")
    base_o, base_p = media_desvio(valores(orig, "padrao", "laco2"))[0], media_desvio(valores(port, "padrao", "laco2"))[0]
    for nome in nomes_unicos:
        if nome == "silencio_depois":
            continue
        mo, mp = media_desvio(valores(orig, nome, "laco2"))[0], media_desvio(valores(port, nome, "laco2"))[0]
        eo = ganho_esperado(volumes[nome], distancia_o) / ganho_esperado(1.0, distancia_o)
        ep = ganho_esperado(volumes[nome], distancia_p) / ganho_esperado(1.0, distancia_p)
        linhas.append(f"{nome:16} {mo / base_o:9.4f} {mp / base_p:9.4f} {eo:14.4f} {ep:14.4f}")
    linhas.append("\nrazão port/original por bloco de 0,5 s (média das repetições; o mesmo bloco do laço nos dois jogos)")
    for nome in nomes_unicos:
        bo = [valores(orig, nome, "blocos")]
        po = [m["blocos"] for _, _, n, m in orig if n == nome]
        pp = [m["blocos"] for _, _, n, m in port if n == nome]
        if not po or not pp:
            continue
        razoes = [(sum(v[k] for v in pp) / len(pp)) / (sum(v[k] for v in po) / len(po)) for k in range(len(po[0]))]
        linhas.append(f"{nome:16} " + " ".join(f"{r:7.4f}" for r in razoes) + f"   média {sum(razoes) / len(razoes):.4f}")
    linhas.append("\nduração do trecho (s): o `silencio_depois` cai a 4 a 6 s depois do começo (o intervalo de 20 a 39 ticks é sorteado por instância nos dois jogos)")
    for nome in nomes_unicos:
        do, dp = valores(orig, nome, "duracao_s"), valores(port, nome, "duracao_s")
        linhas.append(f"{nome:16} original {sum(do) / len(do):6.2f} (min {min(do):.2f}, máx {max(do):.2f})   port {sum(dp) / len(dp):6.2f} (min {min(dp):.2f}, máx {max(dp):.2f})")
    return "\n".join(linhas)


def envelopes(orig, port, nomes_unicos):
    linhas = ["\nenvelope de RMS de 20 ms (média das repetições alinhadas no começo do som)"]
    medias = {}
    for nome in nomes_unicos:
        if nome == "silencio_depois":
            continue
        for jogo, dados in (("original", orig), ("port", port)):
            env = [m["envelope"] for _, _, n, m in dados if n == nome]
            n = min(len(e) for e in env)
            medias[(nome, jogo)] = [sum(e[i] for e in env) / len(env) for i in range(n)]
        eo, ep = medias[(nome, "original")], medias[(nome, "port")]
        n = min(len(eo), len(ep))
        c, atraso = s.correlacao_do_envelope(eo[:n], ep[:n])
        c0, _ = s.correlacao_do_envelope(eo[:n], eo[:n])
        linhas.append(f"{nome:16} correlação {c:.4f} (atraso {atraso * 20:+d} ms); razão das médias do envelope {sum(ep[:n]) / sum(eo[:n]):.4f}")
    return "\n".join(linhas), medias


def grafico(caminho, orig, port, nomes_unicos, medias):
    """Por cenário: o RMS de cada repetição dos dois jogos (pontos) com a média e o desvio, num eixo de mais ou menos 2 % da média do original, com a marca da diferença de antes (+1,6 %); e o envelope."""
    larg, alt, esq = 1100, 262, 70
    corpo = []
    cor = {"original": "#1f77b4", "port": "#ff7f0e"}
    for k, nome in enumerate(nomes_unicos):
        y0 = 116 + k * alt
        chave = "laco2" if nome != "silencio_depois" else "janela"
        po, pp = valores(orig, nome, chave), valores(port, nome, chave)
        mo = sum(po) / len(po)
        lo, hi = mo * 0.98, mo * 1.02
        h = alt - 100
        corpo.append(f'<text x="{esq}" y="{y0 - 34}" font-size="15" font-family="sans-serif" font-weight="bold">{nome}</text>')
        corpo.append(f'<text x="{esq}" y="{y0 - 14}" font-size="12" font-family="sans-serif">RMS por repetição (pontos), média e desvio (barra); eixo: média do original, mais e menos 2 %</text>')
        corpo.append(f'<rect x="{esq}" y="{y0}" width="440" height="{h}" fill="none" stroke="#999"/>')

        def ey(v):
            return y0 + h - (v - lo) / (hi - lo) * h
        corpo.append(f'<line x1="{esq}" y1="{ey(mo * 1.016):.1f}" x2="{esq + 440}" y2="{ey(mo * 1.016):.1f}" stroke="#c0392b" stroke-dasharray="5,4"/>')
        corpo.append(f'<text x="{esq + 436}" y="{ey(mo * 1.016) - 4:.1f}" font-size="11" font-family="sans-serif" fill="#c0392b" text-anchor="end">+1,6 %: o port da gravação de antes (0,5 a 1,7 %)</text>')
        corpo.append(f'<line x1="{esq}" y1="{ey(mo):.1f}" x2="{esq + 440}" y2="{ey(mo):.1f}" stroke="#bbb"/>')
        for jogo, vs, x0 in (("original", po, esq + 30), ("port", pp, esq + 250)):
            m, d = media_desvio(vs)
            for i, v in enumerate(vs):
                corpo.append(f'<circle cx="{x0 + 30 * i:.1f}" cy="{ey(v):.1f}" r="4" fill="{cor[jogo]}" fill-opacity="0.8"/>')
            corpo.append(f'<line x1="{x0 - 8}" y1="{ey(m):.1f}" x2="{x0 + 30 * (len(vs) - 1) + 8}" y2="{ey(m):.1f}" stroke="{cor[jogo]}" stroke-width="2"/>')
            corpo.append(f'<text x="{x0 - 8}" y="{y0 + h + 15}" font-size="12" font-family="sans-serif" fill="{cor[jogo]}">{jogo} {m:.2f} ± {d:.3f} (n={len(vs)})</text>')
        corpo.append(f'<text x="{esq - 6}" y="{y0 + 8}" font-size="10" font-family="sans-serif" text-anchor="end">{hi:.1f}</text>')
        corpo.append(f'<text x="{esq - 6}" y="{y0 + h}" font-size="10" font-family="sans-serif" text-anchor="end">{lo:.1f}</text>')
        if nome != "silencio_depois":
            x0, x1 = 580, 1060
            corpo.append(f'<rect x="{x0}" y="{y0}" width="{x1 - x0}" height="{h}" fill="none" stroke="#999"/>')
            corpo.append(f'<text x="{x0}" y="{y0 + h + 15}" font-size="12" font-family="sans-serif">envelope de RMS de 20 ms (média das repetições), 0,5 s a 8 s</text>')
            maximo = max(max(medias[(nome, "original")]), max(medias[(nome, "port")]))
            for jogo in ("original", "port"):
                dados = medias[(nome, jogo)]
                pts = " ".join(f"{x0 + (x1 - x0) * i / (len(dados) - 1):.1f},{y0 + h - v / maximo * h * 0.95:.1f}" for i, v in enumerate(dados))
                corpo.append(f'<polyline points="{pts}" fill="none" stroke="{cor[jogo]}" stroke-width="1.2"/>')
    cabecalho = '<text x="70" y="30" font-size="18" font-family="sans-serif" font-weight="bold">Som da Enrichment Chamber em condições controladas</text><text x="70" y="52" font-size="13" font-family="sans-serif">original (azul) e port (laranja); mesmo mundo superplano, mesma semente, máquina por fill, jogador por tp</text>'
    svg = f'<svg xmlns="http://www.w3.org/2000/svg" width="{larg}" height="{len(nomes_unicos) * alt + 130}"><rect width="100%" height="100%" fill="white"/>{cabecalho}' + "".join(corpo) + "</svg>"
    svg_caminho = caminho.rsplit(".", 1)[0] + ".svg"
    with open(svg_caminho, "w", encoding="utf-8") as arquivo:
        arquivo.write(svg)
    import subprocess
    subprocess.run(["rsvg-convert", "-o", caminho, svg_caminho], check=True)


BANDAS = [(0, 500), (500, 1000), (1000, 2000), (2000, 4000), (4000, 8000), (8000, 12000), (12000, 16000), (16000, 20000)]


def rms_da_banda(wav, inicio, duracao, baixo, alto):
    """O RMS (escala de 16 bits) de um trecho do WAV filtrado entre `baixo` e `alto` Hz (dois passa-altas e dois passa-baixas do ffmpeg, o `astats` lê o nível)."""
    import re, subprocess
    filtros = []
    if baixo:
        filtros += [f"highpass=f={baixo}"] * 2
    if alto:
        filtros += [f"lowpass=f={alto}"] * 2
    filtros.append("astats=metadata=0:reset=0")
    saida = subprocess.run(["ffmpeg", "-v", "info", "-nostats", "-ss", str(inicio), "-t", str(duracao), "-i", wav, "-af", ",".join(filtros), "-f", "null", "-"], capture_output=True, text=True).stderr
    niveis = re.findall(r"RMS level dB:\s*(-?[\d.]+)", saida)
    return 10 ** (float(niveis[-1]) / 20) * 32768


def bandas(wav_original, inicio_original, wav_port, inicio_port):
    """A razão port/original do RMS por faixa de frequência, no mesmo trecho (as duas voltas do laço do primeiro `padrao` de cada gravação): onde a diferença mora."""
    linhas = ["\nrazão port/original por faixa de frequência (primeiro `padrao`, duas voltas do laço; ffmpeg)", f"{'faixa (Hz)':>14} {'original':>10} {'port':>10} {'port/orig':>10}"]
    dur = 2 * LACO_S
    for baixo, alto in BANDAS:
        o = rms_da_banda(wav_original, inicio_original + 0.5, dur, baixo, alto)
        p = rms_da_banda(wav_port, inicio_port + 0.5, dur, baixo, alto)
        linhas.append(f"{baixo:6d} a {alto:5d} {o:10.3f} {p:10.3f} {p / o:10.4f}")
    return "\n".join(linhas)


def reamostragem(ogg):
    """O mesmo `.ogg` (44,1 kHz) levado a 48 kHz por um reamostrador exato (ffmpeg), pela interpolação linear e pela cúbica (Catmull-Rom), duas voltas: a razão exato/reamostrado por faixa."""
    import array, subprocess, tempfile
    with tempfile.TemporaryDirectory() as pasta:
        bruto = os.path.join(pasta, "t.f32")
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", ogg, "-f", "f32le", "-ac", "1", "-ar", "44100", bruto], check=True)
        a = array.array("f")
        with open(bruto, "rb") as f:
            a.frombytes(f.read())
        laco = array.array("f", a) + a
        passo = 44100 / 48000
        arquivos = {"exato": (laco, 44100)}
        for nome in ("linear", "cubico"):
            saida, pos = array.array("f"), 1.0
            while int(pos) + 2 < len(laco):
                i, t = int(pos), pos - int(pos)
                if nome == "linear":
                    saida.append(laco[i] * (1 - t) + laco[i + 1] * t)
                else:
                    p0, p1, p2, p3 = laco[i - 1], laco[i], laco[i + 1], laco[i + 2]
                    saida.append(0.5 * ((2 * p1) + (-p0 + p2) * t + (2 * p0 - 5 * p1 + 4 * p2 - p3) * t * t + (-p0 + 3 * p1 - 3 * p2 + p3) * t ** 3))
                pos += passo
            arquivos[nome] = (saida, 48000)
        wavs = {}
        for nome, (dados, taxa) in arquivos.items():
            f32 = os.path.join(pasta, nome + ".f32")
            with open(f32, "wb") as f:
                f.write(dados.tobytes())
            wavs[nome] = os.path.join(pasta, nome + ".wav")
            subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "f32le", "-ar", str(taxa), "-ac", "1", "-i", f32, "-ar", "48000", "-ac", "2", wavs[nome]], check=True)
        linhas = ["\nsimulação no mesmo .ogg (44,1 para 48 kHz): razão do som exato contra o reamostrado, por faixa", f"{'faixa (Hz)':>14} {'exato/linear':>13} {'exato/cúbico':>13}"]
        for baixo, alto in BANDAS:
            e = rms_da_banda(wavs["exato"], 0.0, 8.0, baixo, alto)
            linhas.append(f"{baixo:6d} a {alto:5d} {e / rms_da_banda(wavs['linear'], 0.0, 8.0, baixo, alto):13.4f} {e / rms_da_banda(wavs['cubico'], 0.0, 8.0, baixo, alto):13.4f}")
        return "\n".join(linhas)


def lado(cena, original, port):
    """A variante com o jogador de lado (fonte a 90 graus, onde a panorâmica satura): uma gravação de cada jogo, razão port/original das mesmas medidas."""
    nomes = nomes_da_cena(cena)
    o, p = carregar(original, nomes), carregar(port, nomes)
    linhas = ["\njogador de lado (`cenas/som_plano_lado.txt`: pés a 3 blocos a leste, pitch 12; a média dos canais de uma fonte saturada vale 0,5 do ganho, sem depender da elevação)",
              f"{'cenário':16} {'original laco2':>15} {'port laco2':>11} {'port/orig':>9} {'original janela':>16} {'port janela':>12} {'port/orig':>9}"]
    for (_, _, nome, mo), (_, _, _, mp) in zip(o, p):
        lo = f"{mo['laco2']:15.3f} {mp['laco2']:11.3f} {mp['laco2'] / mo['laco2']:9.5f}" if mo["laco2"] else f"{'':15} {'':11} {'':9}"
        linhas.append(f"{nome:16} {lo} {mo['janela']:16.3f} {mp['janela']:12.3f} {mp['janela'] / mo['janela']:9.5f}")
    return "\n".join(linhas)


def tabela_png(caminho, texto):
    import subprocess
    linhas = texto.split("\n")
    larg = 1560
    svg = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{larg}" height="{len(linhas) * 17 + 30}"><rect width="100%" height="100%" fill="white"/>']
    for i, linha in enumerate(linhas):
        linha = linha.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
        svg.append(f'<text x="14" y="{24 + i * 17}" font-size="13" font-family="monospace" xml:space="preserve">{linha}</text>')
    svg.append("</svg>")
    svg_caminho = caminho.rsplit(".", 1)[0] + ".svg"
    open(svg_caminho, "w", encoding="utf-8").write("".join(svg))
    subprocess.run(["rsvg-convert", "-o", caminho, svg_caminho], check=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cena", required=True)
    ap.add_argument("--original", nargs="+", required=True)
    ap.add_argument("--port", nargs="+", required=True)
    ap.add_argument("--logs-original", nargs="*", default=[])
    ap.add_argument("--distancia-original", type=float)
    ap.add_argument("--distancia-port", type=float, required=True)
    ap.add_argument("--cena-lado")
    ap.add_argument("--lado-original", nargs="*", default=[])
    ap.add_argument("--lado-port", nargs="*", default=[])
    ap.add_argument("--ogg", help="o enrichment_chamber.ogg: liga as faixas de frequência e a simulação do reamostrador (precisa do ffmpeg)")
    ap.add_argument("--png")
    ap.add_argument("--tabela-png")
    a = ap.parse_args()
    nomes = nomes_da_cena(a.cena)
    unicos = list(dict.fromkeys(nomes))
    volumes = {"padrao": 1.0, "base_0_5": 0.5, "base_2": 2.0, "silencio_depois": 1.0}
    d_o = a.distancia_original
    if d_o is None:
        ds = [d for d, _ in distancias(a.logs_original) if d]
        d_o = sorted(ds)[len(ds) // 2]
    orig, port = carregar(a.original, nomes), carregar(a.port, nomes)
    texto = tabela(orig, port, unicos, d_o, a.distancia_port, volumes)
    env_texto, medias = envelopes(orig, port, unicos)
    cab = (f"distância ouvinte-máquina: original {d_o:.4f}, port {a.distancia_port:.4f} blocos; "
           f"ganho esperado do padrão (1 - d/16): original {ganho_esperado(1, d_o):.4f}, port {ganho_esperado(1, a.distancia_port):.4f}\n"
           f"RMS esperado do padrão sem a elevação (RMS do .ogg {RMS_MODELO} x 0,5957 x ganho): original {RMS_MODELO * GANHO_CENTRADO * ganho_esperado(1, d_o):.1f}, port {RMS_MODELO * GANHO_CENTRADO * ganho_esperado(1, a.distancia_port):.1f}; "
           f"com a elevação de -20,5 graus (0,5 + 0,0957 x cos) o modelo dá {RMS_MODELO * (0.5 + 0.0956608 * math.cos(math.asin(-1.12 / d_o))) * ganho_esperado(1, d_o):.1f}")
    saida = cab + "\n" + texto + "\n" + env_texto
    if a.cena_lado and a.lado_original and a.lado_port:
        saida += "\n" + lado(a.cena_lado, a.lado_original, a.lado_port)
    if a.ogg:
        saida += bandas(a.original[0], orig[0][3]["inicio_s"], a.port[0], port[0][3]["inicio_s"]) + "\n" + reamostragem(a.ogg)
    print("\n" + saida)
    if a.png:
        grafico(a.png, orig, port, unicos, medias)
    if a.tabela_png:
        tabela_png(a.tabela_png, saida)


if __name__ == "__main__":
    main()
