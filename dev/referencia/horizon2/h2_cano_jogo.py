#!/usr/bin/env python3
"""Os canos mecânicos no jogo contra o original (frente `cano-jogo`, Fase 3, decisões 136 a 138).

    H_NAME=horizon PASTA=<pasta do game.log> h2_cano_jogo.py fluido CENARIO|todos   # repete o começo dos rastros fluid_*.jsonl tick a tick
    H_NAME=horizon PASTA=<pasta do game.log> h2_cano_jogo.py lava                    # lava de uma fonte por quatro canos ao Heat Generator, que acende e queima
    H_NAME=horizon PASTA=<pasta do game.log> h2_cano_jogo.py configurador            # o Configurator cicla um lado do cano
    H_NAME=horizon PASTA=<pasta do game.log> h2_cano_jogo.py reabrir montar|conferir # o mundo salvo e reaberto
    NOME=... h2_cano_jogo.py video                                                   # o vídeo com som

A sonda (`TRANSMITTER_PROBE = true` na cópia de teste, `enviar_fontes.sh`) escreve no stderr do jogo (`PASTA/game.log`): `MBP {t, runs, nets: [[fluido, quantidade, capacidade, saiu, canos,
aceitadores], ...]}` a cada `ServerTickEvent.Post` e `MBW {p, conn, kind, raw, net}` a cada vez que um cano publica as conexões dele. Na cópia de teste o caldeirão de lava é um tanque
criativo cheio de lava (o Fluid Tank ainda não está portado) e a chave `probe_push` do NBT de um cano empurra fluido para a rede pela capability dele, como um vizinho faria (o `push` do
rastro). O servidor fica parado (`tick freeze`) e anda com `tick step 1`: cada passo dá uma linha `MBP` com `runs` verdadeiro, que se compara com a linha `estate` do rastro do mesmo tick.

O original é o rastro que o `FluidNetworkTraceTest` gravou com o Mekanism de referência (`dev/referencia/rastros/fluid_*.jsonl`, duas execuções): é o que o original fez, tick a tick. Só
vale até a primeira ação que o jogo não tem (um Fluid Tank, o Heat Generator com a vazão de 100 por tick do rastro).
"""
import json, os, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("H_NAME", "horizon")
import h2_identity as h

RASTROS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "rastros")
PASTA = os.environ.get("PASTA", "~/.cache/rust-mekanism/restos")
BASE = (0, 150, 0)
PIPE = {t: f"mekanism:{t}_mechanical_pipe" for t in ("basic", "advanced", "elite", "ultimate")}
GENERATOR = "mekanismgenerators:heat_generator"
# Cada cenário, até a primeira ação que o jogo não repete (o `tank`, o `source`, o `generator`, o `mode` que o Configurator só faz clicando).
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
    """O rastro põe o cano com `setBlock` (o `gameTest`, sem item): o roteiro também, que não precisa de um bloco para apoiar o cano no ar."""
    cmd("setblock %s %s" % (txt(p), block))


def clear():
    cmd("tick unfreeze")
    # O chão da cena: o jogador de pé (o mundo `limpo` tem terra em y 64, e o raio do Configurator pede o jogador perto do cano).
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
    """Um tick: devolve a linha `MBP` com `runs` verdadeiro que ele deu."""
    before = len(log_lines("MBP"))
    cmd("tick step 1")
    for _ in range(40):
        time.sleep(0.15)
        new = [row for row in log_lines("MBP")[before:] if row.get("runs")]
        if new:
            return new[-1]
    return None


def push(p, fluid, amount):
    """O que um vizinho empurra pela capability do cano: a sonda lê `probe_push` no tick do cano e insere no contêiner da rede (`EXTERNAL`)."""
    cmd('data merge block %s {probe_push:{fluid:"%s",amount:%d}}' % (txt(p), fluid, amount))


def rows_of(scenario, run=""):
    name = f"fluid_{run + '_' if run else ''}{scenario}.jsonl"
    return [json.loads(l) for l in open(os.path.join(RASTROS, name)) if l.strip()]


