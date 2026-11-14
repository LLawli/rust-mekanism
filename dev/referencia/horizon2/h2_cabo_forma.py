#!/usr/bin/env python3
"""A forma de seleção e a de colisão dos transmissores no jogo (frente `cabo-forma`, Fase 4): o cabo pequeno e o transportador grande com um braço de cada tipo, no port e no original.

    python3 h2_cabo_forma.py gerar > ../cenas/forma.txt          # os passos da cena (o original lê o mesmo arquivo: `testes/CenaForma.java`)
    H_NAME=horizon2 PASTA=<pasta do game.log> h2_cabo_forma.py montar         # limpa a área, monta as quatro cenas e põe os tipos `PUSH` e `PULL` pelo Configurator agachado
    H_NAME=horizon2 h2_cabo_forma.py itens SAIDA.json            # solta os itens de `cenas/forma.txt`, espera parar e lê o `Pos` de cada um
    H_NAME=horizon2 h2_cabo_forma.py pessoa SAIDA.json           # o jogador cai sobre o braço, e anda de encontro ao centro e aos braços: a posição em que para
    NOME=... H_NAME=horizon2 h2_cabo_forma.py contorno           # grava as poses de câmera da cena e extrai os quadros (o contorno só aparece na captura PipeWire)
    h2_cabo_forma.py comparar PORT.json LATEST.log               # port x original, linha a linha

A cena é a mesma nos dois jogos, em coordenadas relativas à origem `O` de cada um (port: `BASE` de `h2_ajustes_jogo`; original: 300 -61 10):
  `pequeno`  (O + 0 0 0):   o cabo no centro, cubo de energia criativo a oeste (`PUSH`) e a leste (`PULL`), um cabo ao sul (`NORMAL`) e nada ao norte (sem conexão).
  `grande`   (O + 0 0 12):  o transportador logístico, com baús no lugar dos cubos, e o mesmo resto.
  `reta`     (O + 0 0 24):  três cabos em linha (x de 0 a 2), o do meio só com braços `NORMAL` (a forma do estado do Horizon contra `getRealShape`).
  `esquina`  (O + 0 0 30):  cabos em (0,0,30), (1,0,30) e (0,0,31): a esquina é o do (0,0,30), com braços a leste e ao sul.
"""
import json, os, re, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

CABO = "mekanism:basic_universal_cable"
CUBO = "mekanism:creative_energy_cube"
TRANSP = "mekanism:basic_logistical_transporter"
BAU = "minecraft:chest"
CONFIG_CUBO = "{component_config:{config0:[I;4,4,4,4,4,4],eject0:0b},energy_amount:9223372036854775807L}"
# Cena: (nome, origem do transmissor do meio, bloco, aceitador).
CENAS = [("pequeno", (0, 0, 0), CABO, CUBO), ("grande", (0, 0, 12), TRANSP, BAU)]
PONTA = [0.07, 0.13, 0.20, 0.27, 0.34, 0.40]  # a distância do ponto à face do bloco, ao longo do braço
LADO = [0.0, 0.20, 0.30, 0.34, 0.38, 0.42]  # o deslocamento lateral (a partir do eixo do braço)
LADO_S = [0.0, 0.20, 0.34]  # o braço ao sul tem o cubo a leste e a oeste da coluna: sem os deslocamentos que passam do meio bloco
ALTURA = 1.6  # de onde o item cai, acima do chão
CAMERA_DIST = 2.6


def itens_do_braco(cena, braco, origem, pontas, lados):
    """Os itens de um braço: `braco` é `W`, `E` ou `S`. O deslocamento lateral vai ao norte nos braços de leste e oeste, e a oeste no do sul."""
    ox, oy, oz = origem
    out = []
    for i, p in enumerate(pontas):
        for j, d in enumerate(lados):
            if braco == "W":
                x, z = ox + p, oz + 0.5 - d
            elif braco == "E":
                x, z = ox + 1 - p, oz + 0.5 - d
            else:  # S
                x, z = ox + 0.5 - d, oz + 1 - p
            out.append((f"{cena}.{braco}.p{i}d{j}", round(x, 4), oy + ALTURA, round(z, 4)))
    return out


