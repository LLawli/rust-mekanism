#!/usr/bin/env python3
"""Os tubos pressurizados no jogo contra o original (frente `tubo-jogo`, Fase 3, decisões 142 a 144).

    H_NAME=horizon PASTA=<pasta do game.log> h2_tubo_jogo.py quimico CENARIO|todos   # repete o começo dos rastros chemical_*.jsonl tick a tick
    H_NAME=horizon PASTA=<pasta do game.log> h2_tubo_jogo.py carbono                 # carbono de uma fonte por quatro tubos ao Metallurgic Infuser, que faz ferro enriquecido
    H_NAME=horizon PASTA=<pasta do game.log> h2_tubo_jogo.py configurador            # o Configurator cicla um lado do tubo
    H_NAME=horizon PASTA=<pasta do game.log> h2_tubo_jogo.py reabrir montar|conferir # o mundo salvo e reaberto
    NOME=... h2_tubo_jogo.py video                                                   # o vídeo com som

A sonda (`TRANSMITTER_PROBE = true` na cópia de teste, `enviar_fontes.sh`) escreve no stderr do jogo (`PASTA/game.log`): `MTN {t, runs, nets: [[químico, quantidade, capacidade, saiu, tubos,
aceitadores], ...]}` a cada `ServerTickEvent.Post` e `MTV {p, conn, kind, raw, net}` a cada vez que um tubo publica as conexões dele. Na cópia de teste o bloco de carvão é um tanque
criativo cheio de carbono (o Chemical Tank ainda não está portado) e a chave `probe_push` do NBT de um tubo empurra químico para a rede pela capability dele, como um vizinho faria (o
`push` do rastro). O servidor fica parado (`tick freeze`) e anda com `tick step 1`: cada passo dá uma linha `MTN` com `runs` verdadeiro, que se compara com a linha `estate` do rastro do
mesmo tick.

O original é o rastro que o `ChemicalNetworkTraceTest` gravou com o Mekanism de referência (`dev/referencia/rastros/chemical_*.jsonl`, duas execuções): é o que o original fez, tick a
tick. Só vale até a primeira ação que o jogo não tem (um Chemical Tank, o `mode` que o Configurator só faz clicando).
"""
import json, os, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("H_NAME", "horizon")
import h2_identity as h

RASTROS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "rastros")
PASTA = os.environ.get("PASTA", "~/.cache/rust-mekanism/restos")
BASE = (0, 150, 0)
TUBE = {t: f"mekanism:{t}_pressurized_tube" for t in ("basic", "advanced", "elite", "ultimate")}
INFUSER = "mekanism:metallurgic_infuser"
# Cada cenário, até a primeira ação que o jogo não repete (o `tank`, o `source`, o `generator`, o `mode` que o Configurator só faz clicando).
ORDER_DEPENDENT = {"break_end_odd"}
SCENARIOS = {"break_end_odd": None, "split_middle": 25, "tiers": 17, "merge_same": 19}


def cmd(text):
    try:
        return h.command(text)[0]
    except (KeyError, IndexError):
        return ""


def wait(n):
    h.ctl("wait", str(n))


def at(dx, dy=0, dz=0):
    return (BASE[0] + dx, BASE[1] + dy, BASE[2] + dz)


def txt(p):
    return "%d %d %d" % p


def stand(p):
    cmd("tp @s %d %d %d" % p)
    wait(8)


def place(p, block):
    """O rastro põe o tubo com `setBlock` (o `gameTest`, sem item): o roteiro também, que não precisa de um bloco para apoiar o tubo no ar."""
    cmd("setblock %s %s" % (txt(p), block))


