#!/usr/bin/env python3
"""As partículas da máquina acesa no port (`horizon`, H_NAME=horizon), com o mesmo cenário da `CenaParticulas` do original: a máquina no mesmo lugar, o jogador a 3 blocos da
face onde caem as partículas, olhando para o centro do bloco, um chão de pedra e uma caixa de ar em volta. O build é o de teste (`FX_TRACE = true` em `glue.rs`, que
`enviar_fontes.sh` liga): cada vez que o `animateTick` de um bloco aceso solta partículas, o jogo escreve `MEKFX gt=<tick do jogo> block=... n=<partículas>` no stderr.

Cada etapa: escreve `config/mekanism/client.toml` (`machineEffects`), põe o bloco com `setblock` (o tile nasce com o padrão), liga (`data merge`: item e energia; o gerador queima carvão e a
energia é esvaziada a cada 0,25 s, porque cheia ele pára), espera o bloco acender, e conta as chamadas do log entre o tick de jogo do começo e o do fim. O jogador fica em pé num bloco de barreira
(os pés em y = -60,0 como o `tp` a cada tick da cena do original). `prints=T1,T2` tira prints (por `horizon-ctl screenshot`) nos ticks T de jogo depois do começo.

Uso: H_NAME=horizon h2_particulas.py LOG_NO_DESKTOP SAIDA.jsonl PASTA_DOS_PRINTS_NO_DESKTOP [etapa ...]
  etapas: chamber, heat, chamber_sem_efeitos (as três por padrão)
"""
import json, os, re, shlex, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h
import h2_som_medida as m

POS = m.POS
CENTER = m.CENTER
CONFIG = m.CONFIG
TICK = 0.05
# nome: (bloco, pés do jogador relativos ao centro, yaw, pitch, ticks de jogo, machineEffects, prints em ticks depois do começo)
ETAPAS = {
    "chamber": ("mekanism:enrichment_chamber", (0.0, -0.5, 3.0), 180.0, 20.0, 2400, True, (200, 203, 206, 209, 212, 215)),
    "heat": ("mekanismgenerators:heat_generator", (0.0, -0.5, -3.0), 0.0, 20.0, 1200, True, (200, 203, 206, 209, 212, 215)),
    "chamber_sem_efeitos": ("mekanism:enrichment_chamber", (0.0, -0.5, 3.0), 180.0, 20.0, 300, False, ()),
}
FUEL = {"mekanismgenerators:heat_generator": 'Items:[{Slot:0b,id:"minecraft:coal",count:64}]'}
ENTRADA = {"mekanism:enrichment_chamber": 'Items:[{Slot:0b,id:"minecraft:coal",count:64}]'}
POS_S = " ".join(map(str, POS))


def ssh(text):
    return subprocess.run(["ssh", "desktop", text], capture_output=True, text=True).stdout


def config(effects):
    """`machineEffects` no `config/mekanism/client.toml`: troca o valor da chave que já existe (o jogo escreve a seção com o padrão na primeira leitura) ou acrescenta a seção."""
    valor = str(effects).lower()
    script = (
        "import os, re\n"
        f"p = os.path.expanduser({CONFIG!r})\n"
        "os.makedirs(os.path.dirname(p), exist_ok=True)\n"
        "t = open(p).read() if os.path.exists(p) else ''\n"
        f"n = re.subn(r'(machineEffects\\s*=\\s*)(true|false)', r'\\g<1>{valor}', t)\n"
        "if n[1]:\n"
        "    t = n[0]\n"
        "else:\n"
        "    t = t.rstrip('\\n') + ('\\n\\n' if t.strip() else '') + "
        f"\"\\t#Settings for configuring Mekanism's Particles\\n\\t[rendering.particle]\\n\\t\\t#Show particles when machines active.\\n\\t\\tmachineEffects = {valor}\\n\"\n"
        "open(p, 'w').write(t)\n"
    )
    ssh("python3 - <<'PY'\n" + script + "PY")


def gametime():
    chat = h.command("time query gametime")[0]
    return int(re.search(r"(-?\d+)", chat.split("is")[-1]).group(1))


