#!/usr/bin/env python3
"""O ar do bioma no Network Reader, no jogo contra o original (frente `ar-bioma`, Fase 3, decisão 184).

    H_NAME=horizon2 PASTA=<pasta do game.log> h2_ar_bioma.py casos [ORIGINAL]   # os casos de `cenas/ar_bioma.txt` no port, em mundo plano
    H_NAME=horizon2 PASTA=<pasta do game.log> h2_ar_bioma.py spike               # a cena antiga (condutores a y 150, bioma do mundo) com a sonda: o que o jogo leu
    NOME=... h2_ar_bioma.py video                                               # o vídeo com som do leitor no condutor

O original (`CenaLeitor`, `cenas/ar_bioma.txt`) põe a linha de três condutores termodinâmicos em x = 300, z = 0 de um mundo plano (nível do mar -63) a cada altura e bioma (`/fillbiome` em
volta), espera 300 tiques e clica no do meio com o Network Reader; o log dele tem uma linha `AR` (o `HeatAPI.getAmbientTemp(level, pos)` de cada posição que os condutores leem) e as linhas `CHAT`
(o texto). O port repete cada caso nas mesmas coordenadas, no mesmo bioma e na mesma altura, e compara o ar que a sonda `MBA` imprime (`TRANSMITTER_PROBE`, `enviar_fontes.sh`) e o texto do chat; o
chat do original e o do port saem recortados e empilhados (`comparacao_<caso>.png`).
"""
import json, os, re, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("H_NAME", "horizon")
import h2_ajustes_jogo as a
import h2_identity as h
import h2_network_reader as nr

cmd, wait = a.cmd, a.wait
HERE = os.path.dirname(os.path.abspath(__file__))
CENA = os.path.join(HERE, "..", "cenas", "ar_bioma.txt")
LOCAL = os.path.expanduser(os.environ.get("LOCAL", "~/.cache/rust-mekanism/wa/ar-bioma"))
ORIGINAL = os.path.expanduser(os.environ.get("ORIGINAL", "~/.cache/rust-mekanism/ar-bioma-orig"))
CONDUCTOR = "mekanism:basic_thermodynamic_conductor"
# Onde a cena do original põe a linha (`CenaLeitor`: X = 300, Z = 0, o chão de teste em y = -62 e o jogador a 1,4 ao norte).
X, Z, Y0 = 300, 0, -61


def casos():
    """Os passos de `cenas/ar_bioma.txt`: [(nome, bioma, y, ate)]."""
    out = []
    with open(CENA) as file:
        for line in file:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            campos = dict(word.split("=", 1) for word in line.split() if "=" in word)
            out.append((campos["nome"], campos["bioma"], int(campos["y"]), int(campos.get("ate", "30"))))
    return out


def original_log():
    """O que o original gravou: {nome: {"ar": {(x, y, z): ar}, "sea": n, "chat": [linhas]}}."""
    out = {}
    with open(os.path.join(ORIGINAL, "ar_log.txt")) as file:
        for line in file:
            m = re.search(r" AR (\{.*\})\s*$", line)
            if m:
                row = json.loads(m.group(1))
                out.setdefault(row["nome"], {"chat": []}).update(
                    {"sea": row["sea"], "ar": {tuple(p["at"]): p["ambient"] for p in row["pos"]}, "bioma": {tuple(p["at"]): p["biome"] for p in row["pos"]}}
                )
                continue
            m = re.search(r" CHAT (\w+): ?(.*)$", line)
            if m and m.group(1) in out:
                out[m.group(1)]["chat"].append(m.group(2).rstrip())
    return out


def preparar(nome, bioma, y):
    """O mesmo que a `CenaLeitor` faz antes do clique: o bioma em volta, o chão, o ar e a linha de três condutores; o jogador no mesmo lugar, olhando 40 graus para baixo."""
    topo = max(y, Y0)
    cmd("gamemode creative @s")
    cmd("clear @s")
    cmd("tp @s %.2f %.2f %.2f 0 40" % (X + 1.5, topo + 2.5, Z - 1.4))
    wait(40)
    cmd("fillbiome %d %d %d %d %d %d %s" % (X - 8, max(-64, y - 8), Z - 10, X + 12, min(319, y + 8), Z + 10, bioma))
    if y > Y0:
        cmd("fill %d %d %d %d %d %d minecraft:stone" % (X - 4, y - 1, Z - 8, X + 8, y - 1, Z + 10))
    cmd("fill 296 %d -2 308 %d 6 minecraft:air" % (topo, topo + 6))
    for i in range(3):
        cmd("setblock %d %d %d %s" % (X + i, y, Z, CONDUCTOR))
    cmd("tp @s %.2f %.2f %.2f 0 40" % (X + 1.5, topo + 2.5, Z - 1.4))


