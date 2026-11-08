#!/usr/bin/env python3
"""Compara, pixel a pixel, as telas do convidado de LAN no original (`oficial_dois.sh`: `convidado_oficial_dois_<nome>.png`) e no port (`horizon2/h2_dois_roteiro.py`:
`port_dois_<nome>.png`), nos mesmos estados do roteiro `cenas/dois_tela.txt`. O convidado fica no mesmo ponto nos dois jogos, de frente para a máquina, dentro da mesma caixa de
concreto preto, então o mundo atrás da tela é o do `cena_telas.py` e vale a regra dele (`comparar_lote`): zero pixel nos elementos do mod; o que sai do mundo (H22) e dos cantos
transparentes do painel não é do mod. A fase da animação do tanque e da lava (`barra_quimico`, `medidor_lava`) depende do relógio de cada cliente: se é só ela que difere e o nível do
enchimento é o mesmo (a primeira linha com diferença está no topo do líquido), conta como fase.

Uso: comparar_dois.py PASTA_DO_ORIGINAL PASTA_DO_PORT [escala]"""
import os, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cena_telas as cs
import comparar_lote as cl
import comparar_tela as ct

ANIMADO = {"infuser": "barra_quimico", "generator": "medidor_lava"}


def caso_de(nome):
    base = nome.split("_t")[0] if "_t" in nome and nome.rsplit("_t", 1)[1].isdigit() else nome
    for c in cs.todos_os_casos():
        if c["nome"] == base:
            return c
    return None


def main():
    pasta_oficial, pasta_port = sys.argv[1], sys.argv[2]
    escala = int(sys.argv[3]) if len(sys.argv) > 3 else 3
    ruim = 0
    for arquivo in sorted(os.listdir(pasta_oficial)):
        if not (arquivo.startswith("convidado_oficial_dois_") and arquivo.endswith(".png")):
            continue
        nome = arquivo[len("convidado_oficial_dois_"):-4]
        caminho_port = os.path.join(pasta_port, f"port_dois_{nome}.png")
        if not os.path.exists(caminho_port):
            print(f"FALTA {nome} no port")
            continue
        c = caso_de(nome)
        if c is None:
            continue
        tela = cs.MAQUINAS[c["maquina"]]["tela"]
        a, b = ct.ler_png(os.path.join(pasta_oficial, arquivo)), ct.ler_png(caminho_port)
        res, _ = ct.comparar(a, b, tela, escala, margem=28, ausentes=cl.ausentes_do_caso(c), mascarar_abas=True,
                             limite_mundo=cl.LIMITE_MUNDO_POR_LUZ[cs.luz_do_caso(c)])
        dados = {n: e["diferentes"] for n, e in res["elementos"].items() if e["diferentes"]}
        # `aba_avisos` é o retângulo onde a aba ficaria: sem erro ela não existe nos dois jogos e o que sobra é o mundo atrás, com no máximo 1 nível por canal (o limite do mundo apagado).
        mundo = {n for n, e in res["elementos"].items() if n == "aba_avisos" and e["maior"] <= 1}
        reais = {n: v for n, v in dados.items() if "mundo" not in n and "canto" not in n and n not in mundo}
        fase = {n: v for n, v in reais.items() if n == ANIMADO.get(tela)}
        outros = {n: v for n, v in reais.items() if n not in fase}
        estado = "OK  " if not outros and not fase else ("FASE" if not outros else "DIFF")
        ruim += bool(outros)
        print(f"{estado} {nome}: elementos do mod {outros or 0}; animação (fase) {fase or 0}; mundo/cantos {sum(v for n, v in dados.items() if n not in reais)}")
    print(f"{ruim} prints com diferença nos elementos do mod")
    sys.exit(1 if ruim else 0)


if __name__ == "__main__":
    main()
