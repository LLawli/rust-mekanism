#!/usr/bin/env python3
"""Os estados de tela que o original e o port mostram lado a lado, e o roteiro do port. A tabela está aqui. Duas saídas:

    cena_telas.py oficial ARQUIVO ESCALA... [--so NOME,NOME] escreve o arquivo `cena-telas` que `testes/CenaTelas.java` lê (um por escala)
    cena_telas.py port DESTINO ESCALA... [--so NOME,NOME]
                                      monta cada estado no Horizon pelo `horizon-ctl` e grava `port_<caso>_e<escala>.png` em DESTINO

O port roda no container `horizon` do desktop (`docker exec horizon`), com o mundo superplano aberto e o servidor parado (`tick
freeze`) enquanto o estado é montado, como o `CenaTelas`. Os nomes dos casos são os mesmos nos dois lados.
"""
import json
import shlex
import subprocess
import sys

# A origem de cada tela e onde pôr o cursor, em pixels de GUI relativos ao canto da tela (um ponto dentro do elemento).
HOVER = {
    "electric": {
        "slot_entrada": (72, 25),
        "slot_saida": (124, 43),
        "slot_energia": (72, 61),
        "barra_energia": (167, 40),
        "progresso": (98, 42),
    },
    "infuser": {
        "slot_quimico": (25, 43),
        "slot_entrada": (59, 51),
        "slot_saida": (117, 51),
        "slot_energia": (151, 43),
        "barra_energia": (167, 40),
        "barra_quimico": (10, 40),
        "dump": (26, 64),
        "progresso": (88, 51),
    },
    "generator": {
        "slot_combustivel": (25, 43),
        "slot_energia": (151, 43),
        "medidor_lava": (88, 43),
        "barra_energia": (167, 40),
    },
}

# Por máquina: o bloco, a tela (a tabela do comparador), a capacidade de energia de uma máquina colocada por um jogador, que
# `applyImplicitComponents` recalcula (`MachineEnergyContainer.updateMaxEnergy`: consumo por tick vezes `ticksRequired`, 200),
# a entrada que tem receita, e o slot de entrada e de saída na ordem do menu.
MAQUINAS = {
    "chamber": dict(block="mekanism:enrichment_chamber", tela="electric", cap=1000, entrada="coal", slots=(0, 1)),
    "crusher": dict(block="mekanism:crusher", tela="electric", cap=1600, entrada="blaze_rod", slots=(0, 1)),
    "smelter": dict(block="mekanism:energized_smelter", tela="electric", cap=1000, entrada="raw_iron", slots=(0, 1)),
    "infuser": dict(block="mekanism:metallurgic_infuser", tela="infuser", cap=1000, entrada="clay_ball", slots=(1, 2)),
    "generator": dict(block="mekanismgenerators:heat_generator", tela="generator", cap=240, entrada=None, slots=(0, 1)),
}

# O que sai do slot de saída "cheio" e o item que não é o da receita.
SAIDA_CHEIA = {"chamber": "clay_ball", "crusher": "gravel", "smelter": "glass", "infuser": "dirt"}


def caso(nome, maquina, **args):
    return dict(nome=nome, maquina=maquina, args=args)