def clear():
    cmd("tick unfreeze")
    # O chão da cena: o jogador de pé (o mundo `limpo` tem terra em y 64, e o raio do Configurator pede o jogador perto do tubo).
    cmd("fill %d %d %d %d %d %d minecraft:stone" % (BASE[0] - 6, BASE[1] - 1, BASE[2] - 8, BASE[0] + 42, BASE[1] - 1, BASE[2] + 10))
    cmd("fill %d %d %d %d %d %d minecraft:air" % (BASE[0] - 4, BASE[1], BASE[2] - 6, BASE[0] + 40, BASE[1] + 6, BASE[2] + 8))
    wait(10)


def log_lines(prefix):
    out = subprocess.run(["ssh", "desktop", f"grep -a '^{prefix} ' {PASTA}/game.log | cut -c{len(prefix) + 2}-"], capture_output=True, text=True, timeout=120).stdout
    rows = []
    for line in out.splitlines():
        try:
            rows.append(json.loads(line))
        except ValueError:
            pass
    return rows


def step():
    """Um tick: devolve a linha `MTN` com `runs` verdadeiro que ele deu."""
    before = len(log_lines("MTN"))
    cmd("tick step 1")
    for _ in range(40):
        time.sleep(0.15)
        new = [row for row in log_lines("MTN")[before:] if row.get("runs")]
        if new:
            return new[-1]
    return None


def push(p, chem, amount):
    """O que um vizinho empurra pela capability do tubo: a sonda lê `probe_push` no tick do tubo e insere no contêiner da rede (`EXTERNAL`)."""
    cmd('data merge block %s {probe_push:{chem:"%s",amount:%d}}' % (txt(p), chem, amount))


def rows_of(scenario, run=""):
    name = f"chemical_{run + '_' if run else ''}{scenario}.jsonl"
    return [json.loads(l) for l in open(os.path.join(RASTROS, name)) if l.strip()]


def scenario_nets(rows):
    mine = set()
    for r in rows:
        if r["e"] == "state":
            mine |= {n["net"] for n in r["nets"]} | {c["net"] for c in r["cables"] if c and c.get("net") is not None}
    return mine


def run_chemical(scenario, run=""):
    rows = rows_of(scenario, run)
    mine = scenario_nets(rows)
    stop = SCENARIOS[scenario]
    last = max(r["t"] for r in rows) if stop is None else stop
    marks = {}
    for r in rows:
        if r["e"] == "mark":
            marks.setdefault(r["t"], []).append(r["m"])
    estates = {r["t"]: r for r in rows if r["e"] == "estate"}
    clear()
    cmd("gamemode creative @s")
    diffs = []

    def apply(mark):
        words = mark.removeprefix("pre ").split()
        n = lambda i: int(words[i])
        if words[0] == "place":
            place(at(n(1), 0, n(2)), TUBE[words[3]])
        elif words[0] == "break":
            cmd("setblock %s minecraft:air" % txt(at(n(1), 0, n(2))))
        elif words[0] == "push":
            push(at(n(1), 0, n(2)), words[3], n(4))
        elif words[0] == "finish":
            pass
        else:
            raise SystemExit("ação que o jogo não repete: " + mark)

    cmd("tick freeze")
    for t in range(0, last + 1):
        ms = marks.get(t, [])
        if any(m == "pre finish" for m in ms):
            break
        for m in ms:
            if m.startswith("pre "):
                apply(m)
        mbp = step()
        for m in ms:
            if not m.startswith("pre "):
                apply(m)
        want = estates.get(t)
        if want is None:
            continue
        want_nets = sorted((n["chem"], n["amount"], n["capacity"], n["prev"]) for n in want["nets"] if n["net"] in mine and n["capacity"] > 0)
        got_nets = sorted((n[0], n[1], n[2], n[3]) for n in (mbp or {}).get("nets", []) if n[4] > 0)
        if got_nets != want_nets:
            diffs.append((t, want_nets, got_nets))
    cmd("tick unfreeze")
    print(f"{run or 'run1'} {scenario} (até o tick {last}): {'IGUAL' if not diffs else 'DIFERE em %d ticks' % len(diffs)}")
    for d in diffs[:4]:
        print("   t=%d rastro %s  jogo %s" % d)
    return not diffs


