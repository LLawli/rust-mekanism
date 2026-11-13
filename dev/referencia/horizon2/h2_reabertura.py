#!/usr/bin/env python3
"""O mundo salvo e reaberto, com e sem o servidor parado (frente `pendencias-rede`, Fase 3, decisão 156): cinco cabos e cinco condutores com o Heat Generator.

    H_NAME=horizon PASTA=<pasta do game.log> h2_reabertura.py montar [parar]     # monta as duas linhas, anota as redes, (parar: `tick freeze`), salva e fecha o jogo
    H_NAME=horizon PASTA=<pasta do game.log> h2_reabertura.py conferir [parar]   # com o mundo reaberto: (parar: `tick freeze` logo que o jogo está pronto), lê as redes e põe um cabo e um condutor novos

Quem chama fecha o jogo depois do `montar` e abre o mundo de novo (`abrir_horizon.sh`) antes do `conferir`. A sonda (`TRANSMITTER_PROBE = true` na cópia de teste) escreve no stderr do
jogo (`PASTA/game.log`): `MBE` as redes de energia a cada `ServerTickEvent.Post`, `MBQ` as de calor, e `MBT` os eventos do jogo (`clearRemoved`...). O que se mede é se o dono dos
transmissores recebe os eventos dos block entities lidos do disco (linhas `MBT`, `MBE`, `MBQ`) e se as redes se refazem, com as quatro combinações de parar antes de salvar e parar logo
depois de reabrir.
"""
import json, os, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("H_NAME", "horizon")
import h2_identity as h

PASTA = os.environ.get("PASTA", "~/.cache/rust-mekanism/restos")
BASE = (0, 150, 0)
CABLE = "mekanism:basic_universal_cable"
CONDUCTOR = "mekanism:basic_thermodynamic_conductor"
GENERATOR = "mekanismgenerators:heat_generator"
CABLE_Z, CONDUCTOR_Z = 0, 4


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


def ssh(command):
    return subprocess.run(["ssh", "desktop", command], capture_output=True, text=True, timeout=120).stdout


def log_lines(prefix):
    out = ssh(f"grep -a '^{prefix} ' {PASTA}/game.log | cut -c{len(prefix) + 2}-")
    rows = []
    for line in out.splitlines():
        try:
            rows.append(json.loads(line))
        except ValueError:
            pass
    return rows


def report(label):
    """Quantas linhas cada sonda deu desde que o jogo abriu e a última rede de cada família."""
    events = ssh(f"grep -a -c '^MBT .*\"e\":\"clearRemoved\"' {PASTA}/game.log").strip()
    cables = log_lines("MBE")
    heat = log_lines("MBQ")
    print(f"[{label}] clearRemoved: {events}; linhas MBE: {len(cables)}; linhas MBQ: {len(heat)}")
    print(f"[{label}] redes de energia (última linha): {cables[-1]['nets'] if cables else None}")
    print(f"[{label}] redes de calor (última linha): {[r['nets'] for r in heat[-1:]]}")
    return cables, heat


def montar(parar):
    cmd("tick unfreeze")
    cmd("fill %d %d %d %d %d %d minecraft:stone" % (BASE[0] - 4, BASE[1] - 1, BASE[2] - 4, BASE[0] + 10, BASE[1] - 1, BASE[2] + 10))
    cmd("fill %d %d %d %d %d %d minecraft:air" % (BASE[0] - 4, BASE[1], BASE[2] - 4, BASE[0] + 10, BASE[1] + 4, BASE[2] + 10))
    cmd("gamemode creative @s")
    cmd("clear @s")
    for x in range(5):
        cmd("setblock %s %s" % (txt(at(x, 0, CABLE_Z)), CABLE))
        cmd("setblock %s %s" % (txt(at(x, 0, CONDUCTOR_Z)), CONDUCTOR))
    cmd("setblock %s %s" % (txt(at(4, 1, CONDUCTOR_Z)), GENERATOR))
    wait(60)
    cmd("data merge block %s {energy:31001}" % txt(at(0, 0, CABLE_Z)))
    wait(30)
    cables, heat = report("antes de fechar")
    good = bool(cables) and cables[-1]["nets"] and cables[-1]["nets"][0][3] == 5 and bool(heat) and heat[-1]["nets"] and heat[-1]["nets"][0][3] == 5
    print(("OK   " if good else "DIFF ") + "as duas redes de 5 estavam de pé antes de salvar")
    if parar:
        cmd("tick freeze")
        time.sleep(1.5)
        print("com o servidor parado:", cmd("tick query"))
    h.ctl("save")
    wait(20)
    print("depois do save:", cmd("tick query"))
    h.ctl("quit")
    return good