def lista_itens():
    """Todos os itens da cena, em coordenadas relativas à origem."""
    out = []
    for cena, o, _, _ in CENAS:
        out += itens_do_braco(cena, "W", o, PONTA, LADO)
        out += itens_do_braco(cena, "E", o, PONTA, LADO)
        out += itens_do_braco(cena, "S", o, PONTA, LADO_S)
        out.append((f"{cena}.centro", o[0] + 0.5, o[1] + ALTURA, o[2] + 0.5))
    # A reta: o do meio é (1,0,24); braços a oeste e a leste (para os cabos vizinhos); o lado, ao norte.
    o = (1, 0, 24)
    out += itens_do_braco("reta", "W", o, [0.03, 0.10, 0.20, 0.27, 0.34, 0.40], LADO)
    out += itens_do_braco("reta", "E", o, [0.03, 0.10, 0.20, 0.27, 0.34, 0.40], LADO)
    out.append(("reta.centro", o[0] + 0.5, o[1] + ALTURA, o[2] + 0.5))
    out += [("reta.N.p0", o[0] + 0.5, o[1] + ALTURA, o[2] + 0.07), ("reta.N.p1", o[0] + 0.5, o[1] + ALTURA, o[2] + 0.2), ("reta.S.p0", o[0] + 0.5, o[1] + ALTURA, o[2] + 0.93)]
    # A esquina é (0,0,30): braços a leste e ao sul; sem braço a oeste e ao norte.
    o = (0, 0, 30)
    out += itens_do_braco("esquina", "E", o, [0.03, 0.10, 0.20, 0.27, 0.34, 0.40], LADO)
    out += itens_do_braco("esquina", "S", o, [0.03, 0.10, 0.20, 0.27, 0.34, 0.40], LADO)
    out.append(("esquina.centro", o[0] + 0.5, o[1] + ALTURA, o[2] + 0.5))
    out += [("esquina.W.p0", o[0] + 0.07, o[1] + ALTURA, o[2] + 0.5), ("esquina.W.p1", o[0] + 0.2, o[1] + ALTURA, o[2] + 0.5),
            ("esquina.N.p0", o[0] + 0.5, o[1] + ALTURA, o[2] + 0.07), ("esquina.N.p1", o[0] + 0.5, o[1] + ALTURA, o[2] + 0.2)]
    return [(n, round(x, 4), round(y, 4), round(z, 4)) for n, x, y, z in out]


def lista_flechas():
    """As flechas que caem sobre a ponta de cada braço do cabo pequeno (`itens_do_braco`): o oeste (`PUSH`), o leste (`PULL`) e o sul (`NORMAL`), da ponta e a 0,27 dela. Lidas
    como `flecha`; a flecha parada no chão fica onde `ArrowScene.collision_boxes` a deixa."""
    out = []
    for braco in ("W", "E", "S"):
        for nome, x, y, z in itens_do_braco("pequeno", braco, (0, 0, 0), [0.07, 0.27], [0.0]):
            out.append(("flecha." + nome, x, y + 0.2, z))
    return out


def lista_pessoas():
    """(nome, x, y, z, yaw, ticks): ticks 0 é o jogador que cai e para; os outros andam para o sul (yaw 0) por `ticks` tiques e param de encontro ao que houver."""
    out = []
    for cena, o, _, _ in CENAS:
        ox, oy, oz = o
        for nome, x, z in (("braco_W", 0.31, 0.5), ("braco_E", 0.69, 0.5), ("braco_S", 0.5, 0.69), ("centro", 0.5, 0.5), ("braco_W_lado", 0.31, 0.1), ("braco_E_lado", 0.69, 0.1)):
            out.append((f"{cena}.cai.{nome}", ox + x, oy + 2.0, oz + z, 0, 0))
        for nome, x in (("centro", 0.5), ("W", 0.31), ("E", 0.69)):
            out.append((f"{cena}.anda.{nome}", ox + x, oy, oz - 1.5, 0, 50))
    # A reta e a esquina, sem `PUSH` nem `PULL`: o jogador cai sobre os braços e anda de encontro ao centro.
    for cena, o, nomes in (("reta", (1, 0, 24), (("braco_W", 0.2, 0.5), ("braco_E", 0.8, 0.5), ("centro", 0.5, 0.5))), ("esquina", (0, 0, 30), (("braco_E", 0.8, 0.5), ("braco_S", 0.5, 0.8), ("centro", 0.5, 0.5)))):
        for nome, x, z in nomes:
            out.append((f"{cena}.cai.{nome}", o[0] + x, o[1] + 2.0, o[2] + z, 0, 0))
        out.append((f"{cena}.anda.centro", o[0] + 0.5, o[1], o[2] - 1.5, 0, 50))
    return [(n, round(x, 4), round(y, 4), round(z, 4), yaw, t) for n, x, y, z, yaw, t in out]


