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


def preparar(nome, bioma, y, ate):
    """O mesmo que a `CenaLeitor` faz antes do clique: o bioma em volta, o chão, o ar e a linha de três condutores; o jogador no mesmo lugar, olhando 40 graus para baixo."""
    topo = max(y, Y0)
    cmd("tick unfreeze")
    cmd("gamemode creative @s")
    cmd("clear @s")
    cmd("tp @s %.2f %.2f %.2f 0 40" % (X + 1.5, topo + 2.5, Z - 1.4))
    wait(40)
    cmd("fillbiome %d %d %d %d %d %d %s" % (X - 8, max(-64, y - 8), Z - 10, X + 12, min(319, y + 8), Z + 10, bioma))
    if y > Y0:
        cmd("fill %d %d %d %d %d %d minecraft:stone" % (X - 4, y - 1, Z - 8, X + 8, y - 1, Z + 10))
    cmd("fill 296 %d -2 308 %d 6 minecraft:air" % (topo, topo + 6))
    cmd("fill %d %d %d %d %d %d minecraft:air" % (X, y, Z, X + 2, y, Z))
    if y < Y0:
        # A linha enterrada (abaixo do chão de teste): a vala em cima e ao norte dela, para a mira do jogador chegar ao condutor. O original clica pelo servidor e não precisa dela; o ar é lido
        # por posição e não muda com o bloco que está lá.
        cmd("fill %d %d %d %d %d %d minecraft:air" % (X, y, Z - 1, X + 2, Y0, Z))
    cmd("tp @s %.2f %.2f %.2f 0 40" % (X + 1.5, topo + 2.5, Z - 1.4))
    h.ctl("give", nr.READER, "1")
    wait(6)
    h.ctl("select", nr.READER)
    # O servidor parado: os condutores entram e andam exatamente `ate` tiques até o clique (o original clica 300 tiques depois de pô-los), que os números de vazão em microkelvin pedem.
    cmd("tick freeze")
    time.sleep(0.5)
    for i in range(3):
        cmd("setblock %d %d %d %s" % (X + i, y, Z, CONDUCTOR))
    a.step(ate)


def sonda():
    """As linhas `MBA` do log do jogo: {(x, y, z): [linhas]}. A sonda imprime uma vez por posição e valor, então o mesmo lugar com o mesmo ar em dois casos sai só no primeiro."""
    lidas = {}
    for r in a.log_lines("MBA"):
        if "at" in r and "v" in r:
            lidas.setdefault(tuple(r["at"]), []).append(r)
    return lidas


def clicar_no_meio(y):
    """O clique direito do Network Reader no condutor do meio, de quem olha o centro do bloco (a mira e o botão do mouse, como o jogador)."""
    pos = (X + 1, y, Z)
    return nr.clicar(pos, (0.5, 0.5, 0.5))


def normalizar(linhas):
    return [re.sub(r"\s+", " ", l).strip() for l in linhas if l.strip() and not l.startswith("Stepping")]


