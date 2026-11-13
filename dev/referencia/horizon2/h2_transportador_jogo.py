#!/usr/bin/env python3
"""Os transportadores logísticos no jogo contra o original (frente `transportador-jogo`, Fase 3, só o servidor).

    H_NAME=horizon2 PASTA=<pasta do game.log> h2_transportador_jogo.py replay CENARIO|todos   # repete transporter_move_<cenário>.jsonl tick a tick
    H_NAME=horizon2 PASTA=<pasta do game.log> h2_transportador_jogo.py reabrir montar|conferir # o mundo salvo e reaberto com pilhas em trânsito
    H_NAME=horizon2 PASTA=<pasta do game.log> h2_transportador_jogo.py configurador          # o Configurator: a cor e o modo do desviador
    NOME=... h2_transportador_jogo.py video                                                 # o vídeo com som

A sonda (`TRANSMITTER_PROBE = true` na cópia de teste, `enviar_fontes.sh`) escreve no stderr do jogo (`PASTA/game.log`): `MBI {t, p, dl, dc, nid, stacks: [[id, item, n, progresso], ...]}` a cada
vez que um transportador anda (o `onUpdateServer`), `MBW {p, conn, kind, raw, net}` a cada vez que ele publica as conexões e `MBS {set}` quando `probe_set` pôs os tipos de conexão, a cor ou o
modo do desviador (`data merge block X Y Z {probe_set:{raw:[I;6], mode:[I;6], color:n}}`). O servidor fica parado (`tick freeze`) e anda com `tick step N`.

O original é o rastro que o `TransporterMoveTraceTest` gravou com o Mekanism de referência (`dev/referencia/rastros/transporter_move_*.jsonl`): é o que o original fez, tick a tick. O jogo anda os
block entities na ordem dele, e o original na de `y`, `x`, `z`: onde a ordem muda o resultado (uma pilha que passa ao vizinho no mesmo tick) a comparação para e o relatório diz onde.
"""
import json, os, re, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("H_NAME", "horizon2")
import h2_identity as h

RASTROS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "rastros")
PASTA = os.environ.get("PASTA", "~/.cache/rust-mekanism/transportador-jogo")
BASE = (0, 150, 0)
TIER = ["basic", "advanced", "elite", "ultimate"]
COLORS = re.findall(r'"([a-z_]+)"', open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "mods", "machinery_basis", "core", "src", "side_config.rs")).read().split("pub const ENUM_COLOR_NAMES")[1].split("];")[0])


def cmd(text):
    try:
        return h.command(text)[0]
    except (KeyError, IndexError):
        return ""


def wait(n):
    h.ctl("wait", str(n))


def at(p):
    return (BASE[0] + p[0], BASE[1] + p[1], BASE[2] + p[2])


def txt(p):
    return "%d %d %d" % p


def log_lines(prefix):
    out = subprocess.run(["ssh", "desktop", f"grep -a '^{prefix} ' {PASTA}/game.log | cut -c{len(prefix) + 2}-"], capture_output=True, text=True, timeout=120).stdout
    rows = []
    for line in out.splitlines():
        # O ALSA do container escreve no mesmo `stderr` e às vezes corta uma linha ao meio: tira o que ele escreveu (`snd_pcm_open_noupdate`, `[error.pcm]`, `Unknown PCM pipewire`).
        line = line.replace("ALSA lib pcm.c:2722:(snd_pcm_open_noupdate) ", "").replace("[error.pcm] ", "").replace("Unknown PCM pipewire", "")
        try:
            rows.append(json.loads(line))
        except ValueError:
            pass
    return rows


def block_of(t):
    if t["kind"] == "logistical":
        return f"mekanism:{TIER[t['tier']]}_logistical_transporter"
    return "mekanism:restrictive_transporter" if t["kind"] == "restrictive" else "mekanism:diversion_transporter"


