#!/usr/bin/env python3
"""Confere no jogo (container `horizon`, `H_NAME=horizon`) a configuração do Mekanism e do Mekanism Generators: os arquivos que o jogo escreve em `config/Mekanism/`, a correção de um
valor errado com a cópia `.bak`, e o efeito de cada chave portada num bloco posto. Os arquivos são lidos e escritos no desktop (ssh); o jogo relê o que muda em até um segundo.

Uso: H_NAME=horizon h2_config.py arquivos|correcao|efeitos|tudo
  arquivos   o texto de cada chave dos seis arquivos novos contra o do original (`rastros/mekanism_*.toml`)
  correcao   um valor fora da faixa escrito com o jogo aberto: o arquivo volta corrigido, com o `-1.toml.bak`
  efeitos    consumo da Enrichment Chamber, queima de lava do Heat Generator e atraso do bloco aceso, com valores de `Mekanism/*.toml` mudados com o jogo aberto
"""
import json, os, re, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h

DIR = "~/.local/share/jogo-isolado/horizon/data/horizon/config/Mekanism"
FILES = {
    "client.toml": "client",
    "general.toml": "general",
    "machine-usage.toml": "machine_usage",
    "tiers.toml": "tiers",
    "generators.toml": "generators",
    "generator-storage.toml": "generator_storage",
}
SPOT = (70, -60, 80)
GENERATOR = (70, -60, 82)


def ssh(script):
    return subprocess.run(["ssh", "desktop", script], capture_output=True, text=True).stdout


def read_file(name):
    return ssh(f"cat {DIR}/{name}")


def write_key(name, key, value):
    """Troca o valor de `key` no arquivo (a primeira linha `key = ...`)."""
    ssh(f"sed -i 's/^\\([ \\t]*\\){key} = .*/\\1{key} = {value}/' {DIR}/{name}")


def block(text, section, key):
    """O comentário e a linha de uma chave, como nos testes em Rust."""
    current = ""
    lines = text.splitlines()
    for index, line in enumerate(lines):
        stripped = line.strip()
        if stripped.startswith("[") and stripped.endswith("]"):
            current = stripped[1:-1]
        elif current == section and stripped.startswith(f"{key} = "):
            first = index
            while first > 0 and lines[first - 1].lstrip().startswith("#"):
                first -= 1
            return "\n".join(lines[first:index + 1])
    return None


def reference(name):
    return open(f"{h.TRACES}/mekanism_{name}.toml").read()


# Cada arquivo: (seção, chaves) que o port lê.
KEYS = {
    "client.toml": [("sounds", "baseVolume"), ("sounds", "enableMachine")],
    "general.toml": [("", "blockDeactivationDelay"), ("", "maxUpgradeMultiplier")],
    "machine-usage.toml": [("", k) for k in ("enrichmentChamber", "crusher", "metallurgicInfuser", "energizedSmelter")],
    "tiers.toml": [("transmitters.energy", k) for k in ("basicCapacity", "advancedCapacity", "eliteCapacity", "ultimateCapacity")],
    "generators.toml": [("heat_generator", k) for k in ("heatGeneration", "heatGenerationLava", "heatGenerationNether", "tankCapacity", "heatGenerationFluidRate")],
    "generator-storage.toml": [("", "heatGenerator")],
}


def files():
    ok = True
    print(ssh(f"ls -la {DIR}"))
    for name, ref in FILES.items():
        text, original = read_file(name), reference(ref)
        for section, key in KEYS[name]:
            got, want = block(text, section, key), block(original, section, key)
            same = got is not None and got == want
            ok &= same
            print(("OK   " if same else "DIFF ") + f"{name} {section}.{key}")
            if not same:
                print("  esperado:", want)
                print("  obtido:  ", got)
    window = read_file("client.toml")
    expected_start = window.find("#Stores the last position")
    print(("OK   " if expected_start > 0 and "[window.upgrade]" in window else "DIFF ") + "client.toml: bloco [window] com window.upgrade")
    return ok


def correcao():
    """Um valor fora da faixa escrito com o jogo aberto: em até um segundo o arquivo é reescrito, e o de antes fica em `general-1.toml.bak`."""
    ssh(f"rm -f {DIR}/general-*.toml.bak")
    write_key("general.toml", "blockDeactivationDelay", "5000")
    time.sleep(4.0)
    text = read_file("general.toml")
    bak = ssh(f"cat {DIR}/general-1.toml.bak")
    fixed = "blockDeactivationDelay = 1200" in text
    saved = "blockDeactivationDelay = 5000" in bak
    print(("OK   " if fixed else "DIFF ") + "o valor 5000 voltou ao limite, 1200")
    print(("OK   " if saved else "DIFF ") + "a cópia general-1.toml.bak tem o arquivo de antes")
    ssh(f"rm -f {DIR}/general-*.toml.bak")
    write_key("general.toml", "blockDeactivationDelay", "60")
    time.sleep(3.0)
    return fixed and saved