def conferir(parar):
    h.ctl("wait-ready")
    if parar:
        cmd("tick freeze")
    wait(120)
    print("depois de reabrir:", cmd("tick query"))
    cables, heat = report("reaberto" + (", parado" if parar else ""))
    reopened = bool(cables) and bool(cables[-1]["nets"]) and cables[-1]["nets"][0][3] == 5 and bool(heat) and bool(heat[-1]["nets"]) and heat[-1]["nets"][0][3] == 5
    if parar:
        cmd("tick step 5")
        time.sleep(0.6)
        cables, heat = report("reaberto, parado, 5 ticks depois")
        reopened = bool(cables) and bool(cables[-1]["nets"]) and cables[-1]["nets"][0][3] == 5 and bool(heat) and bool(heat[-1]["nets"]) and heat[-1]["nets"][0][3] == 5
    print(("OK   " if reopened else "DIFF ") + "as duas redes de 5 se refizeram sozinhas")
    # Um cabo e um condutor novos ao lado de cada linha: se os cinco antigos estão no dono, entram na rede de 6.
    cmd("setblock %s %s" % (txt(at(5, 0, CABLE_Z)), CABLE))
    cmd("setblock %s %s" % (txt(at(5, 0, CONDUCTOR_Z)), CONDUCTOR))
    if parar:
        cmd("tick step 5")
        time.sleep(0.6)
    else:
        wait(60)
    cables, heat = report("com um cabo e um condutor novos")
    joined = bool(cables) and bool(cables[-1]["nets"]) and cables[-1]["nets"][0][3] == 6 and bool(heat) and bool(heat[-1]["nets"]) and heat[-1]["nets"][0][3] == 6
    print(("OK   " if joined else "DIFF ") + "os novos entraram nas redes dos cinco")
    cmd("tick unfreeze")
    return reopened and joined


def count(pattern):
    return ssh(f"grep -a -c '{pattern}' {PASTA}/game.log").strip()


def cedo():
    """O mundo reaberto e parado antes do primeiro tick do block entity: o `clearRemoved` só marca `markJoined`, e quem junta o transmissor à rede é o ticker do block entity (`onUpdateServer`),
    que não anda com o servidor parado. Sem tick, nenhum condutor nem cabo está na rede (nem `MBV` nem `MBH`); um `tick step 1` junta os dez; um condutor posto agora entra sozinho (`onAdded`)."""
    def snap(label):
        cables = log_lines("MBE")
        heat = log_lines("MBQ")
        print(f"[{label}] clearRemoved {count(chr(34) + 'e' + chr(34) + ':' + chr(34) + 'clearRemoved')}; MBV {count('^MBV ')}; MBH {count('^MBH ')}; linhas MBE {len(cables)} (runs {sum(1 for r in cables if r['runs'])}); "
              f"redes de energia {cables[-1]['nets'] if cables else None}; redes de calor {heat[-1]['nets'] if heat else None}")
        return cables, heat

    print("estado do relógio:", cmd("tick query"))
    wait(60)
    cables, heat = snap("parado, sem nenhum tick")
    nothing = not cables[-1]["nets"] and not heat[-1]["nets"] and not any(r["runs"] for r in cables)
    cmd("setblock %s %s" % (txt(at(5, 0, CONDUCTOR_Z)), CONDUCTOR))
    wait(30)
    cables, heat = snap("parado, com um condutor novo posto")
    cmd("tick step 1")
    time.sleep(0.6)
    cables, heat = snap("depois de tick step 1")
    stepped = bool(cables[-1]["nets"]) and bool(heat[-1]["nets"])
    cmd("tick unfreeze")
    wait(60)
    cables, heat = snap("andando")
    running = bool(cables[-1]["nets"]) and cables[-1]["nets"][0][3] == 5 and bool(heat[-1]["nets"]) and heat[-1]["nets"][0][3] == 6
    print(("OK   " if nothing else "DIFF ") + "parado desde o primeiro instante, nenhum cabo nem condutor está na rede")
    print(("OK   " if stepped else "DIFF ") + "um tick juntou os cabos e os condutores")
    print(("OK   " if running else "DIFF ") + "andando, a rede dos cabos tem 5 e a dos condutores 6")
    return nothing and stepped and running


def main():
    what = sys.argv[1] if len(sys.argv) > 1 else ""
    parar = len(sys.argv) > 2 and sys.argv[2] == "parar"
    if what == "montar":
        sys.exit(0 if montar(parar) else 1)
    if what == "conferir":
        sys.exit(0 if conferir(parar) else 1)
    if what == "cedo":
        sys.exit(0 if cedo() else 1)
    raise SystemExit("uso: h2_reabertura.py montar [parar] | conferir [parar] | cedo")


if __name__ == "__main__":
    main()