def casos():
    out = []
    for m in ("chamber", "crusher", "smelter"):
        spec = MAQUINAS[m]
        entrada = f"{spec['entrada']}:2"
        out += [
            caso(f"{m}_vazia", m, energia="0"),
            caso(f"{m}_operacao", m, energia="cap", entrada=entrada, passos="100"),
            caso(f"{m}_aviso_energia", m, energia="0", entrada=entrada, passos="5"),
        ]
    # A Enrichment Chamber tem o conjunto inteiro.
    out += [
        caso("chamber_cheia", "chamber", energia="cap"),
        caso("chamber_aviso_saida", "chamber", energia="cap", entrada="coal:2", saida="mekanism:enriched_carbon:64", passos="5"),
        caso("chamber_hover_slot_vazio", "chamber", energia="0", hover=HOVER["electric"]["slot_saida"]),
        caso("chamber_hover_slot_item", "chamber", energia="0", entrada="coal:2", hover=HOVER["electric"]["slot_entrada"]),
        caso("chamber_hover_slot_energia", "chamber", energia="0", hover=HOVER["electric"]["slot_energia"]),
        caso("chamber_hover_barra", "chamber", energia="500", hover=HOVER["electric"]["barra_energia"]),
        caso("chamber_hover_progresso", "chamber", energia="cap", entrada="coal:2", passos="100",
             hover=HOVER["electric"]["progresso"]),
    ]
    out += [
        caso("infuser_vazia", "infuser", energia="0"),
        caso("infuser_operacao", "infuser", energia="cap", entrada="clay_ball:2", quimico="bio:800", passos="100"),
        caso("infuser_cheia", "infuser", energia="cap", quimico="bio:1000"),
        caso("infuser_aviso_energia", "infuser", energia="0", entrada="clay_ball:2", quimico="bio:500", passos="5"),
        caso("infuser_aviso_infusao", "infuser", energia="cap", entrada="clay_ball:2", passos="5"),
        caso("infuser_tanque_bio", "infuser", energia="0", quimico="bio:500"),
        # `ciclo` prints no original, e `ciclo_port` no port: o ciclo inteiro da animação (quadros vezes `frametime`), porque o original
        # anda um passo de animação a cada dois passos do servidor e o port um a cada passo, e o print do original tem de achar o seu
        # estado entre os do port. `base`: 10 quadros de 4 ticks; `bio`: 8 de 30; `fungi`: 4 de 40.
        caso("infuser_tanque_carbono", "infuser", energia="0", quimico="carbon:500", ciclo="40", ciclo_port="40"),
        caso("infuser_tanque_bio_ciclo", "infuser", energia="0", quimico="bio:500", ciclo="40", ciclo_port="240"),
        caso("infuser_tanque_fungi_ciclo", "infuser", energia="0", quimico="fungi:500", ciclo="40", ciclo_port="160"),
        caso("infuser_hover_barra_energia", "infuser", energia="500", hover=HOVER["infuser"]["barra_energia"]),
        caso("infuser_hover_barra_quimico", "infuser", energia="0", quimico="carbon:500",
             hover=HOVER["infuser"]["barra_quimico"]),
        caso("infuser_hover_barra_quimico_vazia", "infuser", energia="0", hover=HOVER["infuser"]["barra_quimico"]),
        caso("infuser_hover_dump", "infuser", energia="0", quimico="carbon:500", hover=HOVER["infuser"]["dump"]),
        caso("infuser_hover_slot_quimico", "infuser", energia="0", hover=HOVER["infuser"]["slot_quimico"]),
    ]
    out += [
        caso("generator_vazia", "generator", energia="0"),
        caso("generator_lava_meio", "generator", energia="120", lava="500"),
        caso("generator_lava_cheia", "generator", energia="120", lava="1000"),
        # Com a energia cheia o gerador não queima a lava: com 120 ela baixava a cada passo e o medidor mudava de nível em vez de animar.
        # O `lava_still` tem 38 quadros de 2 ticks, sem interpolação.
        caso("generator_lava_ciclo", "generator", energia="cap", lava="500", ciclo="40", ciclo_port="76"),
        caso("generator_hover_barra", "generator", energia="120", lava="500", hover=HOVER["generator"]["barra_energia"]),
        caso("generator_hover_medidor", "generator", energia="120", lava="500", hover=HOVER["generator"]["medidor_lava"]),
        caso("generator_hover_medidor_vazio", "generator", energia="0", hover=HOVER["generator"]["medidor_lava"]),
        caso("generator_hover_slot", "generator", energia="0", hover=HOVER["generator"]["slot_combustivel"]),
        # Queima a lava com a energia subindo: a barra e o `MEKTRACE` do port, contra o servidor do original, passo a passo.
        caso("generator_queima", "generator", energia="0", lava="500", passos="100"),
    ]
    return out