def scenario_nets(rows):
    mine = set()
    for r in rows:
        if r["e"] == "state":
            mine |= {n["net"] for n in r["nets"]} | {c["net"] for c in r["cables"] if c and c.get("net") is not None}
    return mine


def run_fluid(scenario, run=""):
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
            place(at(n(1), 0, n(2)), PIPE[words[3]])
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
        want_nets = sorted((n["fluid"], n["amount"], n["capacity"], n["prev"]) for n in want["nets"] if n["net"] in mine and n["capacity"] > 0)
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
    for v in log_lines("MBW"):
        last[tuple(v["p"])] = v
    return last


def lava_of(p):
    out = cmd("data get block %s machinery_basis" % txt(p))
    if "amount:" not in out:
        return 0
    try:
        return int(out.split("amount:")[1].split("L")[0])
    except ValueError:
        return 0


def montar_lava():
    """A fonte (o caldeirão de lava da sonda) a oeste do primeiro de quatro canos, o Heat Generator a leste do último, com uma Enrichment Chamber ao lado dele para a energia ter para onde ir."""
    clear()
    cmd("gamemode creative @s")
    cmd("clear @s")
    cmd("tick unfreeze")
    # A fonte e o gerador antes dos canos: o cano pergunta pela capability do vizinho ao ligar, e um caldeirão (que não é block entity) não avisa que apareceu.
    cmd("setblock %s minecraft:lava_cauldron" % txt(at(-1)))
    cmd("setblock %s %s[facing=north]" % (txt(at(4)), GENERATOR))
    cmd("setblock %s mekanism:enrichment_chamber" % txt(at(4, 0, 1)))
    wait(10)
    # O buffer do gerador guarda 240 (`generator-storage.toml`): sem um vizinho que leve a energia ele enche e para de queimar. A Enrichment Chamber, com minério de ferro e os seis
    # lados em entrada (sem o item não há a configuração padrão, decisão 113), gasta a energia moendo.
    cmd("data merge block %s {component_config:{config0:[I;1,1,1,1,1,1]},Items:[{Slot:0b,id:\"minecraft:iron_ore\",count:64}]}" % txt(at(4, 0, 1)))
    wait(10)
    for x in range(4):
        place(at(x), PIPE["basic"])
    wait(40)


def click_arm(pipe, side, sneak=True):
    """O clique do Configurator agachado no braço de `side` (leste ou oeste) do cano: o jogador fica ao norte e de lado, e o raio acerta a caixa do braço."""
    local_x = -2.3 if side == "east" else 3.3
    cmd("tp @s %.2f %.2f %.2f" % (pipe[0] + local_x, pipe[1], pipe[2] - 2.5))
    wait(10)
    h.ctl("select", "mekanism:configurator")
    if sneak:
        h.ctl("key", "shift", "down")
        wait(3)
    out = h.ctl("interact", *map(str, pipe), "north")
    if sneak:
        h.ctl("key", "shift", "up")
    wait(6)
    return out


SHOTS = os.environ.get("SHOTS", "~/.cache/rust-mekanism/cano-jogo-b1/prints")
LOCAL = os.environ.get("LOCAL", os.path.expanduser("~/.cache/rust-mekanism/wa/cano-jogo"))


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
    """O Configurator agachado no braço leste do primeiro de dois canos cicla `PUSH`, `PULL`, `NONE` e depois `NORMAL`; a cada tipo a vista que o cano publicou (`MBW`) e um print recortado no
    braço. O braço só "some" se o recorte do print mostrar (decisão 138)."""
    clear()
    cmd("gamemode creative @s")
    cmd("clear @s")
    for x in range(2):
        place(at(x), PIPE["basic"])
    wait(40)
    h.ctl("give", "mekanism:configurator", "1")
    middle = at(0)
    enquadrar(2)
    antes = shot("cano_configurador_antes")
    controle = shot("cano_configurador_antes_2")
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
        print_ = shot(f"cano_configurador_{want}")
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
    place(at(1), PIPE["basic"])
    wait(30)
    enquadrar(2)
    time.sleep(1.5)
    depois = shot("cano_configurador_normal")
    view = last_views().get(middle)
    other = last_views().get(at(1))
    good = bool(view) and view["raw"][5] == "normal" and view["conn"][5] and bool(other) and other["conn"][4]
    ok &= good
    print(("OK   " if good else "DIFF ") + f"agachado na face leste sem braço: tipo cru {view and view['raw'][5]} (esperado normal), os braços voltam {view and view['conn'][5]} {other and other['conn'][4]}; o recorte difere {crop_diff(antes, depois)} pixels do print de antes")
    return ok