def block_lit(block):
    """`horizon-ctl block X Y Z`: o estado do bloco, com a propriedade `lit`."""
    return "lit=true" in h.ctl("block", *map(str, POS))


def power(block):
    if block == "mekanismgenerators:heat_generator":
        h.command(f"data merge block {POS_S} {{energy:0,{FUEL[block]}}}")
    else:
        h.command(f"data merge block {POS_S} {{energy:1000,{ENTRADA[block]}}}")


def stage(nome, log, out, shots):
    block, feet, yaw, pitch, ticks, effects, prints = ETAPAS[nome]
    config(effects)
    time.sleep(2.5)
    h.command("tick unfreeze")
    h.command("gamemode creative")
    x, y, z = POS
    h.command(f"fill {x - 8} -61 {z - 8} {x + 8} -40 {z + 12} minecraft:air")
    h.command(f"fill {x - 8} -62 {z - 8} {x + 8} -62 {z + 12} minecraft:stone")
    h.command(f"setblock {POS_S} minecraft:air")
    h.command(f"setblock {POS_S} {block}[facing=south]")
    px, py, pz = CENTER[0] + feet[0], CENTER[1] + feet[1], CENTER[2] + feet[2]
    h.command(f"setblock {int(px // 1)} {int(py // 1) - 1} {int(pz // 1)} minecraft:barrier")
    h.command(f"tp @s {px:.3f} {py:.3f} {pz:.3f} {yaw} {pitch}")
    time.sleep(1.5)
    power(block)
    # Espera acender (o primeiro item leva alguns ticks), repondo a energia.
    for _ in range(40):
        time.sleep(0.25)
        power(block)
        if block_lit(block):
            break
    time.sleep(1.0)
    start_bytes = int(ssh(f"stat -c %s {log}"))
    t0 = gametime()
    wall0 = time.monotonic()
    pending = list(prints)
    while True:
        time.sleep(0.25)
        power(block)
        now = gametime()
        while pending and now - t0 >= pending[0]:
            h.ctl("screenshot", f"{shots}/particulas_port_{nome}_{pending.pop(0)}.png")
        if now - t0 >= ticks:
            break
    t1 = gametime()
    ativo = block_lit(block)
    end_bytes = int(ssh(f"stat -c %s {log}"))
    text = ssh(f"tail -c +{start_bytes + 1} {log} | head -c {end_bytes - start_bytes} | grep -a MEKFX")
    calls = [int(re.search(r"gt=(-?\d+)", line).group(1)) for line in text.splitlines() if "MEKFX" in line]
    calls = [gt for gt in calls if t0 <= gt <= t1]
    per_call = 1
    for line in text.splitlines():
        found = re.search(r" n=(\d+)", line)
        if found:
            per_call = int(found.group(1))
            break
    row = {
        "e": "resumo",
        "nome": nome,
        "bloco": block,
        "efeitos": effects,
        "tick_inicial": t0,
        "tick_final": t1,
        "ticks": t1 - t0,
        "acesa_no_fim": ativo,
        "chamadas": len(calls),
        "particulas_por_chamada": per_call,
        "particulas": len(calls) * per_call,
        "segundos_de_relogio": round(time.monotonic() - wall0, 1),
    }
    out.write(json.dumps(row) + "\n")
    out.flush()
    print(row, flush=True)
    h.command(f"setblock {POS_S} minecraft:air")
    h.command(f"setblock {int(px // 1)} {int(py // 1) - 1} {int(pz // 1)} minecraft:air")


def main():
    log, saida, shots = sys.argv[1:4]
    only = sys.argv[4:] or list(ETAPAS)
    ssh(f"mkdir -p {shots}")
    m.silenciar()
    antes = ssh(f"cat {CONFIG} 2>/dev/null")
    out = open(saida, "w", encoding="utf-8")
    try:
        for nome in only:
            stage(nome, log, out, shots)
    finally:
        # O arquivo de antes volta (ou some, se não havia).
        if antes:
            ssh(f"printf %s {shlex.quote(antes)} > {CONFIG}")
        else:
            ssh(f"rm -f {CONFIG}")


if __name__ == "__main__":
    main()