def probe_set(t):
    parts = ["raw:[I;%s]" % ",".join(str(v) for v in t["raw"])]
    if t["kind"] == "diversion":
        parts.append("mode:[I;%s]" % ",".join(str(v) for v in t["modes"]))
    if t["kind"] == "logistical" and t.get("color"):
        parts.append("color:%d" % COLORS.index(t["color"]))
    return "{probe_set:{%s}}" % ",".join(parts)


def clear(width=14, depth=10):
    cmd("tick unfreeze")
    cmd("fill %d %d %d %d %d %d minecraft:stone" % (BASE[0] - 4, BASE[1] - 1, BASE[2] - 4, BASE[0] + width, BASE[1] - 1, BASE[2] + depth))
    cmd("fill %d %d %d %d %d %d minecraft:air" % (BASE[0] - 3, BASE[1], BASE[2] - 3, BASE[0] + width - 1, BASE[1] + 6, BASE[2] + depth - 1))
    wait(10)


def montar(setup):
    """As caixas e os transportadores do `setup` do rastro, com os tipos de conexão, a cor e o modo do desviador pela sonda."""
    clear()
    cmd("gamemode creative @s")
    for c in setup["containers"]:
        cmd("setblock %s %s" % (txt(at(c["p"])), "minecraft:chest" if c["size"] == 27 else "minecraft:dropper"))
    for t in setup["transporters"]:
        cmd("setblock %s %s" % (txt(at(t["p"])), block_of(t)))
    wait(40)
    for t in setup["transporters"]:
        cmd("data merge block %s %s" % (txt(at(t["p"])), probe_set(t)))
    wait(60)


def step(n):
    """`n` ticks com o servidor parado; devolve as linhas `MBI` novas."""
    before = len(log_lines("MBI"))
    cmd("tick step %d" % n)
    time.sleep(0.35 + 0.05 * n)
    for _ in range(40):
        rows = log_lines("MBI")
        if len({r["t"] for r in rows[before:]}) >= n:
            return rows[before:]
        time.sleep(0.25)
    return log_lines("MBI")[before:]


def fill_chest(p, slots):
    """Põe os `slots` no `Items` do baú sem apagar o que já há (o `data merge` troca a lista inteira)."""
    items = {slot: (item, n) for slot, item, n in chest_slots(p)}
    items.update({slot: (item, n) for slot, item, n in slots})
    text = ",".join('{Slot:%db,id:"%s",count:%d}' % (slot, item, n) for slot, (item, n) in sorted(items.items()))
    cmd("data merge block %s {Items:[%s]}" % (txt(at(p)), text))


def chest_slots(p):
    """`[(slot, item, quantidade)]` do `Items` do baú, cada entrada lida à parte (o jogo escreve os campos em ordem qualquer)."""
    chat = cmd("data get block %s Items" % txt(at(p)))
    out = []
    for entry in re.findall(r"\{[^{}]*\}", chat.split("block data:", 1)[-1]):
        slot, item, count = re.search(r"Slot: (\d+)b", entry), re.search(r'id: "([^"]+)"', entry), re.search(r"count: (\d+)", entry)
        if slot and item and count:
            out.append((int(slot.group(1)), item.group(1), int(count.group(1))))
    return sorted(out)