def sonda(depois):
    """As linhas `MBA` do log do jogo depois da linha `depois`: {(x, y, z): linha}."""
    rows = a.log_lines("MBA")
    return {tuple(r["at"]): r for r in rows[depois:] if "at" in r and "v" in r}


def clicar_no_meio(y):
    """O clique direito do Network Reader no condutor do meio, de quem olha o centro do bloco (a mira e o botão do mouse, como o jogador)."""
    pos = (X + 1, y, Z)
    return nr.clicar(pos, (0.5, 0.5, 0.5))


def normalizar(linhas):
    return [re.sub(r"\s+", " ", l).strip() for l in linhas if l.strip()]


def casos_no_port(com_original=True):
    original = original_log() if com_original else {}
    cmd("weather clear")
    cmd("time set noon")
    cmd("gamerule advance_time false")
    cmd("gamerule advance_weather false")
    h.ctl("options-set", "chatVisibility", "0")
    h.ctl("options-set", "guiScale", "3")
    ok = True
    a.SHOTS = os.environ.get("SHOTS", "~/.cache/rust-mekanism/ar-bioma/prints")
    a.LOCAL = LOCAL
    os.makedirs(LOCAL, exist_ok=True)
    resumo = []
    for nome, bioma, y, ate in casos():
        depois = len(a.log_lines("MBA"))
        preparar(nome, bioma, y)
        h.ctl("give", nr.READER, "1")
        wait(6)
        h.ctl("select", nr.READER)
        wait(ate)
        got = clicar_no_meio(y)
        arquivo = a.shot("ar_" + nome)
        lidas = sonda(depois)
        linhas = normalizar(got[-8:])
        row = {"nome": nome, "bioma": bioma, "y": y, "ar": {str(k): v["v"] for k, v in lidas.items()}, "sea": sorted({v.get("sea") for v in lidas.values()}), "chat": linhas}
        if nome in original:
            esperado = original[nome]
            diffs = [(k, v["v"], esperado["ar"].get(k)) for k, v in lidas.items() if esperado["ar"].get(k) != v["v"]]
            faltam = [k for k in esperado["ar"] if k not in lidas]
            row["ar_igual"] = not diffs
            row["ar_diferencas"] = diffs[:5]
            row["posicoes_lidas"] = len(lidas)
            row["posicoes_do_original"] = len(esperado["ar"])
            row["posicoes_sem_leitura"] = len(faltam)
            row["chat_igual"] = [re.sub(r"\d+(\.\d+)?[µmkM]?K", "#K", l) for l in linhas] == [re.sub(r"\d+(\.\d+)?[µmkM]?K", "#K", l) for l in normalizar(esperado["chat"])] and linhas[-3:] == normalizar(esperado["chat"])[-3:]
            ok &= row["ar_igual"] and row["chat_igual"]
            original_png = os.path.join(ORIGINAL, "prints", "leitor_oficial_%s.png" % nome)
            if os.path.exists(original_png):
                nr.LINHAS[nome] = 8
                nr.empilhar(original_png, arquivo, os.path.join(LOCAL, "comparacao_%s.png" % nome), nome)
        resumo.append(row)
        print(json.dumps(row, ensure_ascii=False))
    with open(os.path.join(LOCAL, "casos.jsonl"), "w") as file:
        for row in resumo:
            file.write(json.dumps(row, ensure_ascii=False) + "\n")
    print("TUDO IGUAL" if ok else "HÁ DIFERENÇAS")
    return ok


def spike():
    """A cena antiga das frentes de transmissores (BASE = (0, 150, 0), no mundo que estiver aberto): o que a sonda leu ali (temperatura base do bioma, nível do mar, ajuste de altura e o ar)."""
    depois = len(a.log_lines("MBA"))
    a.clear()
    cmd("gamemode creative @s")
    cmd("clear @s")
    for x in range(3):
        cmd("setblock %s %s" % (a.txt(a.at(x)), CONDUCTOR))
    wait(120)
    for pos, r in sorted(sonda(depois).items()):
        print(json.dumps(r))


def main():
    mode = sys.argv[1] if len(sys.argv) > 1 else "casos"
    if mode == "casos":
        sys.exit(0 if casos_no_port() else 1)
    if mode == "spike":
        spike()
        return
    raise SystemExit("modo desconhecido: " + mode)


if __name__ == "__main__":
    main()