def casos_no_port(com_original=True):
    original = original_log() if com_original else {}
    cmd("weather clear")
    cmd("time set noon")
    cmd("gamerule advance_time false")
    cmd("gamerule advance_weather false")
    cmd("gamerule send_command_feedback false")
    h.ctl("options-set", "chatVisibility", "0")
    h.ctl("options-set", "guiScale", "3")
    ok = True
    a.SHOTS = os.environ.get("SHOTS", "~/.cache/rust-mekanism/ar-bioma/prints")
    a.LOCAL = LOCAL
    os.makedirs(LOCAL, exist_ok=True)
    resumo = []
    escolhidos = os.environ.get("CASOS")
    for nome, bioma, y, ate in casos():
        if escolhidos and nome not in escolhidos.split(","):
            continue
        preparar(nome, bioma, y, ate)
        got = clicar_no_meio(y)
        arquivo = a.shot("ar_" + nome)
        cmd("tick unfreeze")
        lidas = sonda()
        linhas = normalizar(got)
        row = {"nome": nome, "bioma": bioma, "y": y, "chat": linhas}
        if nome in original:
            esperado = original[nome]
            # O ar de cada posição que o original leu tem de ter saído na sonda do port com o mesmo valor, e o nível do mar que o port leu (`mar`) tem de ser o -63 do original.
            faltam = [(k, v) for k, v in esperado["ar"].items() if not any(r["v"] == v for r in lidas.get(k, []))]
            mares = sorted({r.get("sea") for k, v in esperado["ar"].items() for r in lidas.get(k, []) if r["v"] == v})
            row["ar_igual"] = not faltam
            row["ar_sem_igual"] = [(list(k), v) for k, v in faltam[:5]]
            row["ar_posicoes"] = len(esperado["ar"])
            row["mar"] = mares
            tamanho = len(esperado["chat"])
            row["chat_igual"] = linhas[-tamanho:] == normalizar(esperado["chat"])
            if not row["chat_igual"]:
                row["chat_original"] = normalizar(esperado["chat"])
            ok &= row["ar_igual"] and row["chat_igual"] and row["mar"] == [esperado["sea"]]
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
    a.clear()
    cmd("gamemode creative @s")
    cmd("clear @s")
    for x in range(3):
        cmd("setblock %s %s" % (a.txt(a.at(x)), CONDUCTOR))
    wait(120)
    for pos, r in sorted((k, v[-1]) for k, v in sonda().items() if k[1] >= 140 and k[0] < 10):
        print(json.dumps(r))


# O vídeo: quatro linhas de condutores, uma a cada bioma e altura (as três do chão a 24 blocos uma da outra, a quarta no alto). No mundo plano do port a grama é y = -61 (o original põe a dele em -62):
# as linhas do chão ficam em cima dela, a y = -60, onde o ar é o mesmo de y = -61 (abaixo de `seaLevel + 17` nada muda com a altura).
VIDEO_LINHAS = [
    ("planicie", "minecraft:plains", -60, 0),
    ("deserto", "minecraft:desert", -60, 24),
    ("neve", "minecraft:snowy_plains", -60, 48),
    ("floresta_alta", "minecraft:forest", 150, 72),
]


def olhar_de(x, y, z, alvo):
    """O jogador a 1,4 ao norte da linha e a 2,5 acima dela (a mira dos casos), olhando o alvo."""
    cmd("tp @s %.2f %.2f %.2f 0 40" % (x + 1.5, max(y, Y0 + 1) + 2.5, z - 1.4))
    wait(12)
    h.ctl("look-at", "%.3f" % (alvo[0] + 0.5), "%.3f" % (alvo[1] + 0.5), "%.3f" % (alvo[2] + 0.5))
    wait(6)


def montar_video():
    """Fora da câmera: o bioma em volta de cada linha, o chão no alto, as três linhas prontas (desert, snowy e forest a y 150) com 300 tiques andados, e a primeira (plains) só com o chão livre."""
    cmd("tick unfreeze")
    cmd("gamerule send_command_feedback false")
    cmd("gamemode creative @s")
    cmd("clear @s")
    for nome, bioma, y, z in VIDEO_LINHAS:
        cmd("tp @s %.2f %.2f %.2f 0 40" % (X + 1.5, max(y, Y0 + 1) + 2.5, z - 1.4))
        wait(40)
        cmd("fillbiome %d %d %d %d %d %d %s" % (X - 8, max(-64, y - 8), z - 8, X + 12, min(319, y + 8), z + 8, bioma))
        if y > Y0:
            cmd("fill %d %d %d %d %d %d minecraft:stone" % (X - 4, y - 1, z - 8, X + 8, y - 1, z + 8))
        if y <= Y0 + 1:
            # A grama de volta onde os casos abriram vala, e o ar em cima dela.
            cmd("fill %d %d %d %d %d %d minecraft:grass_block" % (X - 8, Y0, z - 8, X + 12, Y0, z + 8))
        cmd("fill %d %d %d %d %d %d minecraft:air" % (X - 4, max(y, Y0 + 1), z - 4, X + 8, max(y, Y0 + 1) + 6, z + 4))
    cmd("tick freeze")
    time.sleep(0.5)
    for nome, bioma, y, z in VIDEO_LINHAS[1:]:
        for i in range(3):
            cmd("setblock %d %d %d %s" % (X + i, y, z, CONDUCTOR))
    a.step(300)
    cmd("tick unfreeze")
    wait(20)
    h.ctl("give", nr.READER, "1")
    h.ctl("give", CONDUCTOR, "3")
    wait(6)