def lista_cameras():
    """(nome, x, y, z, alvo x, y, z): a câmera do contorno. Os pés ao nível do chão, 2,6 ao norte do transmissor, olhando o centro; e dois enquadramentos de perto, nos braços de oeste e de leste."""
    out = []
    for cena, o, _, _ in CENAS:
        ox, oy, oz = o
        out.append((f"{cena}_inteiro", ox + 0.5, oy, oz - CAMERA_DIST, ox + 0.5, oy + 0.5, oz + 0.5))
        out.append((f"{cena}_oeste", ox + 0.5, oy, oz - 1.2, ox + 0.15, oy + 0.5, oz + 0.5))
        out.append((f"{cena}_leste", ox + 0.5, oy, oz - 1.2, ox + 0.85, oy + 0.5, oz + 0.5))
    out.append(("reta_inteiro", 1.5, 0, 24 - CAMERA_DIST, 1.5, 0.5, 24.5))
    out.append(("esquina_inteiro", 0.5, 0, 30 - CAMERA_DIST, 0.5, 0.5, 30.5))
    return out


def blocos():
    """(x, y, z, bloco) de cada bloco da cena, mais os tipos crus a pôr: (x, y, z, lado, tipo)."""
    out, tipos, dados = [], [], []
    for cena, o, bloco, acc in CENAS:
        ox, oy, oz = o
        out += [(ox, oy, oz, bloco), (ox - 1, oy, oz, acc), (ox + 1, oy, oz, acc), (ox, oy, oz + 1, bloco)]
        tipos += [(ox, oy, oz, "west", "PUSH"), (ox, oy, oz, "east", "PULL")]
        if acc == CUBO:
            dados += [(ox - 1, oy, oz), (ox + 1, oy, oz)]
    out += [(0, 0, 24, CABO), (1, 0, 24, CABO), (2, 0, 24, CABO)]
    out += [(0, 0, 30, CABO), (1, 0, 30, CABO), (0, 0, 31, CABO)]
    return out, tipos, dados


def gerar():
    print("# A cena da forma dos transmissores (`testes/CenaForma.java` e `horizon2/h2_cabo_forma.py`): coordenadas relativas à origem de cada jogo. Gerado por `h2_cabo_forma.py gerar`.")
    print("a=limpa")
    bl, tipos, dados = blocos()
    for x, y, z, b in bl:
        print(f"a=bloco x={x} y={y} z={z} b={b}")
    for x, y, z in dados:
        print(f"a=dados x={x} y={y} z={z} nbt={CONFIG_CUBO}")
    print("a=espera n=40")
    for x, y, z, lado, tipo in tipos:
        print(f"a=tipo x={x} y={y} z={z} lado={lado} tipo={tipo}")
    print("a=espera n=20")
    for nome, x, y, z, ax, ay, az in lista_cameras():
        print(f"a=camera nome={nome} x={x} y={y} z={z} ax={ax} ay={ay} az={az} espera=50")
    for nome, x, y, z in lista_itens():
        print(f"a=item nome={nome} x={x} y={y} z={z}")
    for nome, x, y, z in lista_flechas():
        print(f"a=flecha nome={nome} x={x} y={y} z={z}")
    print("a=espera n=100")
    print("a=ler")
    print("a=mata")
    print("a=mataflecha")
    for nome, x, y, z, yaw, t in lista_pessoas():
        if t == 0:
            print(f"a=cai nome={nome} x={x} y={y} z={z} yaw={yaw}")
        else:
            print(f"a=anda nome={nome} x={x} y={y} z={z} yaw={yaw} n={t}")
    print("a=fim")


# ---------------------------------------------------------------- o port

def port():
    import h2_identity as h
    import h2_ajustes_jogo as aj
    return h, aj


def origem():
    import h2_ajustes_jogo as aj
    return aj.BASE


def mundo(p, o=None):
    o = o or origem()
    return (o[0] + p[0], o[1] + p[1], o[2] + p[2])