def last_views():
    last = {}
    for v in log_lines("MTV"):
        last[tuple(v["p"])] = v
    return last


def infusion_of(p):
    """O que o tanque de infusão do Infuser em `p` guarda: `(químico, quantidade)`, pelo NBT `chemical_tanks`."""
    import re
    out = cmd("data get block %s chemical_tanks" % txt(p))
    amount = re.search(r"amount: (\d+)", out)
    kind = re.search(r'type: "([^"]+)"', out)
    return (kind.group(1) if kind else "", int(amount.group(1)) if amount else 0)


def items_of(p):
    """O que o Infuser em `p` tem nos slots, como texto do `data get`."""
    return cmd("data get block %s Items" % txt(p))


def alimentar(p, energia=20000):
    """A energia do Infuser (o NBT `energy`): o jogo de teste não tem gerador à mão, e o `data merge` de energia faz o papel do cabo."""
    cmd("data merge block %s {energy:%d}" % (txt(p), energia))


def montar_carbono(com_ferro=False):
    """A fonte (o bloco de carvão da sonda, um tanque criativo de carbono) a oeste do primeiro de quatro tubos, o Metallurgic Infuser a leste do último com o químico em entrada nos seis lados
    (sem o item não há a configuração padrão, decisão 113). Com `com_ferro`, 64 lingotes de ferro no slot de entrada e energia: a máquina faz ferro enriquecido (`iron_ingot` + `carbon`) com o carbono que chega."""
    clear()
    cmd("gamemode creative @s")
    cmd("clear @s")
    cmd("tick unfreeze")
    # A fonte e o Infuser antes dos tubos: o tubo pergunta pela capability do vizinho ao ligar, e um bloco de carvão (que não é block entity) não avisa que apareceu.
    cmd("setblock %s minecraft:coal_block" % txt(at(-1)))
    cmd("setblock %s %s[facing=north]" % (txt(at(4)), INFUSER))
    wait(10)
    config = "component_config:{config2:[I;1,1,1,1,1,1],config3:[I;1,1,1,1,1,1]}"
    items = ',Items:[{Slot:1b,id:"minecraft:iron_ingot",count:64}]' if com_ferro else ""
    cmd("data merge block %s {%s%s}" % (txt(at(4)), config, items))
    wait(10)
    for x in range(4):
        place(at(x), TUBE["basic"])
    wait(40)


def click_arm(cable, side, sneak=True):
    """O clique do Configurator no braço de `side` (leste, oeste ou sul): como um jogador, a mira do `look-at` sobre o meio do braço e o botão direito do mouse (o `interact` mira o centro
    da face do bloco, e a mira de um braço é o que o servidor lê: `getSideLookingAt`)."""
    import h2_ajustes_jogo as a
    if side == "south":
        point, stand = (0.32, 0.5, 0.85), (cable[0] - 2.5, cable[1], cable[2] + 0.85)
    else:
        point = (0.85 if side == "east" else 0.15, 0.5, 0.5)
        # Ao norte e no meio: o raio entra pelo braço, e não pelo centro (de lado ele cruza a face norte do centro antes do braço).
        stand = (cable[0] + 0.5, cable[1] + 1.0, cable[2] - 2.6)
    return a.click_at(cable, point, stand, sneak)


SHOTS = os.environ.get("SHOTS", "~/.cache/rust-mekanism/tubo-jogo-b1/prints")
LOCAL = os.environ.get("LOCAL", os.path.expanduser("~/.cache/rust-mekanism/wa/tubo-jogo"))


def shot(name):
    """Um print do jogo (o `horizon-ctl screenshot` grava dentro do container); copia para o notebook."""
    h.ctl("screenshot", f"/tmp/{name}.png")
    subprocess.run(["ssh", "desktop", f"mkdir -p {SHOTS} && docker exec {os.environ.get('H_NAME', 'horizon')} cat /tmp/{name}.png > {SHOTS}/{name}.png"], check=True)
    os.makedirs(LOCAL, exist_ok=True)
    subprocess.run(["scp", "-q", f"desktop:{SHOTS}/{name}.png", f"{LOCAL}/{name}.png"], check=True)
    return f"{LOCAL}/{name}.png"


