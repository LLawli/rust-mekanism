#!/usr/bin/env python3
"""Roda no port (container `horizon`, dois clientes) o MESMO roteiro que o original roda com `testes/CenaDois.java` (`oficial_dois.sh`): cada linha de `cenas/dois_*.txt` vira
o que o port faz no lugar do `performPrefixedCommand` e do clique de cliente do Java. Assim os dois jogos passam pelos mesmos passos, na mesma ordem.

Uso: H_NAME=horizon h2_dois_roteiro.py ROTEIRO.txt DESTINO
Os prints saem em DESTINO: `port_dois_<nome>.png` (o convidado, como `convidado_oficial_dois_<nome>.png` no original) e `port_anf_<nome>.png` (o anfitrião).
O que o original grava como linha `ler`/`tile` (o XP do convidado, o fator de volume do tile no cliente dele) vai para `DESTINO/port_dois.log`: o XP pelo `data get entity`, e o tile
pelas linhas `MEKSOUND` do log do cliente convidado (`~/.cache/rust-mekanism/dois-clientes/b.log`, só numa cópia de teste com `TRACE = true` em `tile_sound.rs`).

Ações que o roteiro conhece (as do cabeçalho de `CenaDois.java`): lan, espera_convidado, srv, congela, descongela, passos, espera, caixa, montar, estado, silencio, liga, desliga, anf/conv
(abrir, fechar, foto, escala, clicar, cursor, ler, observar, tile), fim. O anfitrião do original se chama `Dev`; o do port, `Player90366`."""
import json, math, os, re, subprocess, sys, time

TOOLS = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, TOOLS)
os.environ.setdefault("H_NAME", "horizon")
import h2_dois as d

SHOTS = "/home/luka/.local/share/jogo-isolado/horizon/data/shots/dois"
LOG_B = "~/.cache/rust-mekanism/dois-clientes/b.log"
SAVES = "/home/luka/.local/share/jogo-isolado/horizon/data/horizon/saves"
HOST = os.environ.get("H2_HOST", "Player90366")  # `H2_HOST=Dev` quando o anfitrião abre com `ANF_EXTRA="--name Dev"` (o nome do dono aparece na tela da Security Desk)
Y, Z = -60, 3
BLOCK = {"chamber": "mekanism:enrichment_chamber", "crusher": "mekanism:crusher", "smelter": "mekanism:energized_smelter", "infuser": "mekanism:metallurgic_infuser",
         "generator": "mekanismgenerators:heat_generator"}
CAP = {"chamber": 1000, "crusher": 1600, "smelter": 1000, "infuser": 1000, "generator": 240}
SLOTS = {"chamber": (0, 1), "crusher": (0, 1), "smelter": (0, 1), "infuser": (1, 2), "generator": (0, 1)}

machine_at = {}
frozen = False
scale = 3
dest = "."
log = None
observing = None


def say(text):
    print(text, flush=True)
    log.write(text + "\n")
    log.flush()


def host(*a):
    return d.host(*a)


def hcmd(text):
    return json.loads(host("command", text.replace("Dev", HOST)))["chat"]


def who_of(name):
    return d.host if name == "anf" else d.guest


panel_size = [176, 166]