# A janela de upgrades (0,0 = canto da tela). A janela abre em (-11, 15): a aba é clicada em (190, 19), uma linha da lista em (29, 40), o
# botão Uninstall em (132, 75) e o rótulo "Supported:" em (-2, 85).
ABA = (190, 19)
LINHA = (29, 40)
UNINSTALL = (132, 75)


def casos_aba():
    out = []
    todos = "speed:3,energy:2,muffling:1,xp_containment:1"
    for m in ("chamber", "crusher", "smelter", "infuser"):
        out += [
            caso(f"{m}_aba_fechada", m, energia="0"),
            caso(f"{m}_aba_vazia", m, energia="0", clicar=f"{ABA[0]},{ABA[1]}"),
            caso(f"{m}_aba_upgrades", m, energia="0", upgrades=todos, clicar=f"{ABA[0]},{ABA[1]}"),
        ]
    out += [
        caso("chamber_aba_hover", "chamber", energia="0", hover=ABA),
        caso("chamber_aba_selecionado", "chamber", energia="0", upgrades=todos,
             clicar=f"{ABA[0]},{ABA[1]};{LINHA[0]},{LINHA[1]}"),
        caso("chamber_aba_botao_hover", "chamber", energia="0", upgrades=todos,
             clicar=f"{ABA[0]},{ABA[1]};{LINHA[0]},{LINHA[1]}", hover=UNINSTALL),
        caso("chamber_aba_instalando", "chamber", energia="0", upgradeentrada="speed:5", passos="10",
             clicar=f"{ABA[0]},{ABA[1]}"),
        caso("chamber_aba_instalado", "chamber", energia="0", upgradeentrada="speed:5", passos="30",
             clicar=f"{ABA[0]},{ABA[1]}"),
        caso("chamber_aba_saida", "chamber", energia="0", upgrades="speed:3", upgradesaida="speed:7",
             clicar=f"{ABA[0]},{ABA[1]}"),
        caso("chamber_aba_hover_lista", "chamber", energia="0", upgrades=todos,
             clicar=f"{ABA[0]},{ABA[1]}", hover=LINHA),
        caso("chamber_aba_hover_aceitos", "chamber", energia="0", upgrades="speed:1",
             clicar=f"{ABA[0]},{ABA[1]}", hover=(-2, 95)),
        # A ordem da lista do original é a de um `Object2IntOpenHashMap` de chaves por identidade (muda a cada partida): com mais de um
        # tipo o print do original só se compara por linha. Com um tipo só a lista, a seleção e o texto têm ordem única.
        caso("chamber_aba_um_upgrades", "chamber", energia="0", upgrades="speed:3", clicar=f"{ABA[0]},{ABA[1]}"),
        caso("chamber_aba_um_selecionado", "chamber", energia="0", upgrades="speed:3",
             clicar=f"{ABA[0]},{ABA[1]};{LINHA[0]},{LINHA[1]}"),
        caso("chamber_aba_um_botao_hover", "chamber", energia="0", upgrades="speed:3",
             clicar=f"{ABA[0]},{ABA[1]};{LINHA[0]},{LINHA[1]}", hover=UNINSTALL),
        caso("chamber_aba_um_hover_lista", "chamber", energia="0", upgrades="speed:3",
             clicar=f"{ABA[0]},{ABA[1]}", hover=LINHA),
        # A regra do clique: com a janela aberta, o Dump (26, 64) fica atrás dela e o clique é consumido (o químico fica). Com a janela
        # fechada pelo X (-1, 25), o mesmo clique chega ao Dump e esvazia o tanque.
        caso("infuser_aba_clique_dentro", "infuser", energia="0", quimico="bio:500",
             clicar=f"{ABA[0]},{ABA[1]};26,64"),
        caso("infuser_aba_clique_fechada", "infuser", energia="0", quimico="bio:500",
             clicar=f"{ABA[0]},{ABA[1]};-1,25;26,64"),
    ]
    return out