def enquadrar(line=3):
    """A câmera: ao norte da linha e um pouco acima, olhando o meio dela."""
    mid = at(line / 2.0 - 0.5)
    cmd("tp @s %.2f %.2f %.2f" % (mid[0] + 0.5, mid[1] + 1.2, mid[2] - 3.5))
    wait(12)
    h.ctl("look-at", "%.2f" % (mid[0] + 0.5), "%.2f" % (mid[1] + 0.5), "%.2f" % (mid[2] + 0.5))
    wait(6)


def crop_diff(a, b, geometry="480x270+720+405"):
    """Os pixels que diferem entre os recortes (o centro da tela, onde está o braço) de dois prints: `compare -metric AE`."""
    outs = []
    for name in (a, b):
        out = f"{name}.corte.png"
        subprocess.run(["magick", name, "-crop", geometry, "+repage", out], check=True)
        outs.append(out)
    result = subprocess.run(["compare", "-metric", "AE", outs[0], outs[1], "null:"], capture_output=True, text=True)
    return int(float((result.stderr or "0").split()[0].split("(")[0]))


def configurador():
    """O Configurator agachado no braço leste do primeiro de dois tubos cicla `PUSH`, `PULL`, `NONE` e depois `NORMAL`; a cada tipo a vista que o tubo publicou (`MBW`) e um print recortado no
    braço. O braço só "some" se o recorte do print mostrar (decisão 138)."""
    clear()
    cmd("gamemode creative @s")
    cmd("clear @s")
    for x in range(2):
        place(at(x), TUBE["basic"])
    wait(40)
    h.ctl("give", "mekanism:configurator", "1")
    middle = at(0)
    enquadrar(2)
    antes = shot("tubo_configurador_antes")
    controle = shot("tubo_configurador_antes_2")
    print("ruído entre dois prints iguais:", crop_diff(antes, controle), "pixels")
    ok = True
    for want in ("push", "pull", "none"):
        for _ in range(10):
            before = (last_views().get(middle) or {}).get("raw", [None] * 6)[5]
            click_arm(middle, "east")
            time.sleep(1.5)
            view = last_views().get(middle)
            if view and view["raw"][5] != before:
                break
        enquadrar(2)
        time.sleep(1.0)
        print_ = shot(f"tubo_configurador_{want}")
        got = view["raw"][5] if view else None
        arm = view["conn"][5] if view else None
        other = last_views().get(at(1))
        other_arm = other["conn"][4] if other else None
        expected_arm = want != "none"
        diff = crop_diff(antes, print_)
        good = got == want and arm == expected_arm and other_arm == expected_arm
        ok &= good
        print(("OK   " if good else "DIFF ") + f"agachado no braço leste: tipo cru {got} (esperado {want}), braço do meio {arm}, braço do vizinho {other_arm}; o recorte do print difere {diff} pixels do print de antes")
    # Sem braço não há o que mirar: o clique cai na face leste (o lado clicado, como o original quando `getSideLookingAt` é nulo) e volta a `NORMAL`. O vizinho sai e volta para a face ficar à vista.
    cmd("setblock %s minecraft:air" % txt(at(1)))
    wait(10)
    stand((middle[0] + 3.5, middle[1], middle[2] + 0.5))
    h.ctl("select", "mekanism:configurator")
    h.ctl("key", "shift", "down")
    wait(3)
    h.ctl("interact", *map(str, middle), "east")
    h.ctl("key", "shift", "up")
    wait(6)
    time.sleep(1.5)
    place(at(1), TUBE["basic"])
    wait(30)
    enquadrar(2)
    time.sleep(1.5)
    depois = shot("tubo_configurador_normal")
    view = last_views().get(middle)
    other = last_views().get(at(1))
    good = bool(view) and view["raw"][5] == "normal" and view["conn"][5] and bool(other) and other["conn"][4]
    ok &= good
    print(("OK   " if good else "DIFF ") + f"agachado na face leste sem braço: tipo cru {view and view['raw'][5]} (esperado normal), os braços voltam {view and view['conn'][5]} {other and other['conn'][4]}; o recorte difere {crop_diff(antes, depois)} pixels do print de antes")
    return ok