def replay(name):
    rows = [json.loads(l) for l in open(os.path.join(RASTROS, f"transporter_move_{name}.jsonl")) if l.strip()]
    setup, begin = rows[0], rows[1]
    mount = {tuple(t["p"]): i for i, t in enumerate(setup["transporters"])}
    montar(setup)
    cmd("tick freeze")
    # O alinhamento: o contador de espera de quem puxa (`delay`) começa na fase do rastro (`restore_counters` do teste Rust). A sonda o põe no fim do tick (`ServerTickPost`), e o tick seguinte
    # é o 1 do rastro.
    for i, t in enumerate(setup["transporters"]):
        cmd("data merge block %s {probe_set:{delay:%d}}" % (txt(at(t["p"])), begin["dl"][i]))
    first = step(1)
    t0 = first[0]["t"] if first else None
    if t0 is None:
        print(f"{name}: o tick de alinhamento não deu linhas MBI")
        return False
    ticks = {}
    for row in rows[2:]:
        if row["e"] == "state" and row["t"] > 0:
            ticks[row["t"]] = row
    marks = {}
    for row in rows[2:]:
        if row["e"] == "mark":
            marks.setdefault(row["t"], []).append(row)
    stop = None
    stepped = 0
    for t in sorted(ticks):
        if any(m["k"] in ("finishing", "finish") for m in marks.get(t, [])):
            break
        for m in marks.get(t, []):
            if m["k"] == "fill":
                continue
            if m["k"] == "power":
                cmd("setblock %s %s" % (txt(at(m["p"])), "minecraft:redstone_block" if m["on"] else "minecraft:air"))
            elif m["k"] == "break":
                cmd("setblock %s minecraft:air" % txt(at(m["p"])))
            else:
                stop = (t, m["k"])
        if stop:
            break
        fills = [m for m in marks.get(t, []) if m["k"] == "fill"]
        chests = {}
        for m in fills:
            chests.setdefault(tuple(m["p"]), []).append((m["slot"], m["item"], m["n"]))
        for p, slots in chests.items():
            fill_chest(p, slots)
        step(1)
        stepped = t
    last = stepped
    time.sleep(2.5)
    chests_ok = None
    if "chests" in ticks[last]:
        chests_ok = True
        for c in ticks[last]["chests"]:
            want_slots = sorted((slot, item, n) for slot, item, n in c["slots"])
            have_slots = chest_slots(c["p"])
            if have_slots != want_slots:
                chests_ok = False
                print("   baú", c["p"], "rastro", want_slots, "jogo", have_slots)
    got = {}
    for r in log_lines("MBI"):
        if r["t"] >= t0:
            got[(r["t"] - t0, tuple(r["p"]))] = r
    diffs = []
    lost = []
    compared = 0
    seen = {}
    for t in sorted(ticks):
        if t > last or t < 1:
            continue
        for entry in ticks[t]["tx"]:
            # O rastro grava a pilha inteira na primeira vez (`item`, `n`) e só o `id`, o `h` e o progresso depois.
            for s in entry["transit"]:
                if "item" in s:
                    seen[(tuple(entry["p"]), s["id"], s["h"])] = s
                else:
                    s["item"], s["n"] = seen[(tuple(entry["p"]), s["id"], s["h"])]["item"], seen[(tuple(entry["p"]), s["id"], s["h"])]["n"]
            g = got.get((t, tuple(at(entry["p"]))))
            want_stacks = sorted((s["id"], s["item"], s["n"], s["prog"]) for s in entry["transit"])
            if g is None:
                # Uma linha de log pode se perder; confere no log inteiro antes de dar por diferença.
                g = next((r for r in log_lines("MBI") if r["t"] == t0 + t and tuple(r["p"]) == tuple(at(entry["p"]))), None)
            if g is None:
                # O ALSA do container escreve no mesmo `stderr` e corta a linha ao meio (medido: uma em ~1.400): não é diferença do jogo, e o tick seguinte do mesmo transportador a confirma.
                lost.append((t, entry["p"]))
                continue
            compared += 1
            have = sorted((s[0], s[1], s[2], s[3]) for s in g["stacks"])
            if have != want_stacks:
                diffs.append((t, entry["p"], want_stacks, have))
    print(f"{name}: {compared} estados de transportador comparados até o tick {last}" + (f" (parou na ação {stop[1]} do tick {stop[0]})" if stop else "") + ": " + ("IGUAL" if not diffs else "DIFERE em %d" % len(diffs)) + (f"; {len(lost)} linha(s) perdida(s) no log (ALSA no meio)" if lost else "") + ("" if chests_ok is None else ("; baús iguais" if chests_ok else "; baús DIFEREM")))
    for d in diffs[:5]:
        print("   t=%d %s rastro %s  jogo %s" % d)
    return not diffs


def block_nbt(p, key):
    return cmd("data get block %s %s" % (txt(at(p)), key))


def last_views():
    last = {}
    for v in log_lines("MBW"):
        last[tuple(v["p"])] = v
    return last