def ciclos(c, lado):
    """Quantos prints de ciclo o `lado` (`oficial` ou `port`) tira em `c`: `ciclo`, ou `ciclo_port` no port quando o caso o pede."""
    ciclo = int(c["args"].get("ciclo", "0"))
    if lado == "port":
        return int(c["args"].get("ciclo_port", ciclo))
    return ciclo


def escreve_oficial(escalas, saida, so=None):
    linhas = []
    for escala in escalas:
        linhas.append(f"escala {escala}")
        for c in casos() + casos_aba():
            if so and c["nome"] not in so:
                continue
            args = dict(c["args"])
            if "hover" in args:
                args["hover"] = "%d,%d" % tuple(args["hover"])
            pares = " ".join(f"{k}={v}" for k, v in args.items())
            linhas.append(f"caso {c['nome']} {c['maquina']} {pares}".rstrip())
    with open(saida, "w") as arquivo:
        arquivo.write("\n".join(linhas) + "\n")


# O Horizon ----------------------------------------------------------------------------------------------------------------------

CTL = (
    "docker exec horizon sh -c 'l=$XDG_DATA_HOME/horizon-launcher; "
    "v=$(sed -n \"s/.*\\\"version\\\": *\\\"\\([^\\\"]*\\)\\\".*/\\1/p\" $l/instances.json | head -n 1); "
    'exec $l/versions/$v/horizon-ctl --timeout 60 "$@"\' sh'
)


def ctl(*args, check=True):
    comando = "ssh desktop " + shlex.quote(CTL + " " + " ".join(shlex.quote(a) for a in args))
    resultado = subprocess.run(comando, shell=True, capture_output=True, text=True)
    if check and resultado.returncode != 0:
        raise RuntimeError(f"{args}: {resultado.stdout} {resultado.stderr}")
    return resultado.stdout


def comando(texto):
    return ctl("command", texto)


def nbt_itens(c, maquina):
    itens = []
    entrada, saida = MAQUINAS[maquina]["slots"]
    for chave, slot in (("entrada", entrada), ("saida", saida)):
        if chave in c["args"]:
            item, n = c["args"][chave].rsplit(":", 1)
            if ":" not in item:
                item = "minecraft:" + item
            itens.append(f'{{Slot:{slot}b,id:"{item}",count:{n}}}')
    return itens


def item_upgrade(texto):
    tipo, n = texto.split(":")
    return f'{{type:{{id:"mekanism:upgrade",components:{{"mekanism:upgrade_type":"mekanism:{tipo}"}}}},amount:{n}L}}'


def componente_upgrade(c):
    a = c["args"]
    partes, itens = [], []
    if "upgrades" in a:
        mapa = ",".join(f'"mekanism:{t}":{n}' for t, n in (p.split(":") for p in a["upgrades"].split(",")))
        partes.append(f"upgrades:{{{mapa}}}")
    if "upgradeentrada" in a:
        itens.append(f"{{container:0b,stored:{item_upgrade(a['upgradeentrada'])}}}")
    if "upgradesaida" in a:
        itens.append(f"{{container:1b,stored:{item_upgrade(a['upgradesaida'])}}}")
    if itens:
        partes.append("items:[" + ",".join(itens) + "]")
    return "{" + ",".join(partes) + "}" if partes else None