def reabrir_montar():
    """`chemical_reopen.jsonl` no jogo, a primeira metade: cinco tubos (o do meio avançado), 12.001 de hidrogênio empurrados no primeiro (a rede de 32.000), o `stored` de cada cano, e o mundo salvo.
    Quem chama fecha o jogo, abre o mundo de novo e roda `reabrir conferir`."""
    clear()
    cmd("gamemode creative @s")
    cmd("clear @s")
    cmd("tick unfreeze")
    for x, tier in enumerate(["basic", "basic", "advanced", "basic", "basic"]):
        place(at(x), TUBE[tier])
    wait(40)
    push(at(0), "mekanism:hydrogen", 12001)
    wait(40)
    nets = log_lines("MTN")[-1]["nets"]
    print("antes de fechar:", nets)
    for x in range(5):
        print("   stored", x, cmd("data get block %s stored" % txt(at(x))))
    h.ctl("save")
    wait(20)
    return nets == [["mekanism:hydrogen", 12001, 32000, 0, 5, 0]]


def reabrir_conferir():
    """A segunda metade: o mundo reaberto tem a rede com o mesmo hidrogênio e a capacidade de antes (12.001 e 32.000, as shares de 5 tubos somando o mesmo), e os canos ligados."""
    wait(120)
    nets = log_lines("MTN")[-1]["nets"]
    views = last_views()
    arms = [views.get(at(x), {}).get("conn") for x in range(5)]
    print("depois de reabrir:", nets)
    good = nets == [["mekanism:hydrogen", 12001, 32000, 0, 5, 0]] and all(arms)
    print(("OK   " if good else "DIFF ") + "a rede reaberta tem o hidrogênio e a capacidade de antes, e os tubos continuam ligados")
    return good


def carbono():
    """O carbono sai da fonte por um tubo em `PULL`, anda os quatro tubos e chega ao tanque de infusão do Metallurgic Infuser: a rede enche e o tanque recebe só o que uma receita usa. Depois
    a máquina recebe ferro e energia e faz ferro enriquecido (`iron_ingot` + `carbon`) com o carbono que chega."""
    montar_carbono()
    h.ctl("give", "mekanism:configurator", "1")
    first = at(0)
    if not pull_oeste(first):
        print("DIFF o Configurator não pôs o lado oeste do primeiro tubo em PULL")
        return False
    wait(120)
    nets = log_lines("MTN")[-1]["nets"]
    tank = infusion_of(at(4))
    print("rede", nets, "tanque de infusão", tank)
    good = tank == ("mekanism:carbon", 1000)
    print(("OK   " if good else "DIFF ") + f"o carbono chegou ao tanque de infusão do Infuser ({tank[1]} mB, o tanque guarda 1.000)")
    # Hidrogênio não tem receita no Infuser: uma linha à parte (z 6) com um Infuser vazio recebe 3.000 de hidrogênio pela capability de um tubo. A rede o guarda, e o tanque segue vazio.
    cmd("setblock %s %s[facing=north]" % (txt(at(4, 0, 6)), INFUSER))
    wait(10)
    cmd("data merge block %s {component_config:{config2:[I;1,1,1,1,1,1],config3:[I;1,1,1,1,1,1]}}" % txt(at(4, 0, 6)))
    wait(10)
    for x in range(4):
        place(at(x, 0, 6), TUBE["basic"])
    wait(40)
    push(at(0, 0, 6), "mekanism:hydrogen", 3000)
    wait(60)
    hydrogen = [net for net in log_lines("MTN")[-1]["nets"] if net[0] == "mekanism:hydrogen"]
    kept = infusion_of(at(4, 0, 6))
    ok2 = bool(hydrogen) and hydrogen[0][1] > 0 and hydrogen[0][5] == 1 and kept == ("", 0)
    print(("OK   " if ok2 else "DIFF ") + f"o hidrogênio ficou na rede {hydrogen} (o Infuser é um aceitador) e o tanque de infusão seguiu vazio {kept}")
    # A máquina de verdade: ferro no slot de entrada, energia, e o ferro enriquecido sai (`iron_ingot` + `carbon`).
    cmd("data merge block %s {Items:[{Slot:1b,id:\"minecraft:iron_ingot\",count:64}]}" % txt(at(4)))
    steel = 0
    for _ in range(40):
        alimentar(at(4))
        wait(20)
        out = items_of(at(4))
        import re
        m = re.search(r"count: (\d+), Slot: 2b, id: \"mekanism:enriched_iron\"", out)
        if m:
            steel = int(m.group(1))
            if steel >= 2:
                break
    tank = infusion_of(at(4))
    print("ferro enriquecido na saída:", steel, "tanque", tank)
    ok3 = steel >= 1
    print(("OK   " if ok3 else "DIFF ") + f"o Infuser fez {steel} de ferro enriquecido com o carbono do tubo")
    return good and ok2 and ok3