def roteiro_video(pausa):
    """Na câmera: a linha de plains posta com o botão direito (o som do bloco), os 300 tiques andados depressa (`tick rate`), e o leitor no condutor do meio de cada uma das quatro linhas."""
    nome, bioma, y, z = VIDEO_LINHAS[0]
    olhar_de(X, y, z, (X + 1, y, z))
    pausa(2)
    h.ctl("select", CONDUCTOR)
    for i in range(3):
        out = h.ctl("place", str(X + i), str(y), str(z), "--item", CONDUCTOR)
        if '"error"' in out:
            print("   place falhou:", out.strip()[:140])
        pausa(0.6)
    cmd("tick rate 200")
    pausa(2.5)
    cmd("tick rate 20")
    pausa(1)
    h.ctl("select", nr.READER)
    for nome, bioma, y, z in VIDEO_LINHAS:
        olhar_de(X, y, z, (X + 1, y, z))
        pausa(1.5)
        h.ctl("mouse", "right", "click")
        pausa(6)


def video():
    """O vídeo com som (`NOME`, `SEGUNDOS`, `ARQUIVO`): o `h2_cabo_jogo.video` com o chat à vista."""
    import threading
    import h2_cabo_jogo as c
    import h2_nota_instrumento as n
    nome, segundos, arquivo = os.environ["NOME"], os.environ.get("SEGUNDOS", "55"), os.environ.get("ARQUIVO", "ar_bioma.mp4")
    n.sky()
    montar_video()
    n.mute()
    h.ctl("options-set", "chatVisibility", "0")
    h.ctl("options-set", "guiScale", "3")
    for categoria in n.SOUND_OFF:
        h.ctl("options-set", f"soundCategory_{categoria}", "0")
    for categoria in ("block", "record"):
        h.ctl("options-set", f"soundCategory_{categoria}", "1.0")
    ligado = False
    for _ in range(60):
        if "movidos 0" not in n.desktop(f"ligar {n.CONTAINER}").stdout and n.desktop(f"verificar {n.CONTAINER}").stdout.startswith("ok"):
            ligado = True
            break
        time.sleep(1)
    if not ligado:
        print("o stream do jogo não foi para o sink nulo: nada gravado, volume mestre em 0")
        sys.exit(1)
    parar = threading.Event()
    grav = f"som_grav_{nome}"
    ate = [None]

    def vigia():
        while not parar.is_set():
            if ate[0] is not None and time.time() < ate[0]:
                subprocess.run(["ssh", "desktop", f"AUDIO_SINK={grav} {n.AUDIO_NULO} ligar {n.CONTAINER}"], capture_output=True, text=True)
            if not n.desktop(f"seguro {n.CONTAINER}").stdout.startswith("ok"):
                print("stream fora dos sinks de teste: volume mestre em 0", flush=True)
                n.mute()
            parar.wait(0.5)

    thread = threading.Thread(target=vigia, daemon=True)
    thread.start()
    rec = None
    try:
        h.ctl("options-set", "soundCategory_master", "1.0")
        time.sleep(2.0)
        rec = subprocess.Popen(["ssh", "desktop", f"{n.GRAVAR} rec {nome} {arquivo} {segundos} {n.CONTAINER}"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        time.sleep(3.0)
        ate[0] = time.time() + int(segundos) - 8
        roteiro_video(lambda s: time.sleep(s))
        out, err = rec.communicate(timeout=int(segundos) + 90)
        print(out, err, flush=True)
    finally:
        n.mute()
        parar.set()
        thread.join(timeout=5)
        if rec is not None and rec.poll() is None:
            rec.wait(timeout=60)
        n.restore()


def main():
    mode = sys.argv[1] if len(sys.argv) > 1 else "casos"
    if mode == "casos":
        sys.exit(0 if casos_no_port() else 1)
    if mode == "spike":
        spike()
        return
    if mode == "video":
        video()
        return
    if mode == "cena":
        montar_video()
        roteiro_video(lambda t: time.sleep(t))
        return
    raise SystemExit("modo desconhecido: " + mode)


if __name__ == "__main__":
    main()