def nbt(pos, path=""):
    return h.command(f"data get block {pos[0]} {pos[1]} {pos[2]} {path}")[0]


def energy_after_one_tick(initial, usage):
    """A Enrichment Chamber nova com `initial` de energia e três de barro: um tick de operação gasta `usage`."""
    p = " ".join(map(str, SPOT))
    h.command(f"setblock {p} minecraft:air")
    time.sleep(0.4)
    h.ctl("setblock", *map(str, SPOT), "mekanism:enrichment_chamber")
    time.sleep(1.5)
    h.command("tick freeze")
    h.command(f'data merge block {p} {{energy:{initial},Items:[{{Slot:0b,id:"minecraft:clay",count:3}}]}}')
    h.command("tick step 1")
    time.sleep(0.5)
    got = int(re.search(r"energy: (-?\d+)", nbt(SPOT)).group(1))
    h.command("tick unfreeze")
    return got


def lava_after_one_tick():
    """O Heat Generator novo com 1.000 mB de lava e energia vazia: um tick queima `heatGenerationFluidRate` de lava."""
    p = " ".join(map(str, GENERATOR))
    h.command(f"setblock {p} minecraft:air")
    time.sleep(0.4)
    h.ctl("setblock", *map(str, GENERATOR), "mekanismgenerators:heat_generator")
    time.sleep(1.5)
    h.command("tick freeze")
    h.command(f'data merge block {p} {{machinery_basis:{{v:1,tanks:[{{resource:"minecraft:lava",amount:1000L}}]}}}}')
    h.command("tick step 1")
    time.sleep(0.5)
    chat = nbt(GENERATOR)
    lava = int(re.search(r"amount: (\d+)L", chat).group(1))
    h.command("tick unfreeze")
    return lava


def geradores():
    ok = True
    write_key("generators.toml", "heatGenerationFluidRate", "250")
    time.sleep(3.0)
    got = lava_after_one_tick()
    ok &= got == 750
    print(("OK   " if got == 750 else "DIFF ") + f"Heat Generator com heatGenerationFluidRate = 250: 1.000 mB, 1 tick, ficou {got} (esperado 750)")
    # Acima do tanque (1.000, o que o jogo tem em cache): volta ao padrão, 100, e o arquivo é reescrito.
    write_key("generators.toml", "heatGenerationFluidRate", "3000")
    time.sleep(4.0)
    text = read_file("generators.toml")
    back = "heatGenerationFluidRate = 100" in text
    got = lava_after_one_tick()
    ok &= back and got == 900
    print(("OK   " if back and got == 900 else "DIFF ") + f"heatGenerationFluidRate = 3000 com o tanque de 1.000: voltou a 100 no arquivo ({back}) e ficou {got} de lava (esperado 900)")
    ssh(f"rm -f {DIR}/generators-*.toml.bak")
    return ok


def efeitos():
    ok = geradores()
    # Consumo da Enrichment Chamber: 5 por tick de padrão, 20 com `enrichmentChamber = 20`.
    write_key("machine-usage.toml", "enrichmentChamber", "20")
    time.sleep(3.0)
    got = energy_after_one_tick(100, 20)
    ok &= got == 80
    print(("OK   " if got == 80 else "DIFF ") + f"Enrichment Chamber com enrichmentChamber = 20: 100 de energia, 1 tick, ficou {got} (esperado 80)")
    write_key("machine-usage.toml", "enrichmentChamber", "5")
    time.sleep(3.0)
    got = energy_after_one_tick(100, 5)
    ok &= got == 95
    print(("OK   " if got == 95 else "DIFF ") + f"Enrichment Chamber com o padrão 5: ficou {got} (esperado 95)")
    return ok


def main():
    what = sys.argv[1] if len(sys.argv) > 1 else "tudo"
    ok = True
    if what in ("arquivos", "tudo"):
        ok &= files()
    if what in ("correcao", "tudo"):
        ok &= correcao()
    if what in ("efeitos", "tudo"):
        ok &= efeitos()
    print("TUDO IGUAL" if ok else "HÁ DIFERENÇAS")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