def monta_port(c, x, escala):
    m = c["maquina"]
    spec = MAQUINAS[m]
    pos = f"{x} -60 3"
    comando("tick freeze")
    # O mundo atrás da tela, igual ao do `CenaTelas`: uma caixa oca de concreto preto.
    comando(f"fill {x - 6} -61 -6 {x + 6} -50 10 minecraft:air")
    comando(f"fill {x - 6} -61 -6 {x + 6} -50 10 minecraft:black_concrete hollow")
    comando(f"setblock {pos} minecraft:air")
    comando(f"setblock {pos} {spec['block']}")
    # Os upgrades entram antes do primeiro tick: o objeto vivo os lê do NBT ao nascer, como o `deserialize`.
    comp = componente_upgrade(c)
    if comp:
        comando(f"data merge block {pos} {{component_upgrade:{comp}}}")
    # Um tick para o bloco nascer e a capacidade valer, como no `CenaTelas`.
    comando("tick step 1")
    ctl("wait", "4")
    nbt = []
    energia = c["args"].get("energia")
    if energia is not None:
        nbt.append("energy:%d" % (spec["cap"] if energia == "cap" else int(energia)))
    itens = nbt_itens(c, m)
    if itens:
        nbt.append("Items:[" + ",".join(itens) + "]")
    if "quimico" in c["args"]:
        tipo, n = c["args"]["quimico"].split(":")
        nbt.append(f'chemical_tanks:[{{container:0b,stored:{{type:"mekanism:{tipo}",amount:{n}L}}}}]')
    if "lava" in c["args"]:
        nbt.append(f'machinery_basis:{{v:1,tanks:[{{resource:"minecraft:lava",amount:{c["args"]["lava"]}L}}]}}')
    if nbt:
        comando(f"data merge block {pos} {{{','.join(nbt)}}}")

    ctl("tp", f"{x}.0", "-60.0", "0.0")
    passos = int(c["args"].get("passos", "2"))
    comando(f"tick step {passos}")
    ctl("wait", str(passos + 4))
    return x


def tira_port(c, x, escala, destino):
    ctl("look", "0", "0")
    ctl("interact", str(x), "-60", "3")
    ctl("wait", "60")
    # `centered_origin`: a janela em pixels de GUI arredondada para cima, e a tela truncada no centro.
    gui_l = -(-1920 // escala)
    gui_a = -(-1080 // escala)
    esquerda = (gui_l - 176) // 2
    topo = (gui_a - 166) // 2
    for clique in filter(None, c["args"].get("clicar", "").split(";")):
        cx, cy = (int(v) for v in clique.split(","))
        ctl("click-at", str(esquerda + cx), str(topo + cy))
        ctl("wait", "6")
    if "hover" in c["args"]:
        # O Horizon dá o cursor em pixels de GUI da janela: o canto da tela mais o ponto dentro dela.
        hx, hy = c["args"]["hover"]
        ctl("scroll-at", str(esquerda + hx), str(topo + hy), "0")
    else:
        ctl("scroll-at", "2", "2", "0")
    ctl("wait", "10")
    ciclo = ciclos(c, "port")
    capturas = ciclo or int(c["args"].get("capturas", "1"))
    for k in range(capturas):
        sufixo = f"_{k}" if capturas > 1 else ""
        if ciclo:
            # Um passo do servidor por quadro, como o `CenaTelas`: a fase de cada print é conhecida.
            comando("tick step 1")
            ctl("wait", "2")
        ctl("screenshot", f"{destino}/port_{c['nome']}_e{escala}{sufixo}.png")
        if capturas > 1 and not ciclo:
            ctl("wait", "3")
    ctl("close")


def roda_port(destino, escalas, so=None):
    # O mesmo inventário vazio do `CenaTelas`.
    comando("clear @s")
    ctl("options-set", "gamma", "0.5")
    for escala in escalas:
        ctl("options-set", "guiScale", str(escala))
        x = 3
        for c in casos() + casos_aba():
            if so and c["nome"] not in so:
                continue
            monta_port(c, x, escala)
            tira_port(c, x, escala, destino)
            print(c["nome"], escala, flush=True)
    comando("tick unfreeze")


if __name__ == "__main__":
    modo = sys.argv[1]
    if modo == "oficial":
        args = sys.argv[2:]
        so = None
        if "--so" in args:
            i = args.index("--so")
            so = args[i + 1].split(",")
            del args[i : i + 2]
        escreve_oficial([int(e) for e in args[1:]], args[0], so)
    elif modo == "port":
        args = sys.argv[2:]
        so = None
        if "--so" in args:
            i = args.index("--so")
            so = args[i + 1].split(",")
            del args[i : i + 2]
        roda_port(args[0], [int(e) for e in args[1:]], so)
    else:
        raise SystemExit(__doc__)