def montar():
    h, aj = port()
    cmd, wait = aj.cmd, aj.wait
    o = origem()
    cmd("tick unfreeze")
    cmd("gamemode creative @s")
    cmd("clear @s")
    h.ctl("options-set", "chatVisibility", "2")
    cmd("fill %d %d %d %d %d %d minecraft:stone" % (o[0] - 8, o[1] - 1, o[2] - 8, o[0] + 12, o[1] - 1, o[2] + 40))
    cmd("fill %d %d %d %d %d %d minecraft:air" % (o[0] - 8, o[1], o[2] - 8, o[0] + 12, o[1] + 8, o[2] + 40))
    cmd("kill @e[type=item]")
    wait(10)
    bl, tipos, dados = blocos()
    for x, y, z, b in bl:
        cmd("setblock %d %d %d %s" % (*mundo((x, y, z)), b))
    wait(10)
    for x, y, z in dados:
        cmd("data merge block %d %d %d %s" % (*mundo((x, y, z)), CONFIG_CUBO))
    wait(40)
    h.ctl("give", "mekanism:configurator", "1")
    # `PUSH` e `PULL`: o Configurator agachado no braço (o ciclo é `NORMAL`, `PUSH`, `PULL`, `NONE`).
    for x, y, z, lado, tipo in tipos:
        bloco = mundo((x, y, z))
        ponto = aj.ARM["west" if lado == "west" else "east"]
        stand = (bloco[0] + 0.5, bloco[1], bloco[2] - CAMERA_DIST)
        cliques = 1 if tipo == "PUSH" else 2
        for _ in range(cliques):
            aimed = aj.click_at(bloco, ponto, stand)
            print(f"{x},{y},{z} {lado} {tipo}: clique, mira {aimed}")
    h.ctl("select", "minecraft:stick")
    cmd("clear @s")


def ler_pos(seletor="@s"):
    h, aj = port()
    out = aj.cmd(f"data get entity {seletor} Pos")
    m = re.search(r"\[([-0-9.e]+)d, ([-0-9.e]+)d, ([-0-9.e]+)d\]", out)
    return tuple(float(v) for v in m.groups()) if m else None


def itens(saida):
    h, aj = port()
    cmd, wait = aj.cmd, aj.wait
    cmd("kill @e[type=item]")
    cmd("tick unfreeze")
    pontos = lista_itens()
    t0 = time.time()
    for n, x, y, z in pontos:
        w = mundo((x, y, z))
        cmd('summon item %.4f %.4f %.4f {Item:{id:"minecraft:stone",count:1},Motion:[0.0,0.0,0.0],PickupDelay:32767s,Tags:["forma","%s"]}' % (*w, n.replace(".", "_")))
    print(f"{len(pontos)} itens soltos em {time.time() - t0:.1f} s")
    wait(120)
    res = {}
    for n, x, y, z in pontos:
        pos = ler_pos('@e[tag=%s,limit=1]' % n.replace(".", "_"))
        res[n] = pos
    cmd("kill @e[type=item]")
    json.dump({"origem": origem(), "itens": res}, open(saida, "w"), indent=1)
    print(f"{sum(1 for v in res.values() if v)} lidos de {len(pontos)} em {time.time() - t0:.1f} s")


def flechas(saida):
    h, aj = port()
    cmd, wait = aj.cmd, aj.wait
    cmd("kill @e[tag=flecha]")
    cmd("tick unfreeze")
    pontos = lista_flechas()
    for n, x, y, z in pontos:
        w = mundo((x, y, z))
        cmd('summon arrow %.4f %.4f %.4f {Motion:[0.0,-0.5,0.0],Tags:["forma","flecha","%s"]}' % (*w, n.replace(".", "_")))
    wait(100)
    res = {n: ler_pos('@e[tag=%s,limit=1]' % n.replace(".", "_")) for n, x, y, z in pontos}
    cmd("kill @e[tag=flecha]")
    json.dump({"origem": origem(), "itens": res}, open(saida, "w"), indent=1)
    print(f"{sum(1 for v in res.values() if v)} flechas lidas de {len(pontos)}")


def pessoa(saida):
    h, aj = port()
    cmd, wait = aj.cmd, aj.wait
    cmd("gamemode creative @s")
    cmd("clear @s")
    cmd("tick unfreeze")
    res = {}
    for n, x, y, z, yaw, t in lista_pessoas():
        w = mundo((x, y, z))
        cmd("tp @s %.4f %.4f %.4f %d 0" % (*w, yaw))
        wait(10)
        if t:
            h.ctl("key", "w", "down")
            wait(t)
            h.ctl("key", "w", "up")
            wait(10)
        else:
            wait(40)
        pos = ler_pos("@s")
        res[n] = pos
        print(n, pos)
    json.dump({"origem": origem(), "pessoas": res}, open(saida, "w"), indent=1)