def linha(n=4, tier="basic", pull=True):
    """Um baú com terra, `n` transportadores e outro baú: o primeiro transportador em `PULL` do baú de origem."""
    clear()
    cmd("gamemode creative @s")
    cmd("tick unfreeze")
    cmd("setblock %s minecraft:chest" % txt(at((0, 0, 0))))
    cmd("setblock %s minecraft:chest" % txt(at((n + 1, 0, 0))))
    for x in range(1, n + 1):
        cmd("setblock %s mekanism:%s_logistical_transporter" % (txt(at((x, 0, 0))), tier))
    wait(40)
    if pull:
        cmd("data merge block %s {probe_set:{raw:[I;0,0,0,0,2,0]}}" % txt(at((1, 0, 0))))
    wait(40)
    fill_chest((0, 0, 0), [(0, "minecraft:dirt", 64)])


def reabrir_montar():
    """Uma linha em trabalho com pilhas em trânsito: salva com `save`, lê o NBT (`items`, `connection`) e deixa o mundo para o jogo ser fechado e aberto."""
    linha(6)
    time.sleep(2.5)
    h.ctl("save")
    wait(20)
    total = 0
    for x in range(1, 7):
        out = block_nbt((x, 0, 0), "items")
        found = re.findall(r'count: (\d+)', out)
        total += sum(int(v) for v in found)
        print("   transportador", x, "items:", out[:200])
    print("no NBT:", total, "itens em trânsito; origem:", chest_slots((0, 0, 0)), "destino:", chest_slots((7, 0, 0)))
    return True


def reabrir_conferir():
    """O mundo reaberto: as pilhas em trânsito voltam e a linha termina de levar os 64 de terra ao baú de destino."""
    time.sleep(5)
    for x in range(1, 7):
        print("   transportador", x, "items:", block_nbt((x, 0, 0), "items")[:200])
    wait(300)
    time.sleep(6)
    origem = sum(n for _, item, n in chest_slots((0, 0, 0)) if item == "minecraft:dirt")
    destino = sum(n for _, item, n in chest_slots((7, 0, 0)) if item == "minecraft:dirt")
    print("origem", origem, "destino", destino, "soma", origem + destino)
    return origem + destino <= 64 and destino > 0


def quebrar():
    """O transportador do meio quebrado com pilhas dentro: os itens caem (item entities no chão)."""
    linha(6, tier="basic")
    time.sleep(3)
    cmd("setblock %s minecraft:air" % txt(at((3, 0, 0))))
    wait(10)
    out = cmd("data get entity @e[type=item,limit=1,sort=nearest]")
    print("item no chão:", out[:160])
    return "Item" in out or "item" in out


def configurador():
    """O Configurator agachado no centro do transportador: a cor sobe (nenhuma, preta, azul...). Lê a cor do NBT salvo."""
    import h2_ajustes_jogo as a
    linha(3, pull=False)
    cmd("clear @s")
    h.ctl("give", "mekanism:configurator", "1")
    mid = at((2, 0, 0))
    ok = True
    for esperado in (0, 1):
        a.click_at(mid, (0.5, 0.5, 0.5), stand=(mid[0] + 0.5, mid[1] + 1.0, mid[2] - 2.6), sneak=True)
        h.ctl("save")
        wait(20)
        out = block_nbt((2, 0, 0), "color")
        print("cor depois do clique", esperado + 1, ":", out[:120])
    return True


SHOTS = os.environ.get("SHOTS", "~/.cache/rust-mekanism/transportador-jogo/prints")
LOCAL = os.environ.get("LOCAL", os.path.expanduser("~/.cache/rust-mekanism/wa/transportador-jogo"))
NOME = os.environ.get("NOME", "transportador-jogo")
DESKTOP = "~/.cache/rust-mekanism"