def pull_oeste(tube):
    """O Configurator agachado no braço oeste de `tube` até `PULL` (a fonte fica a oeste)."""
    view = None
    for _ in range(12):
        click_arm(tube, "west")
        time.sleep(1.2)
        view = last_views().get(tube)
        if view and view["raw"][4] == "pull":
            return True
    return False


def montar_cena():
    """A linha de carbono (z 0: a fonte, quatro tubos com o primeiro em `PULL`, o Infuser com ferro e energia) e uma linha de hidrogênio (z 3: quatro tubos de outro tier, sem consumidor),
    cada uma com a sua rede. A ponte (dois tubos em x 1, z 1 e 2) é posta no roteiro."""
    clear()
    cmd("gamemode creative @s")
    cmd("clear @s")
    cmd("tick unfreeze")
    cmd("setblock %s minecraft:coal_block" % txt(at(-1, 0, 0)))
    cmd("setblock %s %s[facing=north]" % (txt(at(4, 0, 0)), INFUSER))
    wait(20)
    cmd("data merge block %s {component_config:{config2:[I;1,1,1,1,1,1],config3:[I;1,1,1,1,1,1]},Items:[{Slot:1b,id:\"minecraft:iron_ingot\",count:64}]}" % txt(at(4, 0, 0)))
    wait(10)
    for x in range(4):
        place(at(x, 0, 0), TUBE["basic"])
        place(at(x, 0, 3), TUBE["advanced"])
    wait(40)
    h.ctl("give", "mekanism:configurator", "1")
    ok = pull_oeste(at(0, 0, 0))
    cmd("clear @s")
    push(at(0, 0, 3), "mekanism:hydrogen", 12000)
    wait(80)
    return ok


def enquadrar_cena():
    """A câmera do vídeo: ao norte e acima, olhando o meio da cena."""
    cmd("tp @s %.2f %.2f %.2f" % (BASE[0] + 2.0, BASE[1] + 2.6, BASE[2] - 3.5))
    wait(12)
    h.ctl("look-at", "%.2f" % (BASE[0] + 2.0), "%.2f" % (BASE[1] + 0.0), "%.2f" % (BASE[2] + 1.5))
    wait(6)


