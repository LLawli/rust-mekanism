#!/usr/bin/env python3
"""Os ajustes dos transportadores logísticos no jogo (frente `transportador-ajustes`, Fase 3).

    H_NAME=horizon2 PASTA=<pasta do game.log> h2_transportador_ajustes.py funil            # o funil sobre o primeiro transportador, tick a tick contra o rastro do original
    H_NAME=horizon2 PASTA=<pasta do game.log> h2_transportador_ajustes.py componentes montar  # itens com componentes numa linha, em trânsito; salva (o jogo é fechado e aberto depois)
    H_NAME=horizon2 PASTA=<pasta do game.log> h2_transportador_ajustes.py componentes conferir # o mundo reaberto: os itens chegam ao baú de destino com os mesmos componentes

`funil` repete `rastros/transporter_hopper.jsonl` (`TransporterAjustesTraceTest.hopper`, o original): um baú com 64 de terra sobre um funil sobre o primeiro de quatro transportadores
básicos, e um baú no fim da linha. O baú de cima ganha a terra antes do tick 2 e o servidor anda um tick por vez (`tick freeze` e `tick step 1`); a sonda `MBI` dá as pilhas de cada
transportador a cada tick e o fim lê o baú de cima, o funil e o do fim. `componentes` usa `rastros/transporter_components.jsonl` (o original) para o formato do NBT salvo.
"""
import json, os, re, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h
import h2_transportador_jogo as t

RASTROS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "rastros")
LOCAL = os.environ.get("LOCAL", os.path.expanduser("~/.cache/rust-mekanism/wa/transportador-ajustes"))
t.LOCAL = LOCAL
t.SHOTS = os.environ.get("SHOTS", "~/.cache/rust-mekanism/transportador-ajustes/prints")
cmd, wait, at, txt = t.cmd, t.wait, t.at, t.txt


def funil():
    rows = [json.loads(l) for l in open(os.path.join(RASTROS, "transporter_hopper.jsonl")) if l.strip()]
    states = {r["t"]: r for r in rows if r["e"] == "state"}
    final = max(states)
    t.clima()
    t.clear()
    cmd("gamemode creative @s")
    cmd("tick unfreeze")
    # Na ordem de `y`, `x`, `z`, a dos block entities do original no rastro (a estrutura do GameTest os coloca assim), e o funil e o baú de cima por último.
    for x in range(1, 5):
        cmd("setblock %s mekanism:basic_logistical_transporter" % txt(at((x, 0, 0))))
    cmd("setblock %s minecraft:chest" % txt(at((5, 0, 0))))
    cmd("setblock %s minecraft:hopper[facing=down]" % txt(at((1, 1, 0))))
    cmd("setblock %s minecraft:chest" % txt(at((1, 2, 0))))
    for p in ((5, 0, 0), (1, 2, 0)):
        cmd("data merge block %s {Items:[]}" % txt(at(p)))
    cmd("data merge block %s {Items:[]}" % txt(at((1, 1, 0))))
    wait(80)
    cmd("tick freeze")
    got = {}
    t0 = None
    for tick in range(final + 1):
        if tick == 2:
            t.fill_chest((1, 2, 0), [(0, "minecraft:dirt", 64)])
        rows_now = t.step(1)
        if t0 is None and rows_now:
            t0 = rows_now[0]["t"]
        for r in rows_now:
            got[(r["t"] - (t0 if t0 is not None else r["t"]), tuple(r["p"]))] = r
    time.sleep(2.5)
    diffs, handoff, compared, lost = [], 0, 0, 0
    for tick in range(final + 1):
        for x in range(1, 5):
            want = sorted(tuple(s) for s in states[tick]["tx"][x - 1])
            g = got.get((tick, tuple(at((x, 0, 0)))))
            if g is None:
                lost += 1
                continue
            compared += 1
            have = sorted((s[0], s[2], s[3]) for s in g["stacks"])
            if have != want:
                entering = [s for s in want if s[2] == 0]
                if entering and have == [s for s in want if s[2] != 0]:
                    handoff += 1
                    continue
                diffs.append((tick, x, want, have))
    top = sum(n for _, _, n in t.chest_slots((1, 2, 0)))
    hopper = sum(n for _, _, n in t.chest_slots((1, 1, 0)))
    dest = sum(n for _, _, n in t.chest_slots((5, 0, 0)))
    last = states[final]
    print(f"funil: {compared} estados de transportador comparados até o tick {final}: " + ("IGUAL" if not diffs else "DIFERE em %d" % len(diffs)) + f"; {handoff} repasse(s) lido(s) antes do vizinho passar a pilha; {lost} linha(s) perdida(s) no log")
    print(f"funil: baú de cima {top} (rastro {last['top']}), funil {hopper} (rastro {last['hop']}), baú do fim {dest} (rastro {last['dest']})")
    for d in diffs[:6]:
        print("   t=%d transportador %d rastro %s  jogo %s" % d)
    # Quando o primeiro item entrou, nos dois lados.
    first_trace = next(tick for tick in sorted(states) if any(states[tick]["tx"][0]))
    first_game = next((tick for tick in range(final + 1) if (g := got.get((tick, tuple(at((1, 0, 0)))))) and g["stacks"]), None)
    print(f"funil: o primeiro item aparece no transportador 1 no tick {first_trace} (rastro) e {first_game} (jogo, lido quando o transportador anda)")
    cmd("tick unfreeze")
    return not diffs and (top, hopper, dest) == (last["top"], last["hop"], last["dest"])