# ---------------------------------------------------------------- o vídeo e os casos de borda

VIDEO_Z = -5  # a linha de cabos do vídeo: x de -4 a 4 em z -5; cubos de energia ao sul de x=-2 (`PUSH`) e x=1 (`PULL`) e um cabo ao sul de x=3 (`NORMAL`)
VIDEO_CABOS = list(range(-4, 5))
VIDEO_ALVOS = [(-2, "push"), (1, "pull"), (3, "normal"), (-0, "centro")]


BORDA = (0, 0, 36)  # o cabo dos casos de borda; o cubo de energia fica a leste (1, 0, 36)
BORDA_PONTO = (0.87, 1.6, 36.16)  # sobre a ponta do braço leste, 0,34 ao norte do eixo: só o braço `PULL` (a ponta mais larga) o segura (y 0,75); com `NORMAL`, `PUSH` ou sem braço ele cai ao chão (0)


def queda(ponto=BORDA_PONTO, espera=30, nome="zb"):
    """Solta uma pedra parada em `ponto` (relativo à origem), espera `espera` tiques e devolve o y de repouso (relativo)."""
    h, aj = port()
    w = mundo(ponto)
    aj.cmd('summon item %.4f %.4f %.4f {Item:{id:"minecraft:stone",count:1},Motion:[0.0,0.0,0.0],PickupDelay:32767s,Tags:["forma","%s"]}' % (*w, nome))
    aj.wait(espera)
    pos = ler_pos("@e[tag=%s,limit=1]" % nome)
    aj.cmd("kill @e[tag=%s]" % nome)
    return None if pos is None else round(pos[1] - origem()[1], 4)


def clique_borda():
    h, aj = port()
    bloco = mundo(BORDA)
    stand = (bloco[0] + 0.5, bloco[1], bloco[2] - CAMERA_DIST)
    return aj.click_at(bloco, aj.ARM["east"], stand)


def vista_borda():
    h, aj = port()
    v = aj.last_views("MBV").get(mundo(BORDA))
    return v and (v["raw"][5], v["kind"][5], v["conn"][5])


