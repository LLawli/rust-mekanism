#!/usr/bin/env python3
"""Confere o fundo escurecido das telas de contêiner no Horizon (injeção `container_dim`, H22): a tela de máquina tem o degradê do original (`0xC0101010` a
`0xD0101010`), e o baú e a fornalha do vanilla seguem com o retângulo fixo do Horizon (`rgba(16, 16, 16, 160)`).

    conferir_fundo.py SAIDA_NO_DESKTOP

Roda no `horizon` aberto no mundo de teste, na caixa de concreto preto do `cena_telas` (o mundo atrás da tela é quase preto, então o valor de cada linha é o do
fundo): põe a máquina, um baú e uma fornalha lado a lado, abre cada um, tira o print e mede a coluna do canto. Código de saída 1 se algum não é o esperado.
"""
import os
import subprocess
import sys
import tempfile

import cena_telas
import comparar_tela as ct

COLUNA = 100
LINHAS = (5, 540, 1075)


def valores_do_fundo(imagem, coluna=COLUNA, linhas=LINHAS):
    """O canal vermelho do print numa coluna do canto, nas linhas de cima, do meio e de baixo."""
    largura, _, rgba = imagem
    return tuple(rgba[(y * largura + coluna) * 4] for y in linhas)


def e_retangulo(valores):
    """O retângulo fixo do Horizon dá o mesmo valor de cima a baixo (o mundo atrás é preto)."""
    return len(set(valores)) == 1


def e_degrade(valores):
    """O degradê do original escurece mais embaixo: o valor sobe de cima para baixo (12, 12 ou 13, 13 sobre o preto)."""
    return valores[0] < valores[-1] and list(valores) == sorted(valores)


def principal(destino_no_desktop):
    """Tira os três prints no desktop, traz para uma pasta temporária e confere cada um. Devolve o número de telas com o fundo errado."""
    x = 3
    caso = next(c for c in cena_telas.todos_os_casos() if c["nome"] == "chamber_vazia")
    cena_telas.comando("clear @s")
    cena_telas.iguala_as_opcoes()
    cena_telas.monta_port(caso, x, 2)
    for dx, bloco in ((-3, "minecraft:chest[facing=north]"), (3, "minecraft:furnace[facing=north]")):
        cena_telas.comando(f"setblock {x + dx} -60 3 {bloco}")
    telas = (("maquina", 0, e_degrade), ("bau", -3, e_retangulo), ("fornalha", 3, e_retangulo))
    for nome, alvo, _ in telas:
        cena_telas.ctl("tp", f"{x}.0", "-60.0", "0.0")
        cena_telas.ctl("look", "0", "0")
        cena_telas.ctl("interact", str(x + alvo), "-60", "3")
        cena_telas.ctl("wait", "60")
        cena_telas.ctl("look", "0", "0")
        cena_telas.ctl("screenshot", f"{destino_no_desktop}/fundo_{nome}.png")
        cena_telas.ctl("close")
    falhas = 0
    with tempfile.TemporaryDirectory() as pasta:
        subprocess.run(f"ssh desktop 'tar -C {destino_no_desktop} -cf - .' | tar -C {pasta} -xf -", shell=True, check=True)
        for nome, _, forma in telas:
            valores = valores_do_fundo(ct.ler_png(os.path.join(pasta, f"fundo_{nome}.png")))
            ok = forma(valores)
            falhas += not ok
            print(f"{'ok ' if ok else 'ERRO'} {nome:<9} fundo {valores} (esperado: {forma.__doc__.split(':')[0].strip().lower()})")
    return falhas


if __name__ == "__main__":
    sys.exit(1 if principal(sys.argv[1]) else 0)