def panel():
    gl, ga = -(-1920 // scale), -(-1080 // scale)
    return (gl - panel_size[0]) // 2, (ga - panel_size[1]) // 2


def xp_of(name):
    out = {}
    for key in ("XpTotal", "XpLevel", "XpP"):
        chat = hcmd(f"data get entity {name} {key}")[0]
        m = re.search(r"([-0-9.]+)[A-Za-z]?$", chat.strip())
        out[key] = m.group(1) if m else chat
    return out


def pull(remote, local):
    subprocess.run(["scp", "-q", f"desktop:{remote}", local], check=True)


def ssh(text):
    return subprocess.run(["ssh", "desktop", text], capture_output=True, text=True).stdout


def nbt_items(machine, args):
    slot_in, slot_out = SLOTS[machine]
    items = []
    for key, slot in (("entrada", slot_in), ("saida", slot_out)):
        if key in args:
            item, n = args[key].rsplit(":", 1)
            if ":" not in item:
                item = "minecraft:" + item
            items.append(f'{{Slot:{slot}b,id:"{item}",count:{n}}}')
    return items


def act_montar(machine, x):
    machine_at[x] = machine
    pos = f"{x} {Y} {Z}"
    hcmd(f"setblock {pos} minecraft:air")
    if frozen:
        # Um passo sem a máquina (como `monta_port`): o objeto vivo da máquina que estava ali (o mundo parado só anda nos passos) deixa de ser do tick seguinte, e a nova nasce do NBT
        # em vez de continuar o objeto velho (que ficaria sem os upgrades de `estado`).
        hcmd("tick step 1")
        time.sleep(0.6)
    # O anfitrião põe a máquina (como `monta_port`): vai até perto dela e volta ao ponto em que o roteiro o deixou.
    antes = json.loads(host("state"))["player"]
    # O convidado sai da frente (o `place` do `horizon-ctl` clica na face do vizinho e um jogador no caminho o bloqueia) e volta depois.
    conv = d.guest_json("state")["player"]
    hcmd(f"tp {d.GUEST_NAME} {x + 4}.0 -60.0 -3.0 0 0")
    hcmd(f"tp Dev {x}.0 -60.0 0.0 0 0")
    time.sleep(0.5)
    hcmd("clear Dev")
    time.sleep(0.8)
    host("give", BLOCK[machine], "1")
    out = host("place", str(x), str(Y), str(Z), "--item", BLOCK[machine])
    if '"error"' in out:
        raise SystemExit(f"place {machine}: {out}")
    hcmd("clear Dev")
    time.sleep(0.8)
    px, py, pz = antes["position"]
    hcmd(f"tp Dev {px} {py} {pz} {antes.get('yaw', 0)} {antes.get('pitch', 0)}")
    cx, cy, cz = conv["position"]
    hcmd(f"tp {d.GUEST_NAME} {cx} {cy} {cz} {conv.get('yaw', 0)} {conv.get('pitch', 0)}")
    time.sleep(0.3)


def act_estado(x, args):
    machine = machine_at[x]
    pos = f"{x} {Y} {Z}"
    nbt = []
    if "upgrades" in args:
        mapa = ",".join(f'"mekanism:{t}":{n}' for t, n in (p.split(":") for p in args["upgrades"].split(",")))
        hcmd(f"data merge block {pos} {{component_upgrade:{{upgrades:{{{mapa}}}}},redstone:0b}}")
        hcmd("tick step 1")
        time.sleep(0.6)
    if "energia" in args:
        nbt.append("energy:%d" % (CAP[machine] if args["energia"] == "cap" else int(args["energia"])))
    items = nbt_items(machine, args)
    if items:
        nbt.append("Items:[" + ",".join(items) + "]")
    if "quimico" in args:
        tipo, n = args["quimico"].split(":")
        nbt.append(f'chemical_tanks:[{{container:0b,stored:{{type:"mekanism:{tipo}",amount:{n}L}}}}]')
    if "lava" in args:
        nbt.append(f'machinery_basis:{{v:1,tanks:[{{resource:"minecraft:lava",amount:{args["lava"]}L}}]}}')
    if nbt:
        hcmd(f"data merge block {pos} {{{','.join(nbt)}}}")


def act_client(who_name, a):
    who = who_of(who_name)
    verb = a[0]
    left, top = panel()
    if verb == "abrir":
        for _ in range(4):
            who("interact", *a[1:4])
            time.sleep(0.8)
            if '"screen":"container"' in who("screen").replace(" ", ""):
                break
        who("look", "0", "0")
    elif verb == "usar":
        # Um clique direito só (o `abrir` repete até a tela abrir: com o Configurator na mão ele giraria a cada tentativa).
        say(f"usar {who_name} {' '.join(a[1:4])}: {who('interact', *a[1:4]).strip()[:200]}")
        time.sleep(0.6)
    elif verb == "agachar":
        who("key", "shift", "down" if a[1] == "1" else "up")
        time.sleep(0.3)
    elif verb == "fechar":
        who("close")
    elif verb == "foto":
        remote = f"{SHOTS}/{who_name}_{a[1]}.png"
        who("screenshot", remote)
        time.sleep(0.5)
        pull(remote, f"{dest}/port_{'dois' if who_name == 'conv' else 'anf'}_{a[1]}.png")
    elif verb == "escala":
        global scale
        scale = int(a[1])
        who("options-set", "guiScale", a[1])
    elif verb == "clicar":
        who("click-at", str(left + int(a[1])), str(top + int(a[2])), *(["right"] if len(a) > 3 and a[3] == "d" else []))
    elif verb == "cursor":
        who("scroll-at", str(left + int(a[1])), str(top + int(a[2])), "0")
    elif verb == "digitar":
        for ch in a[1]:
            who("type", ch)
            time.sleep(0.05)
    elif verb == "tecla":
        say(f"tecla {who_name} {a[1]}: {who('key', a[1], 'tap').strip()}")
    elif verb == "roda":
        who("scroll-at", str(left + int(a[1])), str(top + int(a[2])), a[3])
    elif verb == "painel":
        panel_size[:] = [int(a[1]), int(a[2])]
    elif verb == "ler":
        try:
            scr = json.loads(who("screen"))
            tela = scr.get("screen")
            cabeca = next((e["item"] for e in scr.get("slots", []) if e["slot"] == 500), None)
        except json.JSONDecodeError:
            tela, cabeca = "?", None
        say(f"ler {who_name} {a[1]} tela={tela} cabeca(slot 500)={cabeca} xp convidado={xp_of(d.GUEST_NAME)} anfitriao={xp_of(HOST)}")
    elif verb == "observar":
        global observing
        observing = (a[2], int(a[1]), int(ssh(f"stat -c %s {LOG_B}")), time.time())
    elif verb == "tile":
        say(f"tile {a[1]} (o port só tem o fator nas linhas MEKSOUND do `observar`)")
    elif verb == "tipos":
        say("tipos: o cliente do port não expõe o registro; veja a janela de upgrades (foto)")


def end_observation():
    global observing
    if not observing:
        return
    name, n, start, t0 = observing
    wait = t0 + n * 0.05 - time.time()
    if wait > 0:
        time.sleep(wait)
    time.sleep(0.5)
    end = int(ssh(f"stat -c %s {LOG_B}"))
    text = ssh(f"tail -c +{start + 1} {LOG_B} | head -c {end - start} | grep -a MEKSOUND")
    say(f"observar {name}:\n{text}")
    observing = None


def main():
    global dest, log, frozen
    roteiro, dest = sys.argv[1], sys.argv[2]
    os.makedirs(dest, exist_ok=True)
    log = open(f"{dest}/port_dois.log", "w", encoding="utf-8")
    ssh(f"mkdir -p {SHOTS}")
    for who in (d.host, d.guest):
        who("options-set", "chatVisibility", "2")
    for line in open(roteiro, encoding="utf-8"):
        # O original monta em x = 300 (o mundo dele tem o chunk carregado); o mundo `dois` do port, perto do spawn: o mesmo roteiro, o x deslocado de 297 (300 vira 3, como `cena_telas.py`).
        a = [str(float(t) - 297).rstrip("0").rstrip(".") if re.fullmatch(r"3\d\d(\.\d+)?", t) else t for t in line.split()]
        if not a or a[0].startswith("#"):
            continue
        say(f"> {line.strip()}")
        verb = a[0]
        if verb in ("lan", "espera_convidado"):
            continue
        if verb == "srv":
            hcmd(" ".join(a[1:]))
        elif verb == "congela":
            hcmd("tick freeze")
            frozen = True
        elif verb == "descongela":
            hcmd("tick unfreeze")
            frozen = False
        elif verb == "passos":
            hcmd(f"tick step {a[1]}")
            time.sleep(0.35 + 0.05 * int(a[1]))
        elif verb == "espera":
            time.sleep(0.05 * int(a[1]) + 0.1)
        elif verb == "caixa":
            x = int(a[1])
            hcmd(f"fill {x - 6} -61 -6 {x + 6} -50 10 minecraft:air")
            hcmd(f"fill {x - 6} -61 -6 {x + 6} -50 10 minecraft:black_concrete hollow")
            hcmd(f"fillbiome {x - 6} -61 -6 {x + 6} -50 10 minecraft:plains")
        elif verb == "montar":
            act_montar(a[1], int(a[2]))
        elif verb == "por":
            # `por JOGADOR ITEM X`: o jogador põe o bloco (o `place` do `horizon-ctl`, que é o clique dele) e fica o dono.
            quem, nome = (d.host, HOST) if a[1] == "Dev" else (d.guest, d.GUEST_NAME)
            hcmd(f"clear {nome}")
            time.sleep(0.6)
            hcmd(f"give {nome} {a[2]} 1")
            time.sleep(0.6)
            for _ in range(4):
                out = quem("place", a[3], str(Y), str(Z), "--item", a[2]).strip()
                if '"error"' not in out:
                    break
                time.sleep(0.6)
            say(f"por {a[1]} {a[2]}: {out[:160]}")
            time.sleep(0.3)
            hcmd(f"clear {nome}")
        elif verb == "nbt":
            say(f"nbt {a[1]} " + str(hcmd(f"data get block {a[2]} {a[3]} {a[4]}")))
        elif verb == "bloco":
            # O estado do bloco no servidor (a direção fica no estado).
            say(f"bloco {a[1]} " + d.host("block", a[2], a[3], a[4]).strip()[:300])
        elif verb == "freq":
            # A tabela das frequências no disco (o port não tem como mostrar a da memória): o arquivo do mundo, lido no desktop.
            arquivo = ssh(f"ls -t {SAVES}/*/data/mekanism/frequency/security/public.dat 2>/dev/null | head -1").strip()
            say(f"freq {a[1]} disco " + (ssh(f"python3 -I ~/.cache/rust-mekanism/nbt_ler.py '{arquivo}' 2>&1 | tail -1 | head -c 3000").strip() if arquivo else "nenhum arquivo"))
        elif verb == "estado":
            act_estado(int(a[1]), dict(p.split("=", 1) for p in a[2:]))
        elif verb == "silencio":
            # O upgrade só entra pelo jogador (o item em cima da máquina, `h2_som_medida.add_muffling`, ou o botão Uninstall da janela, `remove_muffling`): o anfitrião vai até perto
            # da máquina, faz o gesto e volta ao ponto em que o roteiro o deixou.
            antes = json.loads(host("state"))["player"]
            x = a[1]
            hcmd(f"tp Dev {x}.0 -60.0 0.0 0 0")
            time.sleep(0.4)
            if a[2] != "0":
                hcmd(f'give Dev mekanism:upgrade[mekanism:upgrade_type="mekanism:muffling"] {a[2]}')
                time.sleep(0.4)
                host("key", "shift", "down")
                host("interact", x, str(Y), str(Z))
                host("key", "shift", "up")
                time.sleep(0.4)
            else:
                left, top = panel()
                host("interact", x, str(Y), str(Z))
                time.sleep(1.5)
                for gx, gy in ((190, 19), (29, 40), (132, 75)):
                    host("click-at", str(left + gx), str(top + gy))
                    time.sleep(0.4)
                host("close")
            px, py, pz = antes["position"]
            hcmd(f"tp Dev {px} {py} {pz} {antes.get('yaw', 0)} {antes.get('pitch', 0)}")
        elif verb == "liga":
            hcmd(f'data merge block {a[1]} {Y} {Z} {{energy:1000,Items:[{{Slot:0b,id:"minecraft:coal",count:64}}]}}')
        elif verb == "desliga":
            hcmd(f"data merge block {a[1]} {Y} {Z} {{energy:0}}")
        elif verb in ("anf", "conv"):
            if a[1] == "observar":
                end_observation()
            act_client(verb, a[1:])
        elif verb == "fim":
            break
        else:
            say(f"ação desconhecida {verb}")
    end_observation()
    hcmd("tick unfreeze")


if __name__ == "__main__":
    main()