def bordas():
    """Os casos de borda da colisão: (1) trocar o tipo de um lado pelo Configurator (o ciclo `NORMAL`, `PUSH`, `PULL`, `NONE`, `NORMAL`) atualiza a colisão no mesmo tique da troca; (2) quebrar o cabo e pôr
    outro bloco no lugar não deixa a forma antiga. O item cai sobre a ponta do braço leste (`BORDA_PONTO`): 0 sem o braço `PULL`, 0,75 com ele."""
    h, aj = port()
    cmd, wait = aj.cmd, aj.wait
    cmd("tick unfreeze")
    cmd("gamemode creative @s")
    cmd("clear @s")
    bl = mundo(BORDA)
    cmd("setblock %d %d %d %s" % (*bl, CABO))
    cmd("setblock %d %d %d %s" % (*mundo((BORDA[0] + 1, BORDA[1], BORDA[2])), CUBO))
    wait(10)
    cmd("data merge block %d %d %d %s" % (*mundo((BORDA[0] + 1, BORDA[1], BORDA[2])), CONFIG_CUBO))
    wait(40)
    h.ctl("give", "mekanism:configurator", "1")
    res = {}
    res["inicial"] = (vista_borda(), queda())
    print("inicial", res["inicial"])
    for n in range(1, 5):
        # Prepara o clique do Configurator agachado no braço leste (sem o tempo parado: a mira e o agachar precisam de tiques), para o tempo, clica e anda um tique por vez até a vista publicada mudar;
        # nesse mesmo tique solta o item, a 0,02 acima da ponta `PULL` (cai no primeiro tique): ele só para em 0,75 se a forma de colisão já é a nova.
        stand = (bl[0] + 0.5, bl[1], bl[2] - CAMERA_DIST)
        cmd("tp @s %.2f %.2f %.2f" % stand)
        wait(12)
        h.ctl("select", "mekanism:configurator")
        h.ctl("key", "shift", "down")
        wait(6)
        h.ctl("look-at", *("%.3f" % (bl[i] + aj.ARM["east"][i]) for i in range(3)))
        wait(6)
        antes = vista_borda()
        cmd("tick freeze")
        time.sleep(0.4)
        h.ctl("mouse", "right", "click")
        passos = 0
        while vista_borda() == antes and passos < 40:
            cmd("tick step 1")
            time.sleep(0.4)
            passos += 1
        depois = vista_borda()
        w = mundo((BORDA_PONTO[0], 0.77, BORDA_PONTO[2]))
        cmd('summon item %.4f %.4f %.4f {Item:{id:"minecraft:stone",count:1},Motion:[0.0,0.0,0.0],PickupDelay:32767s,Tags:["forma","zc"]}' % w)
        cmd("tick step 30")
        time.sleep(2.5)
        pos = ler_pos("@e[tag=zc,limit=1]")
        y = None if pos is None else round(pos[1] - origem()[1], 4)
        cmd("kill @e[tag=zc]")
        cmd("tick unfreeze")
        h.ctl("key", "shift", "up")
        print(f"clique {n}: {antes} -> {depois} em {passos} tiques; queda no tique da troca {y}")
        res[f"clique{n}"] = (antes, depois, passos, y)
        wait(4)
    # O quarto clique cai na face (o lado leste está em `NONE`, sem braço para mirar). O cabo é posto de novo (o tipo cru volta a `NORMAL`), o `PULL` volta com dois cliques, e o cabo é quebrado.
    cmd("setblock %d %d %d minecraft:air" % bl)
    wait(10)
    cmd("setblock %d %d %d %s" % (*bl, CABO))
    wait(30)
    for _ in range(2):
        clique_borda()
        wait(6)
    wait(10)
    print("antes de quebrar", vista_borda(), "queda", queda())
    res["pull_de_novo"] = (vista_borda(), queda())
    cmd("setblock %d %d %d minecraft:air" % bl)
    wait(10)
    ar = queda()
    cmd("setblock %d %d %d minecraft:stone_slab" % bl)
    wait(10)
    laje = queda()
    cmd("setblock %d %d %d minecraft:air" % bl)
    wait(5)
    cmd("setblock %d %d %d %s" % (*bl, CABO))
    wait(30)
    de_novo = queda()
    print(f"cabo quebrado: ar {ar} (0 esperado), laje no lugar {laje} (0,5 esperado), cabo posto de novo {de_novo} e vista {vista_borda()} (0 esperado: o tipo cru volta a NORMAL)")
    res["quebra"] = {"ar": ar, "laje": laje, "de_novo": de_novo}
    return res


def montar_video():
    h, aj = port()
    cmd, wait = aj.cmd, aj.wait
    o = origem()
    for x in VIDEO_CABOS:
        cmd("setblock %d %d %d %s" % (*mundo((x, 0, VIDEO_Z)), CABO))
    for x in (-2, 1):
        cmd("setblock %d %d %d %s" % (*mundo((x, 0, VIDEO_Z + 1)), CUBO))
    cmd("setblock %d %d %d %s" % (*mundo((3, 0, VIDEO_Z + 1)), CABO))
    wait(10)
    for x in (-2, 1):
        cmd("data merge block %d %d %d %s" % (*mundo((x, 0, VIDEO_Z + 1)), CONFIG_CUBO))
    wait(30)
    h.ctl("give", "mekanism:configurator", "1")
    for x, tipo in VIDEO_ALVOS[:2]:
        bloco = mundo((x, 0, VIDEO_Z))
        for _ in range(1 if tipo == "push" else 2):
            stand = (bloco[0] + 0.5, bloco[1] + 0.6875, bloco[2] + 0.5)
            aimed = aj.click_at(bloco, (0.5, 0.6875, 0.85), stand)
            print(f"video x={x} {tipo}: clique, mira {aimed}")
    cmd("clear @s")
    cmd("tp @s %.4f %.4f %.4f -90 35" % mundo((-3.5, 0.6875, VIDEO_Z + 0.5)))
    wait(20)


def mirar_arco(h, x, z_ponto=0.85):
    """Olha o braço sul do cabo `x` da linha do vídeo, de cima."""
    h.ctl("look-at", *("%.4f" % v for v in mundo((x + 0.5, 0.6875, VIDEO_Z + z_ponto))))


