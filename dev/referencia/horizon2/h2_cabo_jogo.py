#!/usr/bin/env python3
"""Os cabos universais no jogo contra o original (frente `cabo-jogo`, Fase 3, decisão 121).

    H_NAME=horizon PASTA=<pasta do game.log> h2_cabo_jogo.py energia CENARIO|todos   # repete os rastros energy_*.jsonl tick a tick
    H_NAME=horizon PASTA=<pasta do game.log> h2_cabo_jogo.py configurador            # o Configurator cicla um lado, o braço some e volta, o redstone reativo
    H_NAME=horizon PASTA=<pasta do game.log> h2_cabo_jogo.py puxar                   # o cabo em PULL puxa de um cubo de energia

A sonda (`TRANSMITTER_PROBE = true` na cópia de teste, `enviar_fontes.sh`) escreve no stderr do jogo (`PASTA/game.log`): `MBE {t, runs, nets: [[energia, capacidade, saiu, cabos,
aceitadores], ...]}` a cada `ServerTickEvent.Post` e `MBV {p, conn, kind, raw, net}` a cada vez que um cabo publica as conexões dele. O servidor fica parado (`tick freeze`) e anda
com `tick step 1`: cada passo dá uma linha `MBE` com `runs` verdadeiro, que se compara com a linha `estate` do rastro do mesmo tick (a energia de cada rede, a capacidade e o que ela mandou).
O rastro põe máquinas com buffer de 1.000 a 2.000.000.000 (`EnergyContainer` feito para o teste), e a Enrichment Chamber de verdade guarda 20: a comparação vai até o tick em que
o rastro põe a primeira máquina, e o que a rede faz com consumidores (a divisão entre eles) é conferido nos cenários do `owner_trace.rs`, com o buffer do rastro.

O original é o rastro que o `EnergyNetworkTraceTest` gravou com o Mekanism de referência (`dev/referencia/rastros/energy_*.jsonl`, duas execuções): é o que o original fez, tick a tick.
"""
import json, os, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("H_NAME", "horizon")
import h2_identity as h

RASTROS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "rastros")
PASTA = os.environ.get("PASTA", "~/.cache/rust-mekanism/restos")
BASE = (0, 150, 0)
CABLE = {"basic": "mekanism:basic_universal_cable", "advanced": "mekanism:advanced_universal_cable", "elite": "mekanism:elite_universal_cable",
         "ultimate": "mekanism:ultimate_universal_cable"}
CHAMBER = "mekanism:enrichment_chamber"
SCENARIOS = ["tiers", "merge_full", "break_end_odd", "split_middle"]


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


def place(p, item):
    # O jogador precisa estar perto e com a face à vista: os cabos pelo norte da linha, as máquinas pelo sul.
    stand((p[0] + 2, p[1], p[2] - 2) if item in CABLE.values() else (p[0] + 2, p[1] + 1, p[2] + 2))
    h.ctl("give", item, "1")
    h.ctl("select", item)
    out = h.ctl("place", *map(str, p), "--item", item)
    if '"error"' in out:
        print("   place falhou:", out.strip()[:140])
    return '"error"' not in out


def clear():
    cmd("tick unfreeze")
    # O chão da cena: o `limpo` tem ar em y 149, e o `place` do cabo pede uma face à vista.
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
    """Um tick: devolve a linha `MBE` com `runs` verdadeiro que ele deu."""
    before = len(log_lines("MBE"))
    cmd("tick step 1")
    for _ in range(40):
        time.sleep(0.15)
        new = [row for row in log_lines("MBE")[before:] if row.get("runs")]
        if new:
            return new[-1]
    return None


def energy(p):
    out = cmd("data get block %s energy" % txt(p))
    try:
        return int(out.split()[-1].rstrip("b"))
    except (ValueError, IndexError):
        return 0


def rows_of(scenario, run=""):
    name = f"energy_{run + '_' if run else ''}{scenario}.jsonl"
    return [json.loads(l) for l in open(os.path.join(RASTROS, name)) if l.strip()]


