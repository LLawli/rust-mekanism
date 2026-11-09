#!/usr/bin/env python3
"""Cria no `horizon`, pela tela de criar mundo, o mundo superplano de teste: nome, semente, sem estruturas e sem baú de bônus, criativo, pacífico, comandos ligados, tipo Superflat.

O jogo tem de estar na tela de título (`abrir_som_plano.sh titulo`; o `--quickPlaySingleplayer` de um mundo que não existe deixa o jogo no título). O mundo abre e fica aberto; as regras do
mundo (`advance_time`, `advance_weather`, `spawn_mobs`, a hora e o tempo) são do `h2_som_plano.py`, que as põe por comando nos dois jogos. A criação equivalente no original está em
`testes/CenaSomPlano.java` (`createFreshLevel` com `WorldPresets.FLAT`).

Uso: H_NAME=horizon h2_mundo_plano.py NOME [SEMENTE]   (padrão: a semente de `cenas/som_plano.txt`, 20261108)
"""
import json, os, subprocess, sys, time

TOOLS = os.path.dirname(os.path.abspath(__file__))


def ctl(*args):
    return subprocess.run([f"{TOOLS}/ctl.sh", *args], capture_output=True, text=True, timeout=180).stdout


def menu():
    return json.loads(ctl("state")).get("menu") or {}


def esperar(condicao, texto, segundos=60):
    for _ in range(segundos * 2):
        if condicao():
            return
        time.sleep(0.5)
    raise SystemExit(f"tempo esgotado: {texto}")


def clicar_ate(widget, campo, valor, maximo=6):
    """Clica no botão que cicla um valor (tipo de mundo, modo, dificuldade) até o `menu` mostrar `valor`."""
    for _ in range(maximo):
        if menu().get(campo) == valor:
            return
        ctl("click", widget)
        time.sleep(0.4)
    if menu().get(campo) != valor:
        raise SystemExit(f"{campo} não chegou a {valor}: {menu()}")


def main():
    nome = sys.argv[1]
    semente = sys.argv[2] if len(sys.argv) > 2 else "20261108"
    # O título só aceita cliques depois da tela de acessibilidade da primeira vez ("Continue").
    esperar(lambda: menu().get("mode") in ("main", "accessibility-onboarding"), "tela de título")
    if menu().get("mode") == "accessibility-onboarding":
        ctl("click-button", "Continue")
    esperar(lambda: menu().get("mode") == "main", "menu principal")
    ctl("click", "singleplayer")
    time.sleep(1.5)
    ctl("click", "create-new-world")
    esperar(lambda: menu().get("mode") == "create-world", "tela de criar mundo")
    ctl("type", "--field", "name", "--replace", nome)
    ctl("type", "--field", "seed", semente)
    clicar_ate("world-type", "world_type", "minecraft:flat")
    # Estruturas e baú de bônus: os botões ON e OFF da aba World; clica no que mostra ON.
    if menu().get("generate_structures"):
        ctl("click", "on")
        time.sleep(0.4)
    ctl("click", "game")
    time.sleep(0.5)
    clicar_ate("game-mode", "creative", True, 3)
    clicar_ate("difficulty", "difficulty", "peaceful", 4)
    if not menu().get("allow_commands"):
        ctl("click", "allow-commands")
        time.sleep(0.4)
    m = menu()
    esperado = {"world_type": "minecraft:flat", "generate_structures": False, "bonus_chest": False, "creative": True, "difficulty": "peaceful", "allow_commands": True}
    ruim = {k: (m.get(k), v) for k, v in esperado.items() if m.get(k) != v}
    if ruim or m["fields"]["name"] != nome or m["fields"]["seed"] != semente:
        raise SystemExit(f"a tela de criar mundo não ficou como o planejado: {ruim} {m['fields']}")
    print("criando", json.dumps(m, sort_keys=True))
    ctl("click", "create-new-world")
    esperar(lambda: json.loads(ctl("state")).get("world") == nome, "o mundo abrir", 120)
    ctl("wait-ready")
    print("mundo aberto:", nome)


if __name__ == "__main__":
    main()