def reabrir_montar():
    """`fluid_reopen.jsonl` no jogo, a primeira metade: cinco canos (o do meio avançado), 12.001 de água empurrados no primeiro (a rede de 16.000), o `stored` de cada cano, e o mundo salvo.
    Quem chama fecha o jogo, abre o mundo de novo e roda `reabrir conferir`."""
    clear()
    cmd("gamemode creative @s")
    cmd("clear @s")
    cmd("tick unfreeze")
    for x, tier in enumerate(["basic", "basic", "advanced", "basic", "basic"]):
        place(at(x), PIPE[tier])
    wait(40)
    push(at(0), "minecraft:water", 12001)
    wait(40)
    nets = log_lines("MBP")[-1]["nets"]
    print("antes de fechar:", nets)
    for x in range(5):
        print("   stored", x, cmd("data get block %s stored" % txt(at(x))))
    h.ctl("save")
    wait(20)
    return nets == [["minecraft:water", 12001, 16000, 0, 5, 0]]


def reabrir_conferir():
    """A segunda metade: o mundo reaberto tem a rede com a mesma água e a capacidade de antes (`fluid_reopen`: 12.001 e 16.000, as shares de 5 canos somando o mesmo), e os canos ligados."""
    wait(120)
    nets = log_lines("MBP")[-1]["nets"]
    views = last_views()
    arms = [views.get(at(x), {}).get("conn") for x in range(5)]
    print("depois de reabrir:", nets)
    good = nets == [["minecraft:water", 12001, 16000, 0, 5, 0]] and all(arms)
    print(("OK   " if good else "DIFF ") + "a rede reaberta tem a água e a capacidade de antes, e os canos continuam ligados")
    return good


def lava():
    """A lava sai da fonte por um cano em `PULL`, anda os quatro canos e chega ao Heat Generator: a rede enche, o tanque do gerador recebe, o gerador acende."""
    montar_lava()
    h.ctl("give", "mekanism:configurator", "1")
    first = at(0)
    for _ in range(12):
        # O braço oeste é o lado da fonte: a sequência do Configurator é NORMAL, PUSH, PULL, NONE.
        click_arm(first, "west")
        time.sleep(1.2)
        view = last_views().get(first)
        if view and view["raw"][4] == "pull":
            break
    print("lado oeste do primeiro cano:", view and view["raw"][4])
    wait(120)
    nets = log_lines("MBP")[-1]["nets"]
    gen = lava_of(at(4))
    print("rede", nets, "lava no gerador", gen)
    good = bool(view) and view["raw"][4] == "pull" and gen > 0
    print(("OK   " if good else "DIFF ") + f"a lava chegou ao Heat Generator ({gen} mB no tanque)")
    return good


def main():
    what = sys.argv[1] if len(sys.argv) > 1 else "fluido"
    if what == "fluido":
        names = list(SCENARIOS) if len(sys.argv) < 3 or sys.argv[2] == "todos" else [sys.argv[2]]
        ok = True
        for name in names:
            ok &= run_fluid(name)
        sys.exit(0 if ok else 1)
    if what == "lava":
        sys.exit(0 if lava() else 1)
    if what == "reabrir":
        part = sys.argv[2] if len(sys.argv) > 2 else "montar"
        sys.exit(0 if (reabrir_montar() if part == "montar" else reabrir_conferir()) else 1)
    if what == "configurador":
        sys.exit(0 if configurador() else 1)
    raise SystemExit("uso: h2_cano_jogo.py fluido [CENARIO|todos] | lava | configurador | reabrir | video")


if __name__ == "__main__":
    main()
