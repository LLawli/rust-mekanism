#!/usr/bin/env python3
"""A 11.ª rodada da frente `tanques` em jogo: a luz do Fluid Tank pelo fluido (`BlockFluidTank.getLightEmission` e `TileEntityFluidTank.checkLight`), no port e no
cliente do original, com os mesmos comandos.

    H_NAME=horizon h2_tanques11.py cena [montar] [lava] [niveis] [apaga] [volta] [limpar] [print:NOME] [f1]   (o port, com o jogo aberto)
    h2_tanques11.py roteiro > tanques11.json                                                       (o roteiro da `CenaBlocos` do original)
    h2_tanques11.py original oficial.log                                                           (lê o log do original e compara com as leituras do port)

A cena é um corredor fechado de pedra lisa, sem luz do céu, com três salas separadas por paredes: na primeira um Fluid Tank básico que recebe lava, na segunda um com
água e na terceira um vazio. `niveis` lê a luz em 14 posições por `execute if predicate` com `location_check` de luz, um valor de cada vez (a luz do mundo do servidor, nos dois
jogos); no port lê também a do mundo do cliente, por `horizon-ctl light`. `apaga` tira a lava com o servidor parado e anda tick a tick, lendo se a posição ao lado do tanque tem luz: o `checkLight` apaga a luz
`blockDeactivationDelay` ticks depois de a escala chegar a zero. `volta` põe a lava de novo e lê o tick em que a luz volta.
"""
import json, os, re, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h
import h2_tanques9 as t9

LEITURAS = os.path.join(os.environ.get("LEITURAS_DIR", "."), "luz_%s.json")
# O corredor: o interior vai de (0, 0, 0) a (14, 2, 2) a partir da base, com paredes em x = 9 e x = 12.
LAVA, AGUA, VAZIO = (0, 0, 1), (10, 0, 1), (13, 0, 1)
POSICOES = [(x, 0, 1) for x in range(0, 9)] + [(0, 1, 1), (0, 2, 1), (10, 0, 1), (11, 0, 1), (14, 0, 1)]
SONDA = (1, 0, 1)
# Os ticks andados de uma vez depois de tirar a lava, e os lidos um a um depois deles.
SALTO, PASSOS = 85, 35
VOLTA = 8
CHEIO = 32000


def at(game, p):
    bx, by, bz = t9.BASE[game]
    return "%d %d %d" % (bx + p[0], by + p[1], bz + p[2])


def camera(game):
    bx, by, bz = t9.BASE[game]
    return "%.1f %.1f %.1f 90 0" % (bx + 7.5, by, bz + 1.5)


def caixa(game):
    bx, by, bz = t9.BASE[game]
    return (bx - 1, by - 1, bz - 1, bx + 15, by + 3, bz + 3)


def predicado(game, p, minimo, maximo):
    return 'execute positioned %s if predicate {type:"minecraft:location_check",predicate:{light:{light:{min:%d,max:%d}}}}' % (at(game, p), minimo, maximo)


def tanque(game, p, fluido="", mb=0):
    nbt = t9.contents(game, "fluid", fluido, mb)
    return "setblock %s mekanism:basic_fluid_tank%s" % (at(game, p), "{%s}" % nbt if nbt else "")


def conteudo(game, p, fluido="", mb=0):
    if game == "port":
        return "data merge block %s {%s}" % (at(game, p), t9.contents(game, "fluid", fluido, mb))
    if mb == 0:
        # `IResourceContainer.deserialize`: a entrada sem `stored` esvazia o tanque (sem a lista o tanque ficaria como está).
        return "data merge block %s {fluid_tanks:[{container:0}]}" % at(game, p)
    return "data merge block %s {%s}" % (at(game, p), t9.contents(game, "fluid", fluido, mb))


def montar_cmds(game):
    x1, y1, z1, x2, y2, z2 = caixa(game)
    bx, by, bz = t9.BASE[game]
    return [
        "tick unfreeze",
        "fill %d %d %d %d %d %d minecraft:smooth_stone hollow" % (x1, y1, z1, x2, y2, z2),
        "fill %d %d %d %d %d %d minecraft:smooth_stone" % (bx + 9, by, bz, bx + 9, by + 2, bz + 2),
        "fill %d %d %d %d %d %d minecraft:smooth_stone" % (bx + 12, by, bz, bx + 12, by + 2, bz + 2),
        "kill @e[type=item]",
        tanque(game, LAVA),
        tanque(game, AGUA, "minecraft:water", CHEIO),
        tanque(game, VAZIO),
        "#esperar 40",
    ]


def lava_cmds(game):
    return [conteudo(game, LAVA, "minecraft:lava", CHEIO), "#esperar 40"]


def video_cmds(game):
    """O roteiro do vídeo, com o servidor solto: a lava entra e o corredor acende; o tanque esvazia e a luz apaga uns 5 s depois (a escala desce e o
    `blockDeactivationDelay` corre); a lava volta; o tanque é tirado e a luz some na hora; um tanque com lava é posto e acende."""
    return [
        "#esperar 40",
        conteudo(game, LAVA, "minecraft:lava", CHEIO), "#esperar 80",
        conteudo(game, LAVA), "#esperar 170",
        conteudo(game, LAVA, "minecraft:lava", CHEIO), "#esperar 80",
        "setblock %s minecraft:air" % at(game, LAVA), "#esperar 60",
        tanque(game, LAVA, "minecraft:lava", CHEIO), "#esperar 80",
    ]