def roteiro(pausa):
    """O que o vídeo mostra: o carbono saindo da fonte pela linha até o Infuser, que faz ferro enriquecido (a energia vem do `data merge`); o tubo do meio quebrado (a rede se parte, o Infuser
    para) e reposto; o Configurator no tubo do meio, com o lado leste em `NONE` (o braço some); e a ponte entre a linha de carbono e a de hidrogênio. O tubo sai como o jogador o quebra
    (`break`, para o som do bloco ir ao vídeo) e entra por `setblock`."""
    def por(p):
        # O `place` com o item não pôs o tubo nesta cena; o roteiro usa `setblock`, e só a quebra tem som.
        cmd("setblock %s mekanism:basic_pressurized_tube" % txt(p))

    enquadrar_cena()
    alimentar(at(4, 0, 0))
    shot("tubo_linha_carbono")
    pausa(5)
    alimentar(at(4, 0, 0))
    h.ctl("break", *map(str, at(2, 0, 0)))
    pausa(1)
    shot("tubo_meio_quebrado")
    pausa(4)
    por(at(2, 0, 0))
    alimentar(at(4, 0, 0))
    pausa(1)
    shot("tubo_meio_reposto")
    pausa(4)
    h.ctl("give", "mekanism:configurator", "1")
    middle = at(1, 0, 0)
    for _ in range(6):
        click_arm(middle, "east")
        view = last_views().get(middle)
        if view and view["raw"][5] == "none":
            break
    pausa(1)
    shot("tubo_meio_none")
    alimentar(at(4, 0, 0))
    pausa(4)
    enquadrar_cena()
    por(at(1, 0, 1))
    por(at(1, 0, 2))
    pausa(2)
    shot("tubo_ponte")
    pausa(4)


def video():
    """O vídeo com som: `NOME` (o `gravar-jogo start`), `SEGUNDOS` (padrão 50), `ARQUIVO` (padrão tubo.mp4). O volume mestre só sobe depois de o stream do container estar no sink nulo."""
    import threading
    import h2_nota_instrumento as n
    nome, segundos, arquivo = os.environ["NOME"], os.environ.get("SEGUNDOS", "50"), os.environ.get("ARQUIVO", "tubo.mp4")
    n.sky()
    cmd("tick unfreeze")
    print("cena montada, PULL nos dois:", montar_cena())
    n.mute()
    h.ctl("options-set", "chatVisibility", "2")
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
        roteiro(lambda s: time.sleep(s))
        out, err = rec.communicate(timeout=int(segundos) + 90)
        print(out, err, flush=True)
    finally:
        n.mute()
        parar.set()
        thread.join(timeout=5)
        if rec is not None and rec.poll() is None:
            rec.wait(timeout=60)
        h.ctl("close")
        n.restore()


def main():
    what = sys.argv[1] if len(sys.argv) > 1 else "quimico"
    if what == "quimico":
        names = list(SCENARIOS) if len(sys.argv) < 3 or sys.argv[2] == "todos" else [sys.argv[2]]
        ok = True
        for name in names:
            results = [run_chemical(name, run) for run in ("", "run2")]
            # Qual metade fica com a unidade ímpar: o original dá resultados diferentes nas duas execuções (a ordem em que o chunk devolve os block entities e os hashes, decisões 101 e 131). No
            # jogo os tubos entram na ordem em que são postos, e basta bater com uma das duas (decisão 144).
            ok &= any(results) if name in ORDER_DEPENDENT else all(results)
        sys.exit(0 if ok else 1)
    if what == "video":
        video()
        return
    if what == "cena":
        print(montar_cena())
        roteiro(lambda t: time.sleep(t))
        return
    if what == "carbono":
        sys.exit(0 if carbono() else 1)
    if what == "reabrir":
        part = sys.argv[2] if len(sys.argv) > 2 else "montar"
        sys.exit(0 if (reabrir_montar() if part == "montar" else reabrir_conferir()) else 1)
    if what == "configurador":
        sys.exit(0 if configurador() else 1)
    raise SystemExit("uso: h2_tubo_jogo.py quimico [CENARIO|todos] | carbono | configurador | reabrir | video")


if __name__ == "__main__":
    main()