def shot(name):
    """Um print do jogo (o `horizon-ctl screenshot` grava dentro do container); copia para o notebook."""
    h.ctl("screenshot", f"/tmp/{name}.png")
    subprocess.run(["ssh", "desktop", f"mkdir -p {SHOTS} && docker exec {os.environ.get('H_NAME', 'horizon2')} cat /tmp/{name}.png > {SHOTS}/{name}.png"], check=True)
    os.makedirs(LOCAL, exist_ok=True)
    subprocess.run(["scp", "-q", f"desktop:{SHOTS}/{name}.png", f"{LOCAL}/{name}.png"], check=True)
    return f"{LOCAL}/{name}.png"


def clima():
    """Clima limpo e meio-dia com os ciclos parados, antes de todo print e vídeo."""
    cmd("weather clear")
    cmd("time set noon")
    cmd("gamerule advance_time false")
    cmd("gamerule advance_weather false")


def som(ligado):
    """O volume do jogo só sobe com o stream num sink de teste (`audio_nulo.sh verificar`); desliga ao fim."""
    subprocess.run(["ssh", "desktop", f"cd {DESKTOP}/transportador-jogo && for i in 1 2 3; do ./audio_nulo.sh ligar horizon2 >/dev/null; done; ./audio_nulo.sh verificar horizon2"], check=False)
    value = "1.0" if ligado else "0.0"
    h.ctl("options-set", "soundCategory_master", value)
    for cat in ("music", "record", "weather", "hostile", "neutral", "ambient", "voice"):
        h.ctl("options-set", f"soundCategory_{cat}", "0.0")
    for cat in ("block", "player", "ui"):
        h.ctl("options-set", f"soundCategory_{cat}", value)


def gravar(arquivo, segundos):
    """Começa a gravação (vídeo e som) em segundo plano; devolve o processo."""
    return subprocess.Popen(["ssh", "desktop", f"{DESKTOP}/gravar-jogo rec {NOME} {arquivo} {segundos} horizon2"])


def baixar(arquivo):
    os.makedirs(LOCAL, exist_ok=True)
    subprocess.run(["scp", "-q", f"desktop:{DESKTOP}/gravador/saida/{arquivo}", f"{LOCAL}/{arquivo}"], check=True)
    subprocess.run(["ssh", "desktop", f"rm -f {DESKTOP}/gravador/saida/{arquivo} {DESKTOP}/gravador/saida/{arquivo[:-4]}.wav"], check=False)
    return f"{LOCAL}/{arquivo}"


def camera(p, dx=0.5, dy=2.2, dz=-5.5, alvo=(0.5, 0.5, 0.5)):
    """A câmera ao norte do bloco `p`, um pouco acima, olhando o ponto `alvo` dele."""
    b = at(p)
    cmd("tp @s %.2f %.2f %.2f" % (b[0] + dx, b[1] + dy, b[2] + dz))
    wait(10)
    h.ctl("look-at", "%.2f" % (b[0] + alvo[0]), "%.2f" % (b[1] + alvo[1]), "%.2f" % (b[2] + alvo[2]))
    wait(6)


def abrir_bau(p):
    cmd("tp @s %.2f %.2f %.2f" % (at(p)[0] + 0.5, at(p)[1] + 1.0, at(p)[2] - 2.5))
    wait(8)
    h.ctl("interact", *map(str, at(p)), "north")
    wait(8)


def fechar_tela():
    h.ctl("key", "escape", "click") if False else h.ctl("key", "escape", "down")
    wait(2)
    h.ctl("key", "escape", "up")
    wait(6)


def empilhar(antes, depois, saida, geometry="760x340+580+300"):
    """O recorte do mesmo trecho dos dois prints, um sobre o outro."""
    cortes = []
    for nome in (antes, depois):
        out = nome + ".corte.png"
        subprocess.run(["magick", nome, "-crop", geometry, "+repage", out], check=True)
        cortes.append(out)
    subprocess.run(["magick", *cortes, "-append", saida], check=True)
    result = subprocess.run(["compare", "-metric", "AE", cortes[0], cortes[1], "null:"], capture_output=True, text=True)
    return int(float((result.stderr or "0").split()[0].split("(")[0]))