def niveis_cmds(game):
    cmds = []
    for i, p in enumerate(POSICOES):
        cmds.append("say T11 nivel %d" % i)
        cmds += [predicado(game, p, v, v) for v in range(16)]
    return cmds


def sonda_cmds(game, fase, k):
    return ["say T11 %s %d" % (fase, k), predicado(game, SONDA, 1, 15)]


def apaga_cmds(game):
    cmds = ["tick freeze", "#esperar 10", conteudo(game, LAVA), "tick step %d" % SALTO, "#esperar %d" % (SALTO + 30)]
    for k in range(SALTO, SALTO + PASSOS):
        cmds += sonda_cmds(game, "apaga", k) + ["tick step 1", "#esperar 4"]
    return cmds


def volta_cmds(game):
    cmds = [conteudo(game, LAVA, "minecraft:lava", CHEIO)]
    for k in range(VOLTA):
        cmds += sonda_cmds(game, "volta", k) + ["tick step 1", "#esperar 4"]
    return cmds + ["tick unfreeze", "#esperar 20"]


def limpar_cmds(game):
    return ["tick unfreeze", t9.CLEAR.format(*caixa(game)), "kill @e[type=item]"]


def roteiro():
    game = "original"
    tp = camera(game)
    longe = t9.VIDEO_TP[game]
    shots = [
        {"name": "t11_escuro", "cmds": montar_cmds(game), "tp": tp},
        {"name": "t11_lava", "cmds": video_cmds(game), "tp": tp},
        {"name": "t11_vazio", "cmds": apaga_cmds(game), "tp": tp},
        {"name": "t11_volta", "cmds": volta_cmds(game), "tp": tp},
        {"name": "t11_niveis", "cmds": niveis_cmds(game), "tp": tp},
        {"name": "t11_limpeza", "cmds": limpar_cmds(game), "tp": longe},
    ]
    setup = ["tp @a " + longe, "difficulty peaceful", "time set 6000", "weather clear", "gamerule advance_time false", "gamerule advance_weather false",
             "gamemode creative @a", "effect clear @a"]
    return {"setup": setup, "clear": "weather clear", "hud": False, "shots": shots}


def resumo(readings):
    niveis = [readings.get("nivel", {}).get(str(i)) for i in range(len(POSICOES))]
    apaga = readings.get("apaga", {})
    volta = readings.get("volta", {})
    escuro = next((int(k) for k in sorted(apaga, key=int) if not apaga[k]), None)
    aceso = next((int(k) for k in sorted(volta, key=int) if volta[k]), None)
    return niveis, escuro, aceso


def original(log):
    """Depois de cada `T11 nivel I` vêm 16 respostas (`Test passed` no valor da luz); depois de `T11 apaga K` e `T11 volta K`, uma (se a sonda tem luz)."""
    readings, fase, k, vistos = {}, None, None, []
    for line in open(log, errors="replace"):
        said = re.search(r"T11 (\S+) (\d+)\s*$", line.rstrip("\n")) if "T11 " in line else None
        if said:
            fase, k, vistos = said.group(1), said.group(2), []
            continue
        found = re.search(r"System chat: Test (passed|failed)", line)
        if not found or fase is None:
            continue
        vistos.append(found.group(1) == "passed")
        if fase == "nivel" and len(vistos) == 16:
            readings.setdefault(fase, {})[k] = vistos.index(True) if vistos.count(True) == 1 else None
            fase = None
        elif fase in ("apaga", "volta"):
            readings.setdefault(fase, {})[k] = vistos[0]
            fase = None
    json.dump(readings, open(LEITURAS % "original", "w"), indent=1, sort_keys=True)
    try:
        port = json.load(open(LEITURAS % "port"))
    except FileNotFoundError:
        print("sem as leituras do port")
        return 1
    a, b = resumo(port), resumo(readings)
    cliente = resumo({f: port.get(f + "_cliente", {}) for f in ("nivel", "apaga", "volta")})
    print("   o mundo do cliente do port (`horizon-ctl light`): niveis %s, sem luz no tick %s, de volta no tick %s" % cliente)
    diffs = 0
    for i, p in enumerate(POSICOES):
        igual = a[0][i] == b[0][i]
        diffs += not igual
        print("   luz em %s: port %s original %s%s" % (p, a[0][i], b[0][i], "" if igual else "  DIFERE"))
    for fase in ("apaga", "volta"):
        ks = sorted(set(port.get(fase, {})) | set(readings.get(fase, {})), key=int)
        d = [k for k in ks if port.get(fase, {}).get(k) != readings.get(fase, {}).get(k)]
        diffs += len(d)
        print("   %s: %d leituras, %d diferenças %s" % (fase, len(ks), len(d), d[:8]))
    print("   a sonda fica sem luz no tick: port %s original %s" % (a[1], b[1]))
    print("   a sonda volta a ter luz no tick: port %s original %s" % (a[2], b[2]))
    print("luz do Fluid Tank: %d diferenças entre o port e o original" % diffs)
    return diffs


