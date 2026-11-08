#!/usr/bin/env python3
"""A tela do convidado de LAN contra a do anfitrião, nos mesmos estados de `cena_telas.py` (a energia, o progresso, o tanque e os avisos que o transporte da parte 2 dos
trackers manda): a máquina é montada como `cena_telas.monta_port` (o anfitrião a põe, o servidor parado), os DOIS clientes abrem a tela e tiram o print, e a
`comparar_tela.comparar` conta os pixels diferentes dentro da tela e das abas (o anfitrião já foi comparado com o original: 0 pixel, ver o plano). O convidado vê o mesmo estado
por pacote, o anfitrião pelo espelho local, então um pixel de diferença é o transporte.

Uso: H_NAME=horizon h2_convidado_tela.py DESTINO [--so caso,caso] [--passos-a-passo N]
  `--passos-a-passo N`: depois de montar o caso, anda N ticks de um em um (`tick step 1`) e compara os dois prints a cada tick (o progresso e a energia andam)."""
import json, os, subprocess, sys, time

TOOLS = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, TOOLS)
sys.path.insert(0, os.path.join(TOOLS, ".."))
os.environ.setdefault("H_NAME", "horizon")
import h2_dois as d
import cena_telas as cs
import comparar_tela as ct
import comparar_lote as cl

ESCALA = 3

# Com dois clientes e o servidor parado o `clear @s` do anfitrião chega depois do `give` seguinte (o cliente refaz o inventário no pacote do `clear`): uma folga depois dele.
_comando = cs.comando


def _comando_com_folga(texto):
    saida = _comando(texto)
    if texto.startswith("clear"):
        time.sleep(0.8)
    return saida


cs.comando = _comando_com_folga
CASOS = ["chamber_vazia", "chamber_operacao", "chamber_aviso_energia", "chamber_aviso_saida", "chamber_hover_progresso", "chamber_hover_barra", "chamber_xp_com",
         "chamber_aba_energia_hover_ativa", "infuser_vazia", "infuser_operacao", "infuser_aviso_infusao", "infuser_tanque_bio", "infuser_hover_barra_quimico",
         "generator_vazia", "generator_lava_meio", "generator_hover_barra", "generator_queima", "crusher_operacao", "smelter_operacao"]


def pull(dest_remote, dest_local):
    subprocess.run(["scp", "-q", f"desktop:{dest_remote}", dest_local], check=True)


def aberta(who, x, tries=4):
    for _ in range(tries):
        who("interact", str(x), "-60", "3")
        time.sleep(0.8)
        if '"screen":"container"' in who("screen").replace(" ", ""):
            return
    raise SystemExit("o menu não abriu")


def cursor(who, case, esquerda, topo):
    if "hover" in case["args"]:
        hx, hy = case["args"]["hover"]
        who("scroll-at", str(esquerda + hx), str(topo + hy), "0")
    else:
        who("scroll-at", "2", "2", "0")


def snap(who, remote):
    who("screenshot", remote)


def compare(local_a, local_b, case, extra=None):
    tela = cs.MAQUINAS[case["maquina"]]["tela"]
    a, b = ct.ler_png(local_a), ct.ler_png(local_b)
    try:
        res, _ = ct.comparar(a, b, tela, ESCALA, margem=40, ausentes=cl.ausentes_do_caso(case), mascarar_abas=False,
                             limite_mundo=cl.LIMITE_MUNDO_POR_LUZ[cs.luz_do_caso(case)])
    except Exception as e:  # noqa: BLE001
        return {"erro": str(e)}
    return res


def main():
    args = sys.argv[1:]
    dest = args[0]
    so = None
    por_tick = 0
    if "--so" in args:
        so = args[args.index("--so") + 1].split(",")
    if "--passos-a-passo" in args:
        por_tick = int(args[args.index("--passos-a-passo") + 1])
    os.makedirs(dest, exist_ok=True)
    remote = "/home/luka/.local/share/jogo-isolado/horizon/data/shots/dc_tela"
    subprocess.run(["ssh", "desktop", f"mkdir -p {remote}"], check=True)
    # As mesmas opções de tela nos dois clientes.
    for who in (d.host, d.guest):
        who("options-set", "guiScale", str(ESCALA))
        who("options-set", "chatVisibility", "2")
    d.host_command("clear @s")
    gui_l, gui_a = -(-1920 // ESCALA), -(-1080 // ESCALA)
    esquerda, topo = (gui_l - 176) // 2, (gui_a - 166) // 2
    todos = {c["nome"]: c for c in cs.todos_os_casos()}
    falhas = 0
    for nome in (so or CASOS):
        case = todos.get(nome)
        if case is None:
            print("sem o caso", nome)
            continue
        x = 3
        cs.monta_port(case, x, ESCALA)
        # O convidado onde o jogador do `CenaTelas` fica (x, -60, 0), de frente para a máquina, para o mundo atrás da tela ser o do original; o anfitrião 1 bloco atrás dele.
        d.host_command(f"tp {d.GUEST_NAME} {x}.0 -60.0 0.0 0 0")
        d.host_command(f"tp Player90366 {x}.0 -60.0 -1.0 0 0")
        d.host("look", "0", "0")
        time.sleep(1.0)
        d.guest("close")
        aberta(d.host, x)
        aberta(d.guest, x)
        d.host("look", "0", "0")
        d.guest("look", "0", "0")
        cursor(d.host, case, esquerda, topo)
        cursor(d.guest, case, esquerda, topo)
        time.sleep(1.0)
        ticks = [0] + list(range(1, por_tick + 1))
        for k in ticks:
            if k:
                d.host_command("tick step 1")
                time.sleep(0.5)
            tag = f"{nome}" + (f"_t{k:03d}" if por_tick else "")
            snap(d.host, f"{remote}/h_{tag}.png")
            snap(d.guest, f"{remote}/g_{tag}.png")
            time.sleep(0.4)
            pull(f"{remote}/h_{tag}.png", f"{dest}/h_{tag}.png")
            pull(f"{remote}/g_{tag}.png", f"{dest}/g_{tag}.png")
            res = compare(f"{dest}/h_{tag}.png", f"{dest}/g_{tag}.png", case)
            detalhe = {n: e["diferentes"] for n, e in res.get("elementos", {}).items() if e["diferentes"]}
            # O mundo atrás da tela é outro (o convidado está noutro ponto da caixa): o que sai do mundo e dos cantos transparentes do painel não é do mod.
            reais = {n: v for n, v in detalhe.items() if "mundo" not in n and "canto" not in n}
            bad = sum(reais.values()) if "erro" not in res else -1
            if bad != 0:
                falhas += 1
            print(f"{'OK  ' if bad == 0 else 'DIFF'} {tag}: {bad} pixels diferentes na tela e nas abas ({res.get('comparados')} comparados; o mundo atrás: {sum(v for n, v in detalhe.items() if n not in reais)}) {reais or ''} {res.get('erro', '')}", flush=True)
        d.host("close")
        d.guest("close")
    d.host_command("tick unfreeze")
    for who in (d.host, d.guest):
        who("options-set", "chatVisibility", "0")
    print(f"{falhas} prints com diferença")
    sys.exit(1 if falhas else 0)


if __name__ == "__main__":
    main()
