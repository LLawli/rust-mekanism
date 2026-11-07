#!/usr/bin/env python3
"""Compara todos os estados de `cena_telas.py` entre o original e o port.

    comparar_lote.py PASTA_ORIGINAL PASTA_PORT SAIDA ESCALA... [--so NOME,NOME]

`PASTA_ORIGINAL` tem os `oficial_<caso>_e<escala>[_<n>].png` do `CenaTelas`, `PASTA_PORT` os `port_<caso>_e<escala>[_<n>].png` do
`cena_telas.py port`. Grava em `SAIDA` um `tela_<caso>[_e<escala>].png` por estado (o original, o port e a diferença lado a lado: a
escala 2 não leva sufixo) e um `resultado.json`, e imprime os pixels diferentes por elemento. Sem tolerância e sem máscara além da
das abas, que o original da cena já traz ocultas. Código de saída 1 se algum estado tem diferença.
"""
import json
import os
import sys

import cena_telas
import comparar_tela as ct

def caixa_do_tooltip(imagem, escala, hover):
    """O retângulo do tooltip numa imagem, em pixels de GUI relativos à tela, ou `None` se não há. O fundo e a borda do tooltip são
    roxos (`0xF0100010` e o degradê de `0x505000FF`), o que nada mais na cena é; procura perto do cursor."""
    largura, altura, rgba = imagem
    ox, oy = ct.origem_da_tela(largura, altura, escala)
    hx, hy = hover
    x0, y0 = max(0, ox + (hx - 20) * escala), max(0, oy + (hy - 60) * escala)
    x1, y1 = min(largura, ox + (hx + 300) * escala), min(altura, oy + (hy + 80) * escala)
    achados = []
    for y in range(y0, y1):
        base = y * largura * 4
        for x in range(x0, x1):
            r, g, b = rgba[base + x * 4 : base + x * 4 + 3]
            if b >= g + 8 and r >= g + 8 and r < 140:
                achados.append((x, y))
    if not achados:
        return None
    xs = [p[0] for p in achados]
    ys = [p[1] for p in achados]
    esquerda, topo = (min(xs) - ox) // escala, (min(ys) - oy) // escala
    direita, base = -(-(max(xs) + 1 - ox) // escala), -(-(max(ys) + 1 - oy) // escala)
    return (esquerda, topo, direita - esquerda, base - topo)


def uniao(a, b):
    if a is None or b is None:
        return a or b
    x = min(a[0], b[0])
    y = min(a[1], b[1])
    return (x, y, max(a[0] + a[2], b[0] + b[2]) - x, max(a[1] + a[3], b[1] + b[3]) - y)


def pares(casos, escala, original, port):
    for c in casos:
        capturas = int(c["args"].get("ciclo", "0")) or int(c["args"].get("capturas", "1"))
        for k in range(capturas):
            sufixo = f"_{k}" if capturas > 1 else ""
            nome = f"{c['nome']}_e{escala}{sufixo}"
            yield c, k, nome, os.path.join(original, f"oficial_{nome}.png"), os.path.join(port, f"port_{nome}.png")


def recorte(imagem, escala, rect):
    """As linhas de pixels de um retângulo de GUI relativo à tela."""
    largura, altura, rgba = imagem
    ox, oy = ct.origem_da_tela(largura, altura, escala)
    x, y, w, h = rect
    return [bytes(rgba[((oy + (y * escala) + j) * largura + ox + x * escala) * 4 :][: w * escala * 4]) for j in range(h * escala)]


def diferentes(a, b):
    return sum(1 for ra, rb in zip(a, b) for i in range(0, len(ra), 4) if ra[i : i + 3] != rb[i : i + 3])


def casar_fases(c, escala, original, port):
    """Um ciclo de uma animação do atlas: o original e o port tiram `N` prints, um por passo do servidor, mas cada um conta o tempo da
    animação do seu jeito (o original desde o carregamento das texturas, e anda um quadro a cada dois passos), então as fases não
    coincidem print a print. O port, em `N` passos, passa por todos os estados do ciclo; cada print do original deve ser igual a algum
    deles. Devolve, para cada print do original, o print do port com menos pixels diferentes no elemento animado."""
    n = cena_telas.ciclos(c, "oficial")
    m = cena_telas.ciclos(c, "port")
    tela = cena_telas.MAQUINAS[c["maquina"]]["tela"]
    rect = ct.elementos_da_tela(tela)["barra_quimico" if tela == "infuser" else "medidor_lava"]
    arquivos = lambda lado, prefixo, total: [os.path.join(lado, f"{prefixo}_{c['nome']}_e{escala}_{k}.png") for k in range(total)]
    ori = [recorte(ct.ler_png(p), escala, rect) for p in arquivos(original, "oficial", n)]
    prt = [recorte(ct.ler_png(p), escala, rect) for p in arquivos(port, "port", m)]
    pares_ = []
    for k in range(n):
        custo, j = min((diferentes(ori[k], prt[j]), j) for j in range(m))
        pares_.append((custo, j))
    return pares_, arquivos(original, "oficial", n), arquivos(port, "port", m)


def regioes_do_caso(c, a, b, escala):
    """O tooltip do caso: a união das caixas que ele ocupa no original e no port, para que um tooltip que só um dos dois desenha
    também apareça como diferença."""
    if "hover" not in c["args"]:
        return {}
    caixa = uniao(caixa_do_tooltip(a, escala, c["args"]["hover"]), caixa_do_tooltip(b, escala, c["args"]["hover"]))
    return {"tooltip": caixa} if caixa else {}


# O que o Horizon desenha diferente do original e já está pedido a ele (docs/injecoes-horizon.md): o mundo atrás da tela (H22), os cantos
# transparentes do painel e das abas, a sombra da janela sobre esse mundo e os ícones de item em escala (H25).
RESIDUOS_DO_HORIZON = ("mundo_atras", "janela_fundo", "janela_aceitos")


def ausentes_do_caso(c):
    """Os elementos que o estado do caso não desenha: a aba de xp sem o upgrade, e (nos casos de xp, que não têm erro nenhum) a de avisos."""
    ausentes = [] if cena_telas.xp_visivel(c) else ["aba_xp"]
    if "_xp_" in c["nome"] or "_nome_" in c["nome"] or "_aba_energia" in c["nome"]:
        ausentes.append("aba_avisos")
    return ausentes


def residuo(nome):
    return nome.startswith("canto") or nome in RESIDUOS_DO_HORIZON


def principal(argv):
    so = None
    if "--so" in argv:
        i = argv.index("--so")
        so = argv[i + 1].split(",")
        del argv[i : i + 2]
    original, port, saida = argv[:3]
    escalas = [int(e) for e in argv[3:]]
    os.makedirs(saida, exist_ok=True)
    casos = [c for c in cena_telas.todos_os_casos() if not so or c["nome"] in so]
    resultados = []
    falhas = 0
    for escala in escalas:
        for c in casos:
            if "ciclo" not in c["args"]:
                continue
            try:
                casados, pa, pb = casar_fases(c, escala, original, port)
            except FileNotFoundError:
                print(f"FALTA {c['nome']}_e{escala} (ciclo)")
                falhas += 1
                continue
            n = cena_telas.ciclos(c, "oficial")
            distintos = len({tuple(x) for x in casados})
            sem_par = sum(1 for custo, _ in casados if custo)
            print(f"ciclo {c['nome']}_e{escala}: {len(casados) - sem_par} de {len(casados)} prints do original têm um estado idêntico no port; {sem_par} sem par (pior {max(custo for custo, _ in casados)} px)")
            tela = cena_telas.MAQUINAS[c["maquina"]]["tela"]
            for k in range(n):
                a, b = ct.ler_png(pa[k]), ct.ler_png(pb[casados[k][1]])
                resultado, diferenca = ct.comparar(a, b, tela, escala, mascarar_abas=False)
                resultado["caso"] = f"{c['nome']}_e{escala}_{k}"
                resultados.append(resultado)
                ruim = resultado["diferentes"] > 0
                falhas += ruim
                por_elemento = {n_: d_["diferentes"] for n_, d_ in resultado["elementos"].items() if d_["diferentes"] and not n_.startswith("canto")}
                if k == 0 or ruim:
                    sufixo = "" if escala == 2 else f"_e{escala}"
                    ct.gravar_png(os.path.join(saida, f"tela_{c['nome']}_{k}{sufixo}.png"), *ct.lado_a_lado(a, b, diferenca, resultado["recorte"]))
                print(f"  {'DIF' if ruim else 'ok '} {resultado['caso']:<44} {resultado['diferentes']:>6} {por_elemento if por_elemento else ''}", flush=True)
        for c, k, nome, caminho_a, caminho_b in pares([x for x in casos if "ciclo" not in x["args"]], escala, original, port):
            if not (os.path.exists(caminho_a) and os.path.exists(caminho_b)):
                print(f"FALTA {nome}")
                falhas += 1
                continue
            tela = cena_telas.MAQUINAS[c["maquina"]]["tela"]
            a, b = ct.ler_png(caminho_a), ct.ler_png(caminho_b)
            # Com a janela aberta, os filhos dela têm prioridade e o recorte inclui a janela e a aba.
            janela = ct.JANELA if "clicar" in c["args"] and "%d,%d" % cena_telas.ABA in c["args"]["clicar"].split(";") else None
            resultado, diferenca = ct.comparar(
                a, b, tela, escala, regioes=regioes_do_caso(c, a, b, escala), mascarar_abas=False,
                margem=28 if "_aba_" in c["nome"] or "_xp_" in c["nome"] else 0, prioridade=janela,
                ausentes=ausentes_do_caso(c),
            )
            resultado["caso"] = nome
            resultados.append(resultado)
            ruim = resultado["diferentes"] > 0
            falhas += ruim
            sufixo = "" if escala == 2 else f"_e{escala}"
            extra = f"_{k}" if int(c["args"].get("ciclo", "0")) or int(c["args"].get("capturas", "1")) > 1 else ""
            destino = os.path.join(saida, f"tela_{c['nome']}{extra}{sufixo}.png")
            ct.gravar_png(destino, *ct.lado_a_lado(a, b, diferenca, resultado["recorte"]))
            por_elemento = {n: d["diferentes"] for n, d in resultado["elementos"].items() if d["diferentes"]}
            reais = {n: d for n, d in por_elemento.items() if not residuo(n)}
            print(f"{'DIF' if ruim else 'ok '} {nome:<44} {resultado['diferentes']:>6} fora dos resíduos do Horizon: {reais if reais else 0}", flush=True)
    with open(os.path.join(saida, "resultado.json"), "w") as arquivo:
        json.dump(resultados, arquivo, indent=1, ensure_ascii=False)
    total = sum(r["diferentes"] for r in resultados)
    print(f"{len(resultados)} estados, {falhas} com diferença ou faltando, {total} pixels diferentes")
    return 1 if falhas else 0


if __name__ == "__main__":
    sys.exit(principal(sys.argv[1:]))