def scenario_nets(rows):
    mine = set()
    for r in rows:
        if r["e"] == "state":
            mine |= {n["net"] for n in r["nets"]} | {c["net"] for c in r["cables"] if c and c.get("net") is not None}
    return mine


def run_energy(scenario, run=""):
    rows = rows_of(scenario, run)
    mine = scenario_nets(rows)
    last = max(r["t"] for r in rows)
    marks = {}
    for r in rows:
        if r["e"] == "mark":
            marks.setdefault(r["t"], []).append(r["m"])
    estates = {r["t"]: r for r in rows if r["e"] == "estate"}
    first_machine = min([r["t"] for r in rows if r["e"] == "mark" and r["m"].removeprefix("pre ").startswith("chamber")] or [last + 1])
    pushes = [r for r in rows if r["e"] == "push"]
    clear()
    cmd("gamemode creative @s")
    actors = {}
    state = {"pushed": 0}
    diffs = []

    def apply(mark):
        words = mark.removeprefix("pre ").split()
        n = lambda i: int(words[i])
        if words[0] == "place":
            place(at(n(1), 0, n(2)), CABLE[words[3]])
        elif words[0] == "break":
            cmd("setblock %s minecraft:air" % txt(at(n(1), 0, n(2))))
        elif words[0] == "chamber":
            p = at(n(2), n(3), n(4))
            # Sem o item não há a configuração de lados padrão (decisão 113): o `config0` põe `INPUT` nos seis lados, como o rastro (`input 63`).
            cmd("setblock %s %s" % (txt(p), CHAMBER))
            wait(5)
            cmd("data merge block %s {component_config:{config0:[I;1,1,1,1,1,1]},energy:%d}" % (txt(p), min(n(6), 2**31 - 1)))
            actors[words[1]] = p
        elif words[0] == "setenergy":
            cmd("data merge block %s {energy:%d}" % (txt(actors[words[1]]), n(2)))
        elif words[0] == "push":
            # O que um vizinho empurra pela capability do cabo: a caixa de entrada do Horizon (`energy` inteiro do block entity), que o dono passa para a rede.
            cmd("data merge block %s {energy:%d}" % (txt(at(n(1), 0, n(2))), n(3)))
            state["pushed"] += 1
        elif words[0] == "finish":
            pass
        else:
            raise SystemExit("ação desconhecida: " + mark)

    cmd("tick freeze")
    for t in range(0, last + 1):
        ms = marks.get(t, [])
        if any(m == "pre finish" for m in ms):
            break
        for m in ms:
            if m.startswith("pre "):
                apply(m)
        mbe = step()
        for m in ms:
            if not m.startswith("pre "):
                apply(m)
        want = estates.get(t)
        if want is None or t >= first_machine:
            continue
        want_nets = sorted((n["energy"], n["capacity"], n["prev"]) for n in want["nets"] if n["net"] in mine)
        got_nets = sorted((n[0], n[1], n[2]) for n in (mbe or {}).get("nets", []) if n[3] > 0)
        want_actors = {a["name"]: a["energy"] for a in want["actors"]}
        got_actors = {name: energy(p) for name, p in actors.items() if name in want_actors and want_actors[name] <= 1000}
        want_actors = {k: v for k, v in want_actors.items() if k in got_actors}
        if got_nets != want_nets or got_actors != want_actors:
            diffs.append((t, want_nets, got_nets, want_actors, got_actors))
    cmd("tick unfreeze")
    print(f"{run or 'run1'} {scenario}: {'IGUAL' if not diffs else 'DIFERE em %d ticks' % len(diffs)}")
    for d in diffs[:4]:
        print("   t=%d rastro %s %s  jogo %s %s" % (d[0], d[1], d[3], d[2], d[4]))
    return not diffs


def last_views():
    last = {}
    for v in log_lines("MBV"):
        last[tuple(v["p"])] = v
    return last


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