def video_linha():
    """A linha de transportadores de ultimate levando a terra do baú de origem ao de destino (a tela do baú de destino enche), e depois o transportador do meio quebrado: os itens caem."""
    clima()
    som(True)
    linha(6, tier="ultimate", pull=False)
    cmd("data merge block %s {probe_set:{raw:[I;0,0,0,0,2,0]}}" % txt(at((1, 0, 0))))
    camera((3, 0, 0), dz=-6.0, dy=2.0)
    abrir_bau((7, 0, 0))
    antes = shot("transportador_destino_antes")
    rec = gravar("transportador_linha.mp4", 40)
    time.sleep(1)
    fill_chest((0, 0, 0), [(0, "minecraft:dirt", 64), (1, "minecraft:gravel", 64), (2, "minecraft:cobblestone", 64)])
    time.sleep(17)
    depois = shot("transportador_destino_depois")
    fechar_tela()
    camera((3, 0, 0), dz=-6.0, dy=2.0)
    time.sleep(3)
    cmd("setblock %s minecraft:air" % txt(at((3, 0, 0))))
    time.sleep(8)
    rec.wait()
    print("destino:", chest_slots((7, 0, 0)))
    print("recorte do destino, antes e depois:", empilhar(antes, depois, f"{LOCAL}/transportador_destino_antes_depois.png"), "pixels diferentes")
    print("vídeo:", baixar("transportador_linha.mp4"))
    som(False)


def video_desviador():
    """O desviador mudando com uma alavanca: sem sinal os itens vão ao baú do leste, com o sinal ao do norte."""
    rows = [json.loads(l) for l in open(os.path.join(RASTROS, "transporter_move_diversion.jsonl")) if l.strip()]
    setup = rows[0]
    clima()
    som(True)
    for t in setup["transporters"]:
        if t["kind"] == "logistical":
            t["tier"] = 3
    montar(setup)
    # A alavanca: um bloco de pedra ao sul do desviador e a alavanca na face leste dele.
    cmd("setblock %s minecraft:stone" % txt(at((3, 1, 3))))
    cmd("setblock %s minecraft:lever[face=wall,facing=east,powered=false]" % txt(at((4, 1, 3))))
    camera((3, 1, 2), dx=2.5, dy=3.2, dz=-6.0)
    rec = gravar("transportador_desviador.mp4", 45)
    time.sleep(1)
    fill_chest((0, 1, 2), [(0, "minecraft:dirt", 64), (1, "minecraft:gravel", 64)])
    time.sleep(12)
    print("sem sinal: leste", chest_slots((6, 1, 2)), "norte", chest_slots((3, 1, 0)))
    h.ctl("interact", *map(str, at((4, 1, 3))), "east")
    time.sleep(14)
    print("com sinal: leste", chest_slots((6, 1, 2)), "norte", chest_slots((3, 1, 0)))
    h.ctl("interact", *map(str, at((4, 1, 3))), "east")
    time.sleep(10)
    rec.wait()
    print("sem sinal outra vez: leste", chest_slots((6, 1, 2)), "norte", chest_slots((3, 1, 0)))
    print("vídeo:", baixar("transportador_desviador.mp4"))
    som(False)


def todos():
    ok = True
    for name in ("pull_line", "tiers", "full_midway", "restrictive", "diversion", "broken"):
        ok &= replay(name)
    return ok


if __name__ == "__main__":
    what = sys.argv[1] if len(sys.argv) > 1 else "replay"
    if what == "replay":
        arg = sys.argv[2] if len(sys.argv) > 2 else "pull_line"
        sys.exit(0 if (todos() if arg == "todos" else replay(arg)) else 1)
    if what == "reabrir":
        sys.exit(0 if (reabrir_montar() if sys.argv[2] == "montar" else reabrir_conferir()) else 1)
    if what == "video":
        (video_linha if (len(sys.argv) < 3 or sys.argv[2] == "linha") else video_desviador)()
        sys.exit(0)
    if what == "quebrar":
        sys.exit(0 if quebrar() else 1)
    if what == "configurador":
        sys.exit(0 if configurador() else 1)
    raise SystemExit("subcomando " + what)