def c(text):
    return " | ".join(h.command(text))


def luz(game, p):
    got = json.loads(h.ctl("light", *at(game, p).split(), *at(game, p).split()))["block"]
    return None if got == "." else int(got, 16)


def cena(parts):
    game = "port"
    try:
        readings = json.load(open(LEITURAS % game))
    except FileNotFoundError:
        readings = {}

    def run(cmds):
        # Cada leitura sai duas vezes: a do mundo do servidor, pelo mesmo `execute if predicate` do original (em `leituras[fase]`), e a do mundo do cliente, por
        # `horizon-ctl light` (em `leituras[fase + "_cliente"]`).
        fase = k = None
        vistos = []
        for cmd in cmds:
            if cmd.startswith("#esperar "):
                time.sleep(int(cmd.split()[1]) / 20.0)
            elif cmd.startswith("say T11 "):
                _, _, fase, k = cmd.split()
                vistos = []
            elif cmd.startswith("execute positioned ") and fase:
                vistos.append("passed" in c(cmd).lower())
                if fase == "nivel" and len(vistos) == 16:
                    readings.setdefault(fase, {})[k] = vistos.index(True) if vistos.count(True) == 1 else None
                    readings.setdefault(fase + "_cliente", {})[k] = luz(game, POSICOES[int(k)])
                elif fase in ("apaga", "volta"):
                    readings.setdefault(fase, {})[k] = vistos[0]
                    readings.setdefault(fase + "_cliente", {})[k] = bool(luz(game, SONDA))
            else:
                out = c(cmd)
                if cmd.startswith(("fill", "setblock", "data")):
                    print("  ", cmd[:60], "->", out[:90], flush=True)
                if cmd.startswith("tick step"):
                    time.sleep(0.25)

    for part in parts:
        print(f"== cena {part}", flush=True)
        if part == "setup":
            for cmd in ["difficulty peaceful", "time set noon", "weather clear", "gamerule advance_time false", "gamerule advance_weather false", "gamemode creative", "effect clear @s"]:
                c(cmd)
        elif part.startswith("print:"):
            name = part.split(":", 1)[1]
            c("tp @s " + camera(game))
            time.sleep(1.5)
            h.ctl("screenshot", f"/tmp/{name}.png")
            pasta = os.environ.get("PASTA_PRINTS", "~/.cache/rust-mekanism/tanques-11/prints")
            subprocess.run(["ssh", "desktop", f"mkdir -p {pasta} && docker exec {os.environ['H_NAME']} cat /tmp/{name}.png > {pasta}/{name}.png"], check=True)
        elif part == "f1":
            h.ctl("key", "f1", "tap")
        elif part == "camera":
            c("tp @s " + camera(game))
        elif part.startswith("esperar:"):
            time.sleep(float(part.split(":")[1]))
        else:
            # Só monta num lugar carregado e vazio: a caixa é conferida antes de qualquer `fill`.
            if part == "montar":
                c("tp @s " + t9.VIDEO_TP[game])
                time.sleep(2.0)
                x1, y1, z1, x2, y2, z2 = caixa(game)
                bx, by, bz = t9.BASE[game]
                livre = c("execute if blocks %d %d %d %d %d %d %d %d %d all" % (x1, y1, z1, x2, y2, z2, x1, y1 + 8, z1))
                ar = [c("execute if block %d %d %d minecraft:air" % p) for p in ((x1, y1, z1), (x2, y2, z2), (bx + 7, by + 1, bz + 1), (x1, y1 + 8, z1), (x2, y2 + 8, z2))]
                print("   a caixa %s é igual à de cima dela: %s; ar nos cantos e no meio: %s" % (caixa(game), livre[:40], [a[:11] for a in ar]), flush=True)
                if "passed" not in livre.lower() or not all("passed" in a.lower() for a in ar):
                    print("   a caixa não está vazia: não monto", flush=True)
                    return 1
            run(globals()[part + "_cmds"](game))
            if part in ("niveis", "apaga", "volta"):
                json.dump(readings, open(LEITURAS % game, "w"), indent=1, sort_keys=True)
                for lado, sufixo in (("servidor", ""), ("cliente", "_cliente")):
                    niveis, escuro, aceso = resumo({f: readings.get(f + sufixo, {}) for f in ("nivel", "apaga", "volta")})
                    print("   %s: niveis %s | sem luz no tick %s | de volta no tick %s" % (lado, niveis, escuro, aceso), flush=True)
    return 0


if __name__ == "__main__":
    if sys.argv[1:2] == ["roteiro"]:
        print(json.dumps(roteiro(), indent=1))
    elif sys.argv[1:2] == ["original"]:
        sys.exit(1 if original(sys.argv[2]) else 0)
    elif sys.argv[1:2] == ["cena"]:
        os.environ.setdefault("H_NAME", "horizon")
        sys.exit(1 if cena(sys.argv[2:]) else 0)