def configurador():
    """O Configurator agachado cicla o tipo de conexão do lado do braço que o olhar toca (NORMAL, PUSH, PULL, NONE), o braço some e volta; sem agachar liga o redstone reativo."""
    clear()
    cmd("gamemode creative @s")
    cmd("clear @s")
    cmd("tick unfreeze")
    for x in range(3):
        place(at(x), CABLE["basic"])
    wait(40)
    h.ctl("give", "mekanism:configurator", "1")
    cable = at(1)
    ok = True
    misses = 0
    for want in ("push", "pull", "none"):
        # A mira do `interact` cai no braço na maior parte das vezes e no centro nas outras (o centro dá o lado clicado, o norte): o clique que erra é contado e repetido.
        for _ in range(8):
            before = (last_views().get(cable) or {}).get("raw", [None] * 6)[5]
            click_arm(cable, "east")
            time.sleep(1.5)
            view = last_views().get(cable)
            if view and view["raw"][5] != before:
                break
            misses += 1
        got = view["raw"][5] if view else None
        arm = view["conn"][5] if view else None
        other = last_views().get(at(2))
        other_arm = other["conn"][4] if other else None
        expected_arm = want != "none"
        good = got == want and arm == expected_arm and other_arm == expected_arm
        ok &= good
        print(("OK   " if good else "DIFF ") + f"agachado no braço leste: tipo cru {got} (esperado {want}), braço do meio {arm}, braço do vizinho {other_arm}")
    # Sem braço não há o que mirar: o clique cai na face (o lado clicado, como o original quando `getSideLookingAt` é nulo). O vizinho sai para a face leste ficar à vista.
    cmd("setblock %s minecraft:air" % txt(at(2)))
    wait(10)
    stand((cable[0] + 3.5, cable[1], cable[2] + 0.5))
    h.ctl("select", "mekanism:configurator")
    h.ctl("key", "shift", "down")
    wait(3)
    h.ctl("interact", *map(str, cable), "east")
    h.ctl("key", "shift", "up")
    wait(6)
    time.sleep(1.5)
    place(at(2), CABLE["basic"])
    wait(30)
    time.sleep(1.5)
    view = last_views().get(cable)
    other = last_views().get(at(2))
    good = bool(view) and view["raw"][5] == "normal" and view["conn"][5] and bool(other) and other["conn"][4]
    ok &= good
    print(("OK   " if good else "DIFF ") + f"agachado na face leste sem braço: tipo cru {view and view['raw'][5]} (esperado normal), o braço volta com o vizinho reposto {view and view['conn'][5]} {other and other['conn'][4]}")
    print(f"cliques que erraram o braço: {misses}")
    print("fim", "OK" if ok else "DIFF")
    return ok


def puxar():
    """O cabo em PULL puxa de um cubo de energia: o cubo com 100.000 de energia fica ao sul do primeiro cabo de uma linha de 3; o lado sul vai a PULL com o Configurator e a rede
    enche (24.000 de capacidade) com o que o cubo perde, sem a energia mudar de total."""
    clear()
    cmd("gamemode creative @s")
    cmd("clear @s")
    cmd("tick unfreeze")
    for x in range(3):
        place(at(x), CABLE["basic"])
    place(at(0, 0, 1), "mekanism:basic_energy_cube")
    wait(20)
    # Os seis lados do cubo em saída e sem ejetor: ele não empurra nada para o cabo, e o `PULL` é que tira a energia (`extract` da capability, que respeita a saída do lado).
    cmd("data merge block %s {energy:100000,component_config:{config0:[I;4,4,4,4,4,4],eject0:0b}}" % txt(at(0, 0, 1)))
    wait(10)
    h.ctl("give", "mekanism:configurator", "1")
    print("cubo antes", energy(at(0, 0, 1)))
    cable = at(0)
    for _ in range(12):
        click_arm(cable, "south")
        time.sleep(1.2)
        view = last_views().get(cable)
        if view and view["raw"][3] == "pull":
            break
    print("lado sul:", view and view["raw"][3], view and view["kind"][3])
    wait(60)
    nets = log_lines("MBE")[-1]["nets"]
    after = energy(at(0, 0, 1))
    print("cubo depois", after, "rede", nets)
    stored = nets[0][0] if nets else 0
    good = view is not None and view["raw"][3] == "pull" and stored > 0 and stored + after == 100000
    print(("OK   " if good else "DIFF ") + f"o cabo puxou {stored} do cubo ({after} sobraram): a energia só mudou de lugar")
    return good