# Os itens com componentes: o que o original grava na pilha (`rastros/transporter_components.jsonl`) e como o comando os põe no baú.
ITENS = [
    ("pickaxe", 0, '{Slot:0b,id:"minecraft:iron_pickaxe",count:1,components:{"minecraft:damage":40,"minecraft:custom_name":"Rápida"}}', "minecraft:iron_pickaxe"),
    ("tablet", 1, '{Slot:1b,id:"mekanism:energy_tablet",count:1,components:{"mekanism:energy":500000L}}', "mekanism:energy_tablet"),
    ("dirt", 2, '{Slot:2b,id:"minecraft:dirt",count:3,components:{"minecraft:custom_name":"Terra boa"}}', "minecraft:dirt"),
]


def componentes_de(p):
    """`{item: texto dos components}` do `Items` do baú em `p`, cada entrada lida à parte (o jogo escreve os campos em ordem qualquer)."""
    chat = cmd("data get block %s Items" % txt(at(p)))
    out = {}
    for entry in re.findall(r"\{.*?\}(?=, \{|\]$|$)", chat.split("block data:", 1)[-1].strip()):
        item = re.search(r'id: "([^"]+)"', entry)
        comp = re.search(r"components: (\{.*\})\s*[,}]?", entry)
        if item:
            out.setdefault(item.group(1), []).append(comp.group(1) if comp else "")
    return out


def hover_e_print(p, nome, legenda):
    """Abre o baú em `p` e, para cada item com componentes, passa o mouse no slot dele e tira o print: o tooltip. Devolve os caminhos e onde cada slot ficou."""
    cmd("clear @s")
    t.abrir_bau(p)
    out = []
    widgets = json.loads(h.ctl("ui")).get("widgets", [])
    for rotulo, _, _, item in ITENS:
        slots = [w for w in widgets if w.get("kind") == "slot" and (w.get("stack") or {}).get("item") == item]
        if not slots:
            print("   sem slot de", item)
            continue
        x, y, w, hh = slots[0]["bounds"]
        h.ctl("scroll-at", str(int(x + 8)), str(int(y + 8)), "0")
        time.sleep(0.8)
        caminho = t.shot(f"{nome}_{rotulo}")
        out.append((rotulo, caminho, (int(x), int(y))))
    t.fechar_tela()
    return out


def montar():
    t.clima()
    t.linha(6, pull=False, fill=False)
    cmd("clear @s")
    cmd("kill @e[type=item]")
    cmd("data merge block %s {Items:[%s]}" % (txt(at((0, 0, 0))), ",".join(i[2] for i in ITENS)))
    wait(10)
    print("origem:", componentes_de((0, 0, 0)))
    antes = hover_e_print((0, 0, 0), "componentes_antes", "origem")
    os.makedirs(LOCAL, exist_ok=True)
    json.dump(antes, open(f"{LOCAL}/antes.json", "w"))
    # O puxar liga e o servidor anda até as pilhas estarem em trânsito, com mais de uma ao mesmo tempo.
    cmd("data merge block %s {probe_set:{raw:[I;0,0,0,0,2,0]}}" % txt(at((1, 0, 0))))
    wait(60)
    cmd("tick freeze")
    # O NBT do transportador só é escrito no salvamento do nível: salva antes de ler.
    h.ctl("save")
    wait(20)
    emvoo = 0
    for x in range(1, 7):
        nbt = t.block_nbt((x, 0, 0), "items")
        if "components" in nbt:
            emvoo += 1
        print("   transportador", x, "items:", nbt[:300])
    print("transportadores com componentes no NBT salvo:", emvoo, "| origem:", componentes_de((0, 0, 0)), "| destino:", componentes_de((7, 0, 0)))
    return emvoo > 0


def conferir():
    time.sleep(5)
    for x in range(1, 7):
        print("   transportador", x, "items:", t.block_nbt((x, 0, 0), "items")[:300])
    cmd("tick unfreeze")
    wait(400)
    time.sleep(4)
    destino = componentes_de((7, 0, 0))
    origem = componentes_de((0, 0, 0))
    print("origem:", origem, "| destino:", destino)
    esperado = {i[3]: i[2] for i in ITENS}
    ok = True
    for item in esperado:
        ok &= bool(destino.get(item))
    ok &= not origem
    t.clima()
    depois = hover_e_print((7, 0, 0), "componentes_depois", "destino")
    antes = json.load(open(f"{LOCAL}/antes.json")) if os.path.exists(f"{LOCAL}/antes.json") else []
    for (rotulo, a, (ax, ay)), (_, d, (dx, dy)) in zip(antes, depois):
        # O recorte do tooltip: a partir do slot de cada tela, o mesmo trecho nos dois prints.
        cortes = []
        for caminho, (sx, sy) in ((a, (ax, ay)), (d, (dx, dy))):
            out = caminho + ".corte.png"
            subprocess.run(["magick", caminho, "-crop", f"520x260+{sx - 10}+{sy - 10}", "+repage", out], check=True)
            cortes.append(out)
        saida = f"{LOCAL}/componentes_{rotulo}_antes_depois.png"
        subprocess.run(["magick", *cortes, "-append", saida], check=True)
        print("recorte", rotulo, saida)
    return ok


if __name__ == "__main__":
    what = sys.argv[1] if len(sys.argv) > 1 else "funil"
    if what == "funil":
        sys.exit(0 if funil() else 1)
    if what == "componentes":
        sys.exit(0 if (montar() if sys.argv[2] == "montar" else conferir()) else 1)
    raise SystemExit("subcomando " + what)