def roteiro_video(pausa, marca):
    h, aj = port()
    cmd, wait = aj.cmd, aj.wait
    cmd("clear @s")
    cmd("tp @s %.4f %.4f %.4f -90 35" % mundo((-3.5, 0.6875, VIDEO_Z + 0.5)))
    wait(20)
    pausa(1.0)
    atual = -3.5
    for x, nome in VIDEO_ALVOS:
        alvo_x = x + 0.5
        # Anda para leste até o cabo, olhando para a frente e para baixo.
        passos = max(0.0, (alvo_x - atual) / 4.3)
        if passos > 0.05:
            h.ctl("move", "forward", "%.2f" % passos)
        atual = alvo_x
        wait(6)
        mirar_arco(h, x) if nome != "centro" else h.ctl("look-at", *("%.4f" % v for v in mundo((x + 0.5, 0.6875, VIDEO_Z + 0.5))))
        wait(8)
        marca(f"video_{nome}")
        pausa(4.0)
        h.ctl("look", "-90", "35")
        wait(6)


def video():
    """O vídeo com som: o jogador anda sobre a linha de cabos (`montar_video`) e mira o braço sul de cada um (o contorno mostra a forma inteira do cabo que ele mira). Variáveis: `NOME` (o
    `gravar-jogo start`), `SEGUNDOS` (padrão 45), `ARQUIVO` (padrão forma-video.mp4). O volume mestre só sobe depois de o stream estar no sink nulo (`ligar`/`verificar`), uma thread confere o
    `seguro` a cada meio segundo e o volta a 0 se o stream sair dos sinks de teste."""
    h, aj = port()
    import threading
    import h2_nota_instrumento as n
    nome, segundos, arquivo = os.environ["NOME"], os.environ.get("SEGUNDOS", "45"), os.environ.get("ARQUIVO", "forma-video.mp4")
    n.sky()
    aj.cmd("tick unfreeze")
    n.mute()
    h.ctl("options-set", "chatVisibility", "2")
    for categoria in n.SOUND_OFF:
        h.ctl("options-set", f"soundCategory_{categoria}", "0")
    for categoria in ("block", "record", "player"):
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
    marcas = []
    try:
        h.ctl("options-set", "soundCategory_master", "1.0")
        time.sleep(2.0)
        rec = subprocess.Popen(["ssh", "desktop", f"{n.GRAVAR} rec {nome} {arquivo} {segundos} {n.CONTAINER}"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        t0 = time.time()
        time.sleep(3.0)
        ate[0] = time.time() + int(segundos) - 8

        def marca(rotulo):
            marcas.append((rotulo, round(time.time() - t0, 1)))

        roteiro_video(lambda s: time.sleep(s), marca)
        out, err = rec.communicate(timeout=int(segundos) + 90)
        print(out, err, flush=True)
    finally:
        n.mute()
        parar.set()
        thread.join(timeout=5)
        if rec is not None and rec.poll() is None:
            rec.wait(timeout=60)
        n.restore()
    print("marcas", json.dumps(marcas))


def contorno():
    """As poses de câmera de `lista_cameras` (mão vazia), cada uma parada `SEGURA` s (padrão 4), gravadas pela captura PipeWire do gamescope (o contorno do bloco só aparece nela):
    `NOME` (o `gravar-jogo start`), `ARQUIVO` (padrão forma-contorno.mp4). Escreve em `SAIDA` (padrão a pasta de rascunho) o JSON das marcas (a pose e o segundo, contado do começo do `rec`)."""
    h, aj = port()
    import h2_nota_instrumento as n
    cmd, wait = aj.cmd, aj.wait
    nome, arquivo = os.environ["NOME"], os.environ.get("ARQUIVO", "forma-contorno.mp4")
    segura = float(os.environ.get("SEGURA", "7"))
    poses = lista_cameras()
    segundos = int(len(poses) * (segura + 1.5) + 20)
    n.sky()
    cmd("tick unfreeze")
    cmd("gamemode creative @s")
    cmd("clear @s")
    h.ctl("options-set", "chatVisibility", "2")
    n.mute()
    marcas = []
    rec = subprocess.Popen(["ssh", "desktop", f"{n.GRAVAR} rec {nome} {arquivo} {segundos}"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    t0 = time.time()
    time.sleep(4.0)
    try:
        for pose, x, y, z, ax, ay, az in poses:
            w = mundo((x, y, z))
            cmd("tp @s %.4f %.4f %.4f 0 0" % w)
            wait(12)
            h.ctl("look-at", *("%.4f" % v for v in mundo((ax, ay, az))))
            wait(8)
            estado = json.loads(h.ctl("state"))
            jogador = estado["player"]
            time.sleep(segura / 3)
            com = round(time.time() - t0, 1)
            # O mesmo quadro sem o contorno: F1 esconde a interface (e o contorno, se o Horizon seguir o vanilla); a diferença entre os dois quadros é o contorno e a interface, que `comparar_forma.py` tira.
            h.ctl("key", "f1", "down")
            h.ctl("key", "f1", "up")
            time.sleep(segura / 3)
            sem = round(time.time() - t0, 1)
            h.ctl("key", "f1", "down")
            h.ctl("key", "f1", "up")
            marcas.append((pose, com, sem, jogador["position"], jogador["eye"], round(jogador["yaw"], 4), round(jogador["pitch"], 4)))
            time.sleep(segura / 3)
        out, err = rec.communicate(timeout=segundos + 120)
        print(out, err, flush=True)
    finally:
        if rec.poll() is None:
            rec.wait(timeout=120)
    json.dump(marcas, open(os.environ.get("SAIDA", "forma-contorno-marcas.json"), "w"), indent=1)
    for m in marcas:
        print(m)



def comparar(port_json, latest):
    dados = json.load(open(port_json))
    o = dados["origem"]
    # O original: `cena: item NOME x y z` e `cena: pessoa NOME x y z`, em coordenadas absolutas; a origem dele sai da linha `cena: origem X Y Z`.
    ori = {}
    origem_o = None
    for linha in open(latest, errors="replace"):
        m = re.search(r"cena: origem (-?\d+) (-?\d+) (-?\d+)", linha)
        if m:
            origem_o = tuple(int(v) for v in m.groups())
        m = re.search(r"cena: (item|pessoa|flecha) (\S+) (-?[0-9.E-]+) (-?[0-9.E-]+) (-?[0-9.E-]+)", linha)
        if m:
            ori[m.group(2)] = tuple(float(m.group(i)) for i in (3, 4, 5))
    chaves = dados.get("itens") or dados.get("pessoas")
    iguais = diferentes = faltando = 0
    for nome, pos in chaves.items():
        if nome.replace(".", "_") in ori:
            nome_o = nome.replace(".", "_")
        else:
            nome_o = nome
        if pos is None or nome_o not in ori:
            faltando += 1
            print(f"FALTA {nome}: port {pos} original {ori.get(nome_o)}")
            continue
        rel_p = tuple(round(pos[i] - o[i], 4) for i in range(3))
        rel_o = tuple(round(ori[nome_o][i] - origem_o[i], 4) for i in range(3))
        ok = all(abs(rel_p[i] - rel_o[i]) < 0.0006 for i in range(3))
        iguais += ok
        diferentes += not ok
        print(("OK    " if ok else "DIFF  ") + f"{nome:24} port {rel_p} original {rel_o}")
    print(f"iguais {iguais}, diferentes {diferentes}, faltando {faltando}")


if __name__ == "__main__":
    modo = sys.argv[1]
    if modo == "gerar":
        gerar()
    elif modo == "montar":
        montar()
    elif modo == "itens":
        itens(sys.argv[2])
    elif modo == "flechas":
        flechas(sys.argv[2])
    elif modo == "pessoa":
        pessoa(sys.argv[2])
    elif modo == "repete":
        # `repete NOME N`: solta o item NOME de `lista_itens` N vezes, um por vez, e conta os y de repouso (a queda é por tempo e por thread: o resultado tem de ser sempre o mesmo).
        pontos = {n: (x, y, z) for n, x, y, z in lista_itens()}
        contagem = {}
        for _ in range(int(sys.argv[3])):
            y = queda(pontos[sys.argv[2]], 30, "zr")
            contagem[y] = contagem.get(y, 0) + 1
        print(sys.argv[2], contagem)
    elif modo == "bordas":
        print(json.dumps(bordas()))
    elif modo == "borda_pull":
        # Para a reabertura: põe o `PULL` no lado leste do cabo da borda (dois cliques) e salva.
        for _ in range(2):
            clique_borda()
            port()[1].wait(6)
        print("borda_pull", vista_borda(), queda())
    elif modo == "borda_queda":
        print("borda_queda", vista_borda(), queda())
    elif modo == "montar_video":
        montar_video()
    elif modo == "video":
        video()
    elif modo == "contorno":
        contorno()
    elif modo == "comparar":
        comparar(sys.argv[2], sys.argv[3])
    else:
        print(__doc__)
        sys.exit(2)