def reabrir_montar():
    """`energy_reopen.jsonl` no jogo, a primeira metade: cinco cabos (o do meio avançado), 31.001 de energia na rede (a caixa de entrada do primeiro cabo), e o mundo salvo e fechado.
    Quem chama reabre o mundo e roda `reabrir conferir`."""
    clear()
    cmd("gamemode creative @s")
    cmd("clear @s")
    cmd("tick unfreeze")
    for x, tier in enumerate(["basic", "basic", "advanced", "basic", "basic"]):
        place(at(x), CABLE[tier])
    wait(60)
    cmd("data merge block %s {energy:31001}" % txt(at(0)))
    wait(30)
    nets = log_lines("MBE")[-1]["nets"]
    print("antes de fechar:", nets)
    h.ctl("save")
    wait(20)
    return nets == [[31001, 160000, 0, 5, 0]]


def reabrir_conferir():
    """A segunda metade: o mundo reaberto tem a rede com a mesma energia e a capacidade de antes (`energy_reopen`: 31.001 e 160.000), e os cabos ligados."""
    wait(120)
    nets = log_lines("MBE")[-1]["nets"]
    views = last_views()
    arms = [views.get(at(x), {}).get("conn") for x in range(5)]
    print("depois de reabrir:", nets)
    good = nets == [[31001, 160000, 0, 5, 0]] and all(arms)
    print(("OK   " if good else "DIFF ") + "a rede reaberta tem a energia e a capacidade de antes, e os cabos continuam ligados")
    return good


LINHA = 6
CUBO = at(0, 0, 1)
MAQUINAS = {"A": at(2, 0, 1), "B": at(5, 0, 1)}
MEIO = at(3)
SHOTS = os.environ.get("SHOTS", "~/.cache/rust-mekanism/cabo-jogo/prints")


def shot(name):
    """Um print do jogo (o `horizon-ctl screenshot` grava dentro do container; o arquivo vai para a pasta de prints do desktop)."""
    h.ctl("screenshot", f"/tmp/{name}.png")
    subprocess.run(["ssh", "desktop", f"mkdir -p {SHOTS} && docker exec {os.environ.get('H_NAME', 'horizon')} cat /tmp/{name}.png > {SHOTS}/{name}.png"], check=True)


def enquadrar():
    """A câmera de todo o roteiro: ao norte e a oeste da linha, onde o Configurator alcança o braço (`click_arm`), olhando o meio dela."""
    cmd("tp @s %.2f %.2f %.2f" % (at(1)[0] - 2.3, at(1)[1], at(1)[2] - 2.5))
    wait(12)
    h.ctl("look-at", "%.2f" % (BASE[0] + 3.0), "%.2f" % (BASE[1] + 0.3), "%.2f" % (BASE[2] + 0.7))
    wait(4)


def montar_cena():
    """Seis cabos em linha, o cubo criativo ao lado do primeiro (ejeta), duas Enrichment Chamber com minério de ferro (A no terceiro cabo, B no último)."""
    clear()
    cmd("gamemode creative @s")
    cmd("clear @s")
    cmd("tick unfreeze")
    for x in range(LINHA):
        place(at(x), CABLE["basic"])
    place(MAQUINAS["A"], CHAMBER)
    place(MAQUINAS["B"], CHAMBER)
    place(CUBO, "mekanism:creative_energy_cube")
    wait(10)
    cmd("data merge block %s {component_config:{config0:[I;4,4,4,4,4,4],eject0:1b},energy_amount:9223372036854775807L}" % txt(CUBO))
    for p in MAQUINAS.values():
        cmd("data merge block %s {Items:[{Slot:0b,id:\"minecraft:iron_ore\",count:64}]}" % txt(p))
    h.ctl("give", "mekanism:configurator", "1")
    wait(40)


def video_roteiro(pausa):
    """O que o vídeo mostra: as duas máquinas funcionando com a energia que vem do cubo pela linha; o cabo do meio quebrado (a máquina B, depois dele, para) e reposto (volta); o
    Configurator agachado no braço leste do segundo cabo (o braço some e B para) e na face (o braço volta e B volta). `pausa(s)` espera e deixa o gravador andar."""
    enquadrar()
    shot("cabo_linha_ligada")
    pausa(4)
    cmd("setblock %s minecraft:air destroy" % txt(MEIO))
    pausa(1)
    shot("cabo_meio_quebrado")
    pausa(4)
    cmd("setblock %s mekanism:basic_universal_cable" % txt(MEIO))
    pausa(1)
    shot("cabo_meio_reposto")
    pausa(4)
    for _ in range(3):
        # O `interact` vira a mira para a face do bloco: o roteiro devolve o enquadramento depois de cada clique e repete o que errou o braço (o clique que erra muda o lado
        # clicado, o norte, que não tem braço).
        for _ in range(6):
            antes = (last_views().get(at(1)) or {}).get("raw", [None] * 6)[5]
            click_arm(at(1), "east")
            enquadrar()
            time.sleep(1.2)
            if (last_views().get(at(1)) or {}).get("raw", [None] * 6)[5] != antes:
                break
        pausa(2)
    shot("cabo_configurador_sem_braco")
    pausa(4)
    cmd("setblock %s minecraft:air" % txt(at(2)))
    stand((at(1)[0] + 3.5, at(1)[1], at(1)[2] + 0.5))
    h.ctl("select", "mekanism:configurator")
    h.ctl("key", "shift", "down")
    wait(3)
    h.ctl("interact", *map(str, at(1)), "east")
    h.ctl("key", "shift", "up")
    cmd("setblock %s mekanism:basic_universal_cable" % txt(at(2)))
    enquadrar()
    shot("cabo_configurador_braco_volta")
    pausa(4)


def video():
    """O vídeo com som: `NOME` (o `gravar-jogo start`), `SEGUNDOS` (padrão 60), `ARQUIVO` (padrão cabo.mp4). O volume mestre só sobe depois de o stream do container estar no sink nulo."""
    import threading
    import h2_nota_instrumento as n
    nome, segundos, arquivo = os.environ["NOME"], os.environ.get("SEGUNDOS", "60"), os.environ.get("ARQUIVO", "cabo.mp4")
    n.sky()
    cmd("tick unfreeze")
    montar_cena()
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
        video_roteiro(lambda s: time.sleep(s))
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
    what = sys.argv[1] if len(sys.argv) > 1 else "energia"
    if what == "energia":
        names = SCENARIOS if len(sys.argv) < 3 or sys.argv[2] == "todos" else [sys.argv[2]]
        ok = True
        for name in names:
            ok &= run_energy(name)
        sys.exit(0 if ok else 1)
    if what == "configurador":
        sys.exit(0 if configurador() else 1)
    if what == "video":
        video()
        return
    if what == "cena":
        montar_cena()
        video_roteiro(lambda s: time.sleep(s))
        return
    if what == "reabrir":
        part = sys.argv[2] if len(sys.argv) > 2 else "montar"
        sys.exit(0 if (reabrir_montar() if part == "montar" else reabrir_conferir()) else 1)
    if what == "puxar":
        sys.exit(0 if puxar() else 1)
    raise SystemExit("uso: h2_cabo_jogo.py energia [CENARIO|todos] | configurador | puxar")


if __name__ == "__main__":
    main()
